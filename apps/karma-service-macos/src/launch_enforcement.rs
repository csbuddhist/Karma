//! Ends programs that a blocking schedule forbids from running.
//! 终止屏蔽时段内被设为“禁止启动”的应用进程。
//!
//! While a launch-blocking rule is active, the process list is read every two
//! seconds. Each blocked program is ended with `SIGKILL`, like
//! `TerminateProcess` on Windows, in every user's session. The signal goes
//! through the audit token of the instance whose image path was checked, so a
//! reused pid cannot redirect it to another program. Root and system
//! processes, macOS components, and Karma's own programs are never ended.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use karma_macos::{process, session};
use karma_policy::NetworkRestriction;
use karma_service_core::ServiceCore;

use crate::host::unix_time_ms;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const TICK: Duration = Duration::from_millis(250);

/// Directories of macOS components, which a schedule never ends.
const SYSTEM_DIRECTORIES: &[&str] = &["/System", "/usr", "/bin", "/sbin", "/Library/Apple"];
/// Apple's user applications live under `/System` but are ordinary programs
/// that a schedule may block, such as Chess or Messages. Safari runs from the
/// app cryptex.
const SYSTEM_APPLICATION_DIRECTORIES: &[&str] = &[
    "/System/Applications",
    "/System/Volumes/Preboot/Cryptexes/App/System/Applications",
];

pub fn start(
    core: Arc<ServiceCore>,
    protected_directories: Vec<PathBuf>,
    shutdown: Arc<AtomicBool>,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("karma-launch-enforcement".into())
        .spawn(move || run(&core, &protected_directories, &shutdown))
}

fn run(core: &ServiceCore, protected: &[PathBuf], shutdown: &AtomicBool) {
    let mut waited = POLL_INTERVAL;
    while !shutdown.load(Ordering::Acquire) {
        if waited >= POLL_INTERVAL {
            waited = Duration::ZERO;
            match core.network_enforcement(unix_time_ms()) {
                Ok(enforcement) if !enforcement.restriction.launch_executables.is_empty() => {
                    terminate_blocked(core, &enforcement.restriction, protected);
                }
                Ok(_) => {}
                Err(_) => {
                    eprintln!("status=degraded component=launch_enforcement error=core_unavailable")
                }
            }
        }
        thread::sleep(TICK);
        waited += TICK;
    }
}

fn terminate_blocked(core: &ServiceCore, restriction: &NetworkRestriction, protected: &[PathBuf]) {
    let own_pid = std::process::id();
    for pid in process::all_pids() {
        if pid <= 1 || pid == own_pid {
            continue;
        }
        let Ok(Some(info)) = process::inspect(pid) else {
            continue;
        };
        if !session::is_regular_user(info.uid)
            || !must_terminate(restriction, &info.executable_path, protected)
        {
            continue;
        }
        let Some(image) = info.executable_path.to_str() else {
            continue;
        };
        let terminated = terminate(pid, &info.executable_path);
        if core
            .record_launch_blocked(image, terminated, unix_time_ms())
            .is_err()
        {
            eprintln!("status=degraded component=launch_enforcement error=audit_unavailable");
        }
    }
}

/// Ends the instance running as `pid` if it still runs `image`.
fn terminate(pid: u32, image: &Path) -> bool {
    let Ok(token) = process::audit_token(pid) else {
        return false;
    };
    if process::executable_path_for(&token).as_deref() != Some(image) {
        return false;
    }
    process::signal(&token, libc::SIGKILL).is_ok()
}

/// Whether the process with image path `image` must be ended.
fn must_terminate(restriction: &NetworkRestriction, image: &Path, protected: &[PathBuf]) -> bool {
    image.is_absolute()
        && image
            .to_str()
            .is_some_and(|image| restriction.blocks_launch(image))
        && !is_system_component(image)
        && !protected
            .iter()
            .any(|directory| image.starts_with(directory))
}

fn is_system_component(image: &Path) -> bool {
    SYSTEM_DIRECTORIES
        .iter()
        .any(|directory| image.starts_with(directory))
        && !SYSTEM_APPLICATION_DIRECTORIES
            .iter()
            .any(|directory| image.starts_with(directory))
}

#[cfg(test)]
mod tests {
    use karma_policy::NetworkSchedulePolicy;
    use serde_json::json;

    use super::*;

    fn restriction() -> NetworkRestriction {
        NetworkSchedulePolicy::from_value(&json!({
            "applications": [
                {"id":"s","name":"Steam","executable":"/Applications/Steam.app/Contents/MacOS/steam_osx","category":"game","action":"content_only","enabled":true},
                {"id":"c","name":"Chess","executable":"Chess","category":"game","action":"content_only","enabled":true},
                {"id":"d","name":"Dock","executable":"Dock","category":"game","action":"content_only","enabled":true},
                {"id":"k","name":"Karma","executable":"karma-ui","category":"game","action":"content_only","enabled":true}
            ],
            "schedules": [{"id":"g", "name":"g", "action":"block_launch", "target":"categories",
                "categories":["game"], "enabled":true,
                "blocks":[{"day":0,"start":"08:00","end":"10:00"}]}]
        }))
        .unwrap()
        .restriction_at(9 * 60)
    }

    fn protected() -> Vec<PathBuf> {
        vec![
            PathBuf::from("/Library/Karma"),
            PathBuf::from("/Applications/Karma Family Protection.app"),
        ]
    }

    fn terminates(image: &str) -> bool {
        must_terminate(&restriction(), Path::new(image), &protected())
    }

    #[test]
    fn blocked_programs_are_ended_wherever_they_are_installed() {
        assert!(terminates(
            "/Applications/Steam.app/Contents/MacOS/steam_osx"
        ));
        assert!(terminates(
            "/Users/kid/Games/Chess.app/Contents/MacOS/Chess"
        ));
        assert!(!terminates(
            "/Applications/Other.app/Contents/MacOS/steam_osx"
        ));
        assert!(!terminates("Chess"));
    }

    #[test]
    fn apple_applications_may_be_blocked_but_system_components_are_not() {
        assert!(terminates(
            "/System/Applications/Chess.app/Contents/MacOS/Chess"
        ));
        assert!(terminates(
            "/System/Volumes/Preboot/Cryptexes/App/System/Applications/Chess.app/Contents/MacOS/Chess"
        ));
        assert!(!terminates(
            "/System/Library/CoreServices/Dock.app/Contents/MacOS/Dock"
        ));
        assert!(!terminates("/usr/libexec/Chess"));
        // A sibling directory that shares the prefix is not protected.
        assert!(terminates("/SystemTools/Chess"));
    }

    #[test]
    fn a_running_blocked_program_is_killed() {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("karma-blocked-probe");
        std::fs::copy("/bin/sleep", &program).unwrap();
        let mut child = std::process::Command::new(&program)
            .arg("30")
            .spawn()
            .unwrap();
        let core = ServiceCore::open(
            directory.path().join("state.db"),
            "a".repeat(64),
            directory.path().join("evidence"),
            [7; 32],
            unix_time_ms(),
        )
        .unwrap();
        let restriction = NetworkSchedulePolicy::from_value(&json!({
            "schedules": [{"id":"p", "name":"p", "action":"block_launch", "target":"applications",
                "applications":["karma-blocked-probe"], "enabled":true,
                "blocks":[{"day":0,"start":"08:00","end":"10:00"}]}]
        }))
        .unwrap()
        .restriction_at(9 * 60);

        terminate_blocked(&core, &restriction, &[]);

        let status = child.wait().unwrap();
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            Some(libc::SIGKILL)
        );
    }

    #[test]
    fn karma_is_never_ended() {
        assert!(!terminates(
            "/Applications/Karma Family Protection.app/Contents/MacOS/karma-ui"
        ));
        assert!(!terminates("/Library/Karma/karma-ui"));
        assert!(terminates("/Users/kid/bin/karma-ui"));
    }
}
