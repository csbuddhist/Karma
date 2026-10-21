//! Minimal Service Control Manager helpers shared by KarmaService's mirror
//! watchdog and the KarmaGuardian companion service.
//!
//! Both callers run as LocalSystem, so they may query and start a peer service
//! directly through the SCM. SCM crash-recovery actions only fire on an abnormal
//! exit, so a clean `sc stop` / Services.msc "Stop" leaves nothing to restart the
//! peer. These helpers close that gap by polling and restarting on a stopped
//! peer, which is the self-binding guarantee the install depends on.

use windows::{
    Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_HANDLE,
        SC_MANAGER_CONNECT, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_START,
        SERVICE_START_PENDING, SERVICE_STATUS, StartServiceW,
    },
    core::PCWSTR,
};

/// RAII wrapper so every SCM handle is released on the way out, including the
/// early returns below.
pub(crate) struct ScmHandle(pub(crate) SC_HANDLE);

impl Drop for ScmHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseServiceHandle(self.0);
        }
    }
}

pub(crate) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

/// Start `service_name` if it is installed but currently stopped.
///
/// Every failure is intentionally swallowed: the watchdogs poll on a fixed
/// interval, so a transient SCM error (or a peer that is mid-transition) simply
/// retries on the next pass rather than taking the watchdog down.
pub fn ensure_running(service_name: &str) {
    let manager =
        match unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) } {
            Ok(handle) => ScmHandle(handle),
            Err(_) => return,
        };
    let name = wide(service_name);
    let service = match unsafe {
        OpenServiceW(
            manager.0,
            PCWSTR(name.as_ptr()),
            SERVICE_QUERY_STATUS | SERVICE_START,
        )
    } {
        Ok(handle) => ScmHandle(handle),
        Err(_) => return,
    };
    let mut service_status = SERVICE_STATUS::default();
    if unsafe { QueryServiceStatus(service.0, &mut service_status) }.is_err() {
        return;
    }
    if service_status.dwCurrentState == SERVICE_RUNNING
        || service_status.dwCurrentState == SERVICE_START_PENDING
    {
        return;
    }
    let _ = unsafe { StartServiceW(service.0, None) };
}
