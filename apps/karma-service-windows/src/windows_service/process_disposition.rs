use std::{thread, time::Duration};

use karma_disposition::{
    DispositionExecutor, GraceWaiter, ProcessController, ProcessInspector, ProcessOperationError,
};
use karma_ipc::{DispositionReport, ProcessIdentity};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, FILETIME, GetLastError, HANDLE, HWND, LPARAM,
            WPARAM,
        },
        System::Threading::{
            GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
            QueryFullProcessImageNameW, TerminateProcess,
        },
        UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE},
    },
    core::{BOOL, PWSTR},
};

const WINDOWS_TO_UNIX_EPOCH_MS: u64 = 11_644_473_600_000;
const MAX_PROCESS_PATH_U16: usize = 32_768;

pub fn execute(
    event_id: String,
    target: &ProcessIdentity,
    grace_period: Duration,
) -> DispositionReport {
    DispositionExecutor::new(
        WindowsProcessInspector,
        WindowsProcessController,
        ThreadWaiter,
    )
    .execute(event_id, target, grace_period)
}

struct WindowsProcessInspector;

impl ProcessInspector for WindowsProcessInspector {
    fn inspect(&self, process_id: u32) -> Result<Option<ProcessIdentity>, ProcessOperationError> {
        let handle =
            match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) } {
                Ok(handle) => OwnedHandle(handle),
                Err(_) if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER => return Ok(None),
                Err(_) => return Err(ProcessOperationError),
            };
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe { GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) }
            .map_err(|_| ProcessOperationError)?;
        let mut path = vec![0_u16; MAX_PROCESS_PATH_U16];
        let mut path_len = path.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(
                handle.0,
                Default::default(),
                PWSTR(path.as_mut_ptr()),
                &mut path_len,
            )
        }
        .map_err(|_| ProcessOperationError)?;
        path.truncate(path_len as usize);
        let executable_name = String::from_utf16(&path).map_err(|_| ProcessOperationError)?;
        Ok(Some(ProcessIdentity {
            process_id,
            started_at_ms: filetime_to_unix_ms(creation)?,
            executable_name,
            executable_sha256: None,
        }))
    }
}

struct WindowsProcessController;

impl ProcessController for WindowsProcessController {
    fn request_close(&self, process_id: u32) -> Result<(), ProcessOperationError> {
        let mut context = CloseContext {
            process_id,
            message_sent: false,
        };
        unsafe {
            EnumWindows(
                Some(close_matching_window),
                LPARAM((&mut context as *mut CloseContext) as isize),
            )
        }
        .map_err(|_| ProcessOperationError)?;
        if context.message_sent {
            Ok(())
        } else {
            Err(ProcessOperationError)
        }
    }

    fn terminate(&self, process_id: u32) -> Result<(), ProcessOperationError> {
        let handle = OwnedHandle(
            unsafe { OpenProcess(PROCESS_TERMINATE, false, process_id) }
                .map_err(|_| ProcessOperationError)?,
        );
        unsafe { TerminateProcess(handle.0, 1) }.map_err(|_| ProcessOperationError)
    }
}

struct ThreadWaiter;

impl GraceWaiter for ThreadWaiter {
    fn wait(&self, duration: Duration) {
        thread::sleep(duration.min(Duration::from_secs(5)));
    }
}

struct CloseContext {
    process_id: u32,
    message_sent: bool,
}

unsafe extern "system" fn close_matching_window(window: HWND, parameter: LPARAM) -> BOOL {
    let context = unsafe { &mut *(parameter.0 as *mut CloseContext) };
    let mut process_id = 0_u32;
    unsafe { GetWindowThreadProcessId(window, Some(&mut process_id)) };
    if process_id == context.process_id
        && unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok()
    {
        context.message_sent = true;
    }
    true.into()
}

fn filetime_to_unix_ms(value: FILETIME) -> Result<i64, ProcessOperationError> {
    let ticks = (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime);
    let windows_ms = ticks / 10_000;
    let unix_ms = windows_ms
        .checked_sub(WINDOWS_TO_UNIX_EPOCH_MS)
        .ok_or(ProcessOperationError)?;
    i64::try_from(unix_ms).map_err(|_| ProcessOperationError)
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
