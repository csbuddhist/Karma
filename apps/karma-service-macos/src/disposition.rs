//! Closes an application the core authorized, after checking that the
//! running process is still exactly the one the agent observed.
//!
//! The first inspection captures the kernel audit token of the process
//! instance and confirms its start time, image path, and owner. `SIGTERM` and
//! the later `SIGKILL` are sent through that token, so the kernel refuses them
//! if the instance has exited and its pid was reused in the meantime.

use std::{cell::RefCell, path::PathBuf, rc::Rc, thread, time::Duration};

use karma_disposition::{
    DispositionExecutor, GraceWaiter, ProcessController, ProcessInspector, ProcessOperationError,
};
use karma_ipc::{DispositionReport, ProcessIdentity};
use karma_macos::{ProcessError, process, session};
use karma_macos_ipc::AuditToken;

/// The macOS counterpart of the Windows five-second grace period cap.
const MAXIMUM_GRACE: Duration = Duration::from_secs(5);

/// Who may be closed: only processes of the user at the console, never root,
/// system daemons, or Karma's own components.
#[derive(Debug, Clone)]
pub struct DispositionScope {
    pub console_user: Option<u32>,
    pub protected_executables: Vec<PathBuf>,
}

impl DispositionScope {
    fn permits(&self, info: &process::ProcessInfo) -> bool {
        info.pid > 1
            && session::is_regular_user(info.uid)
            && self.console_user == Some(info.uid)
            && !self
                .protected_executables
                .iter()
                .any(|protected| *protected == info.executable_path)
    }
}

pub fn execute(
    event_id: String,
    target: &ProcessIdentity,
    grace_period: Duration,
    scope: DispositionScope,
) -> DispositionReport {
    let token = Rc::new(RefCell::new(None));
    DispositionExecutor::new(
        Inspector {
            scope,
            token: Rc::clone(&token),
        },
        Controller { token },
        ThreadWaiter,
    )
    .execute(event_id, target, grace_period)
}

struct Inspector {
    scope: DispositionScope,
    /// The instance the latest successful inspection verified.
    token: Rc<RefCell<Option<AuditToken>>>,
}

impl ProcessInspector for Inspector {
    fn inspect(&self, process_id: u32) -> Result<Option<ProcessIdentity>, ProcessOperationError> {
        *self.token.borrow_mut() = None;
        let Some(info) = process::inspect(process_id).map_err(|_| ProcessOperationError)? else {
            return Ok(None);
        };
        if !self.scope.permits(&info) {
            return Err(ProcessOperationError);
        }
        let token = match process::audit_token(process_id) {
            Ok(token) => token,
            Err(_) if process::inspect(process_id).ok().flatten().is_none() => return Ok(None),
            Err(_) => return Err(ProcessOperationError),
        };
        // The token names whatever instance runs as `process_id` now; it must
        // still be the instance whose start time and image were read above.
        let Some(current) = process::inspect(process_id).map_err(|_| ProcessOperationError)? else {
            return Ok(None);
        };
        if current != info
            || process::executable_path_for(&token) != Some(info.executable_path.clone())
        {
            return Ok(Some(identity_of(&current)?));
        }
        *self.token.borrow_mut() = Some(token);
        identity_of(&info).map(Some)
    }
}

fn identity_of(info: &process::ProcessInfo) -> Result<ProcessIdentity, ProcessOperationError> {
    Ok(ProcessIdentity {
        process_id: info.pid,
        started_at_ms: info.started_at_ms,
        executable_name: info
            .executable_path
            .to_str()
            .ok_or(ProcessOperationError)?
            .to_owned(),
        executable_sha256: None,
    })
}

struct Controller {
    token: Rc<RefCell<Option<AuditToken>>>,
}

impl Controller {
    fn signal(&self, process_id: u32, signal: libc::c_int) -> Result<(), ProcessOperationError> {
        let token = (*self.token.borrow()).ok_or(ProcessOperationError)?;
        if token.pid() as u32 != process_id {
            return Err(ProcessOperationError);
        }
        match process::signal(&token, signal) {
            Ok(()) | Err(ProcessError::NotRunning) => Ok(()),
            Err(_) => Err(ProcessOperationError),
        }
    }
}

impl ProcessController for Controller {
    fn request_close(&self, process_id: u32) -> Result<(), ProcessOperationError> {
        self.signal(process_id, libc::SIGTERM)
    }

    fn terminate(&self, process_id: u32) -> Result<(), ProcessOperationError> {
        self.signal(process_id, libc::SIGKILL)
    }
}

struct ThreadWaiter;

impl GraceWaiter for ThreadWaiter {
    fn wait(&self, duration: Duration) {
        thread::sleep(duration.min(MAXIMUM_GRACE));
    }
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};

    use karma_ipc::DispositionOutcome;

    use super::*;

    fn scope() -> DispositionScope {
        DispositionScope {
            // SAFETY: getuid has no preconditions.
            console_user: Some(unsafe { libc::getuid() }),
            protected_executables: Vec::new(),
        }
    }

    fn identity_for(child: &std::process::Child) -> ProcessIdentity {
        identity_of(&process::inspect(child.id()).unwrap().unwrap()).unwrap()
    }

    fn sleeping_child() -> std::process::Child {
        Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .spawn()
            .unwrap()
    }

    #[test]
    fn the_observed_process_is_closed_gracefully() {
        // SAFETY: getuid has no preconditions.
        if !session::is_regular_user(unsafe { libc::getuid() }) {
            return;
        }
        let mut child = sleeping_child();
        let target = identity_for(&child);
        let reaper = thread::spawn(move || child.wait().unwrap());

        let report = execute(
            "event-1".into(),
            &target,
            Duration::from_millis(300),
            scope(),
        );
        assert_eq!(report.outcome, DispositionOutcome::ClosedGracefully);
        assert!(!reaper.join().unwrap().success());
    }

    #[test]
    fn a_changed_identity_is_never_signalled() {
        let mut child = sleeping_child();
        let mut target = identity_for(&child);
        target.started_at_ms -= 1000;

        let report = execute("event-1".into(), &target, Duration::ZERO, scope());
        assert!(matches!(
            report.outcome,
            DispositionOutcome::IdentityChanged | DispositionOutcome::AccessDenied
        ));
        assert!(child.try_wait().unwrap().is_none());
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn processes_outside_the_scope_are_refused() {
        let mut child = sleeping_child();
        let target = identity_for(&child);
        for scope in [
            DispositionScope {
                console_user: None,
                protected_executables: Vec::new(),
            },
            DispositionScope {
                console_user: scope().console_user,
                protected_executables: vec![PathBuf::from(&target.executable_name)],
            },
        ] {
            let report = execute("event-1".into(), &target, Duration::ZERO, scope);
            assert_eq!(report.outcome, DispositionOutcome::AccessDenied);
        }
        assert!(child.try_wait().unwrap().is_none());
        child.kill().unwrap();
        child.wait().unwrap();

        let launchd = ProcessIdentity {
            process_id: 1,
            started_at_ms: 0,
            executable_name: "/sbin/launchd".into(),
            executable_sha256: None,
        };
        assert_eq!(
            execute("event-2".into(), &launchd, Duration::ZERO, scope()).outcome,
            DispositionOutcome::AccessDenied
        );
    }
}
