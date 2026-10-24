//! Polls the process list while a launch-blocking schedule is active. The
//! Service runs as SYSTEM in session 0, so it can terminate the blocked
//! programs in every user session.
//! 服务以 SYSTEM 身份运行于会话 0，可终止所有用户会话中被禁止启动的程序。

use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use karma_policy::NetworkRestriction;
use karma_service_core::ServiceCore;
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            SystemInformation::GetSystemWindowsDirectoryW,
            Threading::{
                GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess,
            },
        },
    },
    core::PWSTR,
};

use super::{candidate_file_names, must_terminate};
use crate::windows_service::unix_time_ms;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_PROCESS_PATH_U16: usize = 32_768;

pub fn start(
    core: Arc<ServiceCore>,
    install_directory: &Path,
    shutdown: Arc<AtomicBool>,
) -> std::io::Result<JoinHandle<()>> {
    let install_directory = install_directory.to_string_lossy().into_owned();
    thread::Builder::new()
        .name("karma-launch-enforcement".into())
        .spawn(move || run(&core, install_directory, &shutdown))
}

fn run(core: &ServiceCore, install_directory: String, shutdown: &AtomicBool) {
    // Without the Windows directory nothing could tell system components
    // apart, so never terminate anything.
    let Some(windows_directory) = windows_directory() else {
        eprintln!("KarmaService launch enforcement: the Windows directory is unavailable");
        return;
    };
    let protected = [windows_directory, install_directory];
    while !shutdown.load(Ordering::Acquire) {
        match core.network_enforcement(unix_time_ms()) {
            Ok(enforcement) if !enforcement.restriction.launch_executables.is_empty() => {
                terminate_blocked(core, &enforcement.restriction, &protected);
            }
            Ok(_) => {}
            Err(error) => eprintln!("KarmaService launch enforcement: {error}"),
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn terminate_blocked(core: &ServiceCore, restriction: &NetworkRestriction, protected: &[String]) {
    let names = candidate_file_names(restriction);
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return;
    };
    let snapshot = OwnedHandle(snapshot);
    let own_pid = unsafe { GetCurrentProcessId() };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) }.is_ok();
    while more {
        let process_id = entry.th32ProcessID;
        // PIDs 0 and 4 are the idle and kernel System processes.
        if process_id > 4
            && process_id != own_pid
            && names.contains(&utf16_until_nul(&entry.szExeFile).to_lowercase())
        {
            if let Some((image, terminated)) =
                terminate_if_blocked(process_id, restriction, protected)
            {
                if let Err(error) = core.record_launch_blocked(&image, terminated, unix_time_ms()) {
                    eprintln!("KarmaService launch enforcement audit: {error}");
                }
            }
        }
        more = unsafe { Process32NextW(snapshot.0, &mut entry) }.is_ok();
    }
}

/// The image path of a blocked process and whether it was terminated; `None`
/// when the process is not blocked or has already exited. The image is
/// checked on the same handle that terminates it, so a reused PID cannot
/// redirect the termination to another program.
fn terminate_if_blocked(
    process_id: u32,
    restriction: &NetworkRestriction,
    protected: &[String],
) -> Option<(String, bool)> {
    let handle = match unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
            false,
            process_id,
        )
    } {
        Ok(handle) => OwnedHandle(handle),
        Err(_) => {
            // Report blocked processes that refuse termination rights.
            let query = OwnedHandle(
                unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
                    .ok()?,
            );
            let image = image_path(query.0)?;
            return must_terminate(restriction, &image, protected).then_some((image, false));
        }
    };
    let image = image_path(handle.0)?;
    if !must_terminate(restriction, &image, protected) {
        return None;
    }
    let terminated = unsafe { TerminateProcess(handle.0, 1) }.is_ok();
    Some((image, terminated))
}

fn image_path(handle: HANDLE) -> Option<String> {
    let mut path = vec![0_u16; MAX_PROCESS_PATH_U16];
    let mut path_len = path.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            handle,
            Default::default(),
            PWSTR(path.as_mut_ptr()),
            &mut path_len,
        )
    }
    .ok()?;
    path.truncate(path_len as usize);
    String::from_utf16(&path).ok()
}

/// The shared Windows directory; `GetWindowsDirectoryW` may return a
/// per-user directory under Terminal Services.
fn windows_directory() -> Option<String> {
    let mut buffer = vec![0_u16; MAX_PROCESS_PATH_U16];
    let length = unsafe { GetSystemWindowsDirectoryW(Some(&mut buffer)) } as usize;
    (length > 0 && length < buffer.len()).then(|| String::from_utf16_lossy(&buffer[..length]))
}

fn utf16_until_nul(value: &[u16]) -> String {
    let end = value
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
