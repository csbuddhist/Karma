use std::{
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use karma_ipc::{ClientKind, RequestEnvelope, ServiceRequest};
use karma_service_core::ServiceCore;
use karma_windows_ipc::{PipeServer, send_request};
use rand::{RngCore, rngs::OsRng};
use thiserror::Error;
use windows::{
    Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
            SetFileSecurityW,
        },
    },
    core::{PCWSTR, w},
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
use zeroize::Zeroizing;

const SERVICE_NAME: &str = "KarmaService";
const DATA_DIRECTORY: &str = "Karma";
const STATE_FILE: &str = "service-state.json";
const AGENT_SECRET_FILE: &str = "agent.secret";
const SERVICE_DATA_SDDL: PCWSTR = w!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)");

#[derive(Debug, Error)]
enum ServiceHostError {
    #[error("ProgramData is unavailable")]
    ProgramDataUnavailable,
    #[error("service data storage is unavailable")]
    StorageUnavailable,
}

define_windows_service!(ffi_service_main, service_main);

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    if env::args_os().any(|argument| argument == "--console") {
        let shutdown = Arc::new(AtomicBool::new(false));
        return serve(shutdown);
    }
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

fn service_main(_arguments: Vec<OsString>) {
    if let Err(error) = run_service() {
        eprintln!("KarmaService failed: {error}");
    }
}

fn run_service() -> Result<(), Box<dyn std::error::Error>> {
    let shutdown = Arc::new(AtomicBool::new(false));
    let handler_shutdown = Arc::clone(&shutdown);
    let status_handle =
        service_control_handler::register(SERVICE_NAME, move |event| match event {
            ServiceControl::Stop => {
                handler_shutdown.store(true, Ordering::Release);
                wake_pipe_server();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;

    status_handle
        .set_service_status(status(ServiceState::StartPending, Duration::from_secs(10)))?;
    status_handle.set_service_status(status(ServiceState::Running, Duration::ZERO))?;
    let result = serve(shutdown);
    status_handle.set_service_status(status(ServiceState::Stopped, Duration::ZERO))?;
    result
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

fn serve(shutdown: Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
    let directory = service_data_directory()?;
    fs::create_dir_all(&directory).map_err(|_| ServiceHostError::StorageUnavailable)?;
    harden_path(&directory)?;
    let secret_path = directory.join(AGENT_SECRET_FILE);
    let state_path = directory.join(STATE_FILE);
    let agent_secret = load_or_create_agent_secret(&secret_path)?;
    harden_path(&secret_path)?;
    if state_path.exists() {
        harden_path(&state_path)?;
    }
    let core = ServiceCore::open(state_path, agent_secret.to_string(), unix_time_ms())?;

    while !shutdown.load(Ordering::Acquire) {
        let pipe = PipeServer::create()?;
        let request = match pipe.accept_request() {
            Ok(request) => request,
            Err(error) => {
                if shutdown.load(Ordering::Acquire) {
                    break;
                }
                return Err(error.into());
            }
        };
        let shutdown_requested = matches!(request.request, ServiceRequest::RequestShutdown { .. });
        let response = core.handle(request, unix_time_ms());
        let authorized_shutdown = shutdown_requested && response.result.is_ok();
        pipe.send_response(&response)?;
        if authorized_shutdown {
            shutdown.store(true, Ordering::Release);
        }
    }
    Ok(())
}

fn harden_path(path: &PathBuf) -> Result<(), ServiceHostError> {
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            SERVICE_DATA_SDDL,
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
    }
    .map_err(|_| ServiceHostError::StorageUnavailable)?;
    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        SetFileSecurityW(
            PCWSTR(wide_path.as_ptr()),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    }
    .ok()
    .map_err(|_| ServiceHostError::StorageUnavailable);
    unsafe {
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
    }
    result
}

fn service_data_directory() -> Result<PathBuf, ServiceHostError> {
    env::var_os("ProgramData")
        .map(PathBuf::from)
        .map(|path| path.join(DATA_DIRECTORY))
        .ok_or(ServiceHostError::ProgramDataUnavailable)
}

fn load_or_create_agent_secret(path: &PathBuf) -> Result<Zeroizing<String>, ServiceHostError> {
    if path.is_file() {
        let metadata = fs::metadata(path).map_err(|_| ServiceHostError::StorageUnavailable)?;
        if metadata.len() != 64 {
            return Err(ServiceHostError::StorageUnavailable);
        }
        let mut secret = String::new();
        OpenOptions::new()
            .read(true)
            .open(path)
            .and_then(|mut file| file.read_to_string(&mut secret))
            .map_err(|_| ServiceHostError::StorageUnavailable)?;
        if !secret.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ServiceHostError::StorageUnavailable);
        }
        return Ok(Zeroizing::new(secret));
    }

    let mut bytes = Zeroizing::new([0_u8; 32]);
    OsRng.fill_bytes(bytes.as_mut());
    let mut secret = Zeroizing::new(String::with_capacity(64));
    for byte in bytes.iter() {
        use std::fmt::Write as _;
        write!(&mut *secret, "{byte:02x}").expect("writing to a String cannot fail");
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|_| ServiceHostError::StorageUnavailable)?;
    file.write_all(secret.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| ServiceHostError::StorageUnavailable)?;
    Ok(secret)
}

fn wake_pipe_server() {
    let request = RequestEnvelope::new(
        "service-stop-wakeup",
        format!("stop-{}", unix_time_ms().unsigned_abs()),
        ClientKind::Ui,
        ServiceRequest::GetBootstrap,
    );
    let _ = send_request(&request, 1000);
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
