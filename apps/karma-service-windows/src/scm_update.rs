use crate::scm::{ScmHandle, wide};
use windows::{
    Win32::System::Services::{
        ControlService, OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_MANAGER_CONNECT,
        SERVICE_CONTROL_STOP, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_START, SERVICE_STATUS,
        SERVICE_STOP, SERVICE_STOPPED, StartServiceW,
    },
    core::PCWSTR,
};

/// Bounded operations for the already-authenticated update worker. The caller
/// supplies only the two compile-time Karma service names.
pub fn stop_and_wait(service_name: &str, timeout: std::time::Duration) -> std::io::Result<()> {
    transition(service_name, false, timeout)
}

pub fn start_and_wait(service_name: &str, timeout: std::time::Duration) -> std::io::Result<()> {
    transition(service_name, true, timeout)
}

fn transition(
    service_name: &str,
    start: bool,
    timeout: std::time::Duration,
) -> std::io::Result<()> {
    if !matches!(service_name, "KarmaService" | "KarmaGuardian") {
        return Err(std::io::Error::other("invalid protected service"));
    }
    let convert = |error: windows::core::Error| std::io::Error::other(error.to_string());
    let manager = ScmHandle(
        unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }
            .map_err(convert)?,
    );
    let name = wide(service_name);
    let service = ScmHandle(
        unsafe {
            OpenServiceW(
                manager.0,
                PCWSTR(name.as_ptr()),
                SERVICE_QUERY_STATUS | if start { SERVICE_START } else { SERVICE_STOP },
            )
        }
        .map_err(convert)?,
    );
    let deadline = std::time::Instant::now() + timeout;
    let desired = if start {
        SERVICE_RUNNING
    } else {
        SERVICE_STOPPED
    };
    let mut requested = false;
    loop {
        let mut state = SERVICE_STATUS::default();
        unsafe { QueryServiceStatus(service.0, &mut state) }.map_err(convert)?;
        if state.dwCurrentState == desired {
            return Ok(());
        }
        if !requested {
            if start {
                if state.dwCurrentState == SERVICE_STOPPED {
                    unsafe { StartServiceW(service.0, None) }.map_err(convert)?;
                    requested = true;
                }
            } else if state.dwCurrentState == SERVICE_RUNNING {
                unsafe { ControlService(service.0, SERVICE_CONTROL_STOP, &mut state) }
                    .map_err(convert)?;
                requested = true;
            }
        }
        if std::time::Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "protected service transition timed out",
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}
