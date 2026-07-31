use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, STILL_ACTIVE},
        System::{
            RemoteDesktop::{WTSGetActiveConsoleSessionId, WTSQueryUserToken},
            Threading::{
                CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW,
                GetExitCodeProcess, PROCESS_INFORMATION, STARTUPINFOW, TerminateProcess,
            },
        },
    },
    core::{PCWSTR, PWSTR},
};
use zeroize::Zeroizing;

const AGENT_EXE: &str = "karma-agent-windows.exe";
const RESTART_DELAY: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_secs(1);

pub fn start(
    install_directory: PathBuf,
    agent_token: String,
    shutdown: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("karma-agent-watchdog".into())
        .spawn(move || run(&install_directory, Zeroizing::new(agent_token), &shutdown))
        .expect("KarmaService failed to start its Agent watchdog")
}

fn run(install_directory: &Path, token: Zeroizing<String>, shutdown: &AtomicBool) {
    let mut child: Option<OwnedProcess> = None;
    while !shutdown.load(Ordering::Acquire) {
        let active_session = unsafe { WTSGetActiveConsoleSessionId() };
        let needs_restart = child
            .as_ref()
            .is_none_or(|process| !process.is_running() || process.session_id != active_session);
        if needs_restart {
            if let Some(process) = child.take() {
                process.terminate();
            }
            match launch_agent(install_directory, &token) {
                Ok(process) => child = Some(process),
                Err(()) => thread::sleep(RESTART_DELAY),
            }
        }
        thread::sleep(POLL_INTERVAL);
    }
    if let Some(process) = child {
        process.terminate();
    }
}

fn launch_agent(install_directory: &Path, token: &str) -> Result<OwnedProcess, ()> {
    let session_id = unsafe { WTSGetActiveConsoleSessionId() };
    if session_id == u32::MAX {
        return Err(());
    }
    let mut user_token = HANDLE::default();
    unsafe { WTSQueryUserToken(session_id, &mut user_token) }.map_err(|_| ())?;
    let user_token = OwnedHandle(user_token);

    let agent_path = install_directory.join(AGENT_EXE);
    if !agent_path.is_file() {
        return Err(());
    }
    let application = wide(agent_path.as_os_str());
    let current_directory = wide(install_directory.as_os_str());
    let environment = environment_block(install_directory, token);
    let mut desktop: Vec<u16> = "winsta0\\default".encode_utf16().chain(Some(0)).collect();
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        lpDesktop: PWSTR(desktop.as_mut_ptr()),
        ..Default::default()
    };
    let mut information = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessAsUserW(
            Some(user_token.0),
            PCWSTR(application.as_ptr()),
            None,
            None,
            None,
            false,
            CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
            Some(environment.as_ptr().cast()),
            PCWSTR(current_directory.as_ptr()),
            &startup,
            &mut information,
        )
    }
    .map_err(|_| ())?;
    unsafe {
        let _ = CloseHandle(information.hThread);
    }
    Ok(OwnedProcess {
        handle: OwnedHandle(information.hProcess),
        session_id,
    })
}

fn environment_block(install_directory: &Path, token: &str) -> Zeroizing<Vec<u16>> {
    let entries = [
        format!("KARMA_AGENT_TOKEN={token}"),
        format!(
            "KARMA_IMAGE_MODEL_MANIFEST={}",
            install_directory
                .join("models/image/viddexa-nano/manifest.json")
                .display()
        ),
        format!(
            "KARMA_OCR_LIGHTWEIGHT_MANIFEST={}",
            install_directory
                .join("models/ocr/pp-ocrv5-mobile/manifest.json")
                .display()
        ),
        "KARMA_OCR_PROFILE=auto".into(),
    ];
    let mut block = Vec::new();
    for entry in entries {
        block.extend(entry.encode_utf16());
        block.push(0);
    }
    block.push(0);
    Zeroizing::new(block)
}

fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct OwnedProcess {
    handle: OwnedHandle,
    session_id: u32,
}

impl OwnedProcess {
    fn is_running(&self) -> bool {
        let mut exit_code = 0_u32;
        unsafe { GetExitCodeProcess(self.handle.0, &mut exit_code) }.is_ok()
            && exit_code == STILL_ACTIVE.0 as u32
    }

    fn terminate(self) {
        unsafe {
            let _ = TerminateProcess(self.handle.0, 0);
        }
    }
}
