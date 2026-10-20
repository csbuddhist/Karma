//! KarmaGuardian — a companion Windows service that keeps KarmaService running.
//!
//! KarmaService registers SCM crash-recovery actions, but Windows only triggers
//! those on an abnormal exit. A clean `sc stop` / Services.msc "Stop" is not a
//! failure, so nothing restarts KarmaService after a manual stop. KarmaGuardian
//! polls KarmaService and starts it again whenever it is found stopped.
//!
//! KarmaService runs the mirror watchdog for KarmaGuardian (see
//! `windows_service::service_watchdog`), so stopping either service is recovered
//! by the other. Stopping both at once remains defeatable by a local
//! administrator, which the installation guide already documents.

#[cfg(windows)]
mod scm;

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    guardian::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("KarmaGuardian can only run on Windows");
}

#[cfg(windows)]
mod guardian {
    use std::{
        env,
        ffi::OsString,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::Duration,
    };

    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };

    const SERVICE_NAME: &str = "KarmaGuardian";
    const PROTECTED_SERVICE: &str = "KarmaService";
    /// How often the protected service is checked. The service ACL already denies
    /// a clean stop, so this is a fast backstop for the kill vector (End task):
    /// it re-starts KarmaService even after SCM's own restart budget is spent.
    const CHECK_INTERVAL: Duration = Duration::from_secs(5);
    /// Fine-grained sleep so a stop request is honored within a second instead of
    /// blocking for the full check interval.
    const TICK: Duration = Duration::from_secs(1);

    define_windows_service!(ffi_service_main, service_main);

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        if env::args_os().any(|argument| argument == "--console") {
            let shutdown = Arc::new(AtomicBool::new(false));
            guard_loop(&shutdown);
            return Ok(());
        }
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
        Ok(())
    }

    fn service_main(_arguments: Vec<OsString>) {
        if let Err(error) = run_service() {
            eprintln!("KarmaGuardian failed: {error}");
        }
    }

    fn run_service() -> Result<(), Box<dyn std::error::Error>> {
        let shutdown = Arc::new(AtomicBool::new(false));
        let handler_shutdown = Arc::clone(&shutdown);
        let status_handle =
            service_control_handler::register(SERVICE_NAME, move |event| match event {
                ServiceControl::Stop => {
                    handler_shutdown.store(true, Ordering::Release);
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            })?;

        status_handle
            .set_service_status(status(ServiceState::StartPending, Duration::from_secs(10)))?;
        status_handle.set_service_status(status(ServiceState::Running, Duration::ZERO))?;
        guard_loop(&shutdown);
        status_handle.set_service_status(status(ServiceState::Stopped, Duration::ZERO))?;
        Ok(())
    }

    /// Check the protected service once on entry, then every `CHECK_INTERVAL`,
    /// waking promptly when `shutdown` is set.
    fn guard_loop(shutdown: &AtomicBool) {
        let mut elapsed = CHECK_INTERVAL;
        while !shutdown.load(Ordering::Acquire) {
            if elapsed >= CHECK_INTERVAL {
                crate::scm::ensure_running(PROTECTED_SERVICE);
                elapsed = Duration::ZERO;
            }
            thread::sleep(TICK);
            elapsed = elapsed.saturating_add(TICK);
        }
    }

    fn status(current_state: ServiceState, wait_hint: Duration) -> ServiceStatus {
        ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state,
            controls_accepted: if current_state == ServiceState::Running {
                ServiceControlAccept::STOP
            } else {
                ServiceControlAccept::empty()
            },
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint,
            process_id: None,
        }
    }
}
