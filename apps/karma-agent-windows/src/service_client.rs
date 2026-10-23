//! Windows glue for the shared agent service client: the named-pipe
//! transport, the agent token handed down by KarmaService, the signed-in
//! user's SID, and foreground-window attribution through Win32 and UI
//! Automation.

use std::{env, mem::size_of, sync::Arc, thread};

use karma_agent_core::{
    inference::InferenceHealthHandle,
    service::{
        self, AgentServiceClient, AgentServiceError, ApplicationPolicyHandle, ContextPolicyHandle,
        ForegroundContext, SourceContext, SourceIdentityProvider, inference_state, random_opaque,
        unix_time_ms,
    },
};
use karma_ipc::{
    AgentContextObservation, ComponentState, MonitorHealth, ProcessIdentity, RequestEnvelope,
    ResponseEnvelope,
};
use karma_windows::{
    AttributionResult, FrameWorkerReport, FrameWorkerStatus, MonitorSnapshot, Rect,
    SourceAttributor, WindowCandidate, WindowsRuntimeApartment, browser_host, foreground_window,
    inspect_process, window_title,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree},
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY, TOKEN_USER,
            TokenUser,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    },
    core::PWSTR,
};
use zeroize::Zeroizing;

const AGENT_TOKEN_ENV: &str = "KARMA_AGENT_TOKEN";

/// The client for the token KarmaService passed in the environment, or
/// `None` when the agent was started without one.
pub fn client_from_environment() -> Result<Option<AgentServiceClient>, AgentServiceError> {
    let Some(token) = env::var_os(AGENT_TOKEN_ENV) else {
        eprintln!("status=degraded component=service_ipc error=agent_token_missing");
        return Ok(None);
    };
    let token = token
        .into_string()
        .map_err(|_| AgentServiceError::IdentityUnavailable)?;
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AgentServiceError::IdentityUnavailable);
    }
    Ok(Some(AgentServiceClient::new(
        Zeroizing::new(token),
        current_user_sid()?,
        pipe_transport,
    )))
}

fn pipe_transport(request: &RequestEnvelope, timeout_ms: u32) -> Result<ResponseEnvelope, ()> {
    karma_windows_ipc::send_request(request, timeout_ms).map_err(drop)
}

/// Attributes a monitor's frames to the foreground window covering it.
pub struct WindowsSourceIdentityProvider {
    monitor_bounds: Rect,
    cached_process: Option<ProcessIdentity>,
}

impl WindowsSourceIdentityProvider {
    pub fn new(monitor_bounds: Rect) -> Self {
        Self {
            monitor_bounds,
            cached_process: None,
        }
    }
}

impl SourceIdentityProvider for WindowsSourceIdentityProvider {
    fn snapshot(&mut self) -> Option<SourceContext> {
        let foreground = foreground_window().ok().flatten()?;
        let candidate = WindowCandidate {
            handle: foreground.handle,
            pid: foreground.pid,
            bounds: foreground.bounds,
        };
        let AttributionResult::Reliable(attributed) =
            SourceAttributor::resolve(self.monitor_bounds, Some(foreground.pid), &[candidate])
        else {
            return None;
        };
        let title = window_title(foreground.handle);
        if self
            .cached_process
            .as_ref()
            .is_some_and(|process| process.process_id == attributed.pid)
        {
            let identity = self.cached_process.clone()?;
            return Some(SourceContext {
                identity,
                window_handle: foreground.handle,
                window_title: title,
            });
        }
        let process = inspect_process(attributed.pid).ok().flatten()?;
        let identity = ProcessIdentity {
            process_id: process.process_id,
            started_at_ms: process.started_at_ms,
            executable_name: process.executable_name,
            executable_sha256: None,
        };
        self.cached_process = Some(identity.clone());
        Some(SourceContext {
            identity,
            window_handle: foreground.handle,
            window_title: title,
        })
    }

    fn browser_host(&mut self, source: &SourceContext) -> Option<String> {
        is_supported_browser(&source.identity.executable_name)
            .then(|| browser_host(source.window_handle))
            .flatten()
    }
}

/// UI Automation needs a multithreaded apartment on the monitor thread.
struct WindowsForegroundContext {
    _runtime: WindowsRuntimeApartment,
}

impl ForegroundContext for WindowsForegroundContext {
    fn observe(&mut self, agent_instance_id: &str) -> Option<AgentContextObservation> {
        foreground_context_observation(agent_instance_id)
    }
}

pub fn start_context_monitor(
    client: Arc<AgentServiceClient>,
    policy: ContextPolicyHandle,
    applications: ApplicationPolicyHandle,
) -> thread::JoinHandle<()> {
    service::start_context_monitor(client, policy, applications, || {
        match WindowsRuntimeApartment::initialize_mta() {
            Ok(runtime) => Some(WindowsForegroundContext { _runtime: runtime }),
            Err(_) => {
                eprintln!("status=degraded component=foreground_context error=runtime_unavailable");
                None
            }
        }
    })
}

fn foreground_context_observation(agent_instance_id: &str) -> Option<AgentContextObservation> {
    let foreground = foreground_window().ok().flatten()?;
    let title = window_title(foreground.handle);
    let process = inspect_process(foreground.pid).ok().flatten()?;
    let source = ProcessIdentity {
        process_id: process.process_id,
        started_at_ms: process.started_at_ms,
        executable_name: process.executable_name,
        executable_sha256: None,
    };
    let browser_host = is_supported_browser(&source.executable_name)
        .then(|| browser_host(foreground.handle))
        .flatten();
    if title.is_empty() && browser_host.is_none() {
        return None;
    }
    Some(AgentContextObservation {
        event_id: format!("context-{}", random_opaque()),
        agent_instance_id: agent_instance_id.into(),
        occurred_at_ms: unix_time_ms(),
        source,
        window_title: title,
        browser_host,
    })
}

fn is_supported_browser(executable_name: &str) -> bool {
    let file_name = executable_name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(executable_name);
    [
        "chrome.exe",
        "msedge.exe",
        "firefox.exe",
        "brave.exe",
        "opera.exe",
        "opera_gx.exe",
        "vivaldi.exe",
        "arc.exe",
    ]
    .iter()
    .any(|browser| file_name.eq_ignore_ascii_case(browser))
}

pub fn monitor_health(
    monitor: &MonitorSnapshot,
    index: usize,
    worker: FrameWorkerReport,
    image: &InferenceHealthHandle,
    ocr: &InferenceHealthHandle,
) -> MonitorHealth {
    let image_snapshot = image.snapshot();
    let ocr_snapshot = ocr.snapshot();
    MonitorHealth {
        monitor_id: monitor.id.0.clone(),
        name: format!("显示器 {}", index.saturating_add(1)),
        width: monitor
            .bounds
            .right
            .saturating_sub(monitor.bounds.left)
            .max(1) as u32,
        height: monitor
            .bounds
            .bottom
            .saturating_sub(monitor.bounds.top)
            .max(1) as u32,
        frame_status: frame_state(worker.status()),
        image_status: inference_state(image_snapshot.inferences(), image.is_available()),
        ocr_status: inference_state(ocr_snapshot.inferences(), ocr.is_available()),
        image_inferences: image_snapshot.inferences(),
        ocr_inferences: ocr_snapshot.inferences(),
        latency_micros: image_snapshot
            .last_latency_micros()
            .max(ocr_snapshot.last_latency_micros()),
    }
}

fn frame_state(status: FrameWorkerStatus) -> ComponentState {
    match status {
        FrameWorkerStatus::Starting => ComponentState::Starting,
        FrameWorkerStatus::Running => ComponentState::Healthy,
        FrameWorkerStatus::Stopped | FrameWorkerStatus::TargetClosed => ComponentState::Stopped,
        FrameWorkerStatus::RecreateRequired
        | FrameWorkerStatus::DeviceLost
        | FrameWorkerStatus::AccessDenied
        | FrameWorkerStatus::Failed => ComponentState::Unavailable,
    }
}

fn current_user_sid() -> Result<String, AgentServiceError> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .map_err(|_| AgentServiceError::IdentityUnavailable)?;
    let result = (|| {
        let mut needed = 0_u32;
        let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut needed) };
        if needed < size_of::<TOKEN_USER>() as u32 {
            return Err(AgentServiceError::IdentityUnavailable);
        }
        let words = (needed as usize).div_ceil(size_of::<usize>());
        let mut buffer = vec![0_usize; words];
        unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                Some(buffer.as_mut_ptr().cast()),
                needed,
                &mut needed,
            )
        }
        .map_err(|_| AgentServiceError::IdentityUnavailable)?;
        let token_user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
        let mut sid = PWSTR::null();
        unsafe { ConvertSidToStringSidW(token_user.User.Sid, &mut sid) }
            .map_err(|_| AgentServiceError::IdentityUnavailable)?;
        let converted =
            unsafe { sid.to_string() }.map_err(|_| AgentServiceError::IdentityUnavailable);
        unsafe {
            let _ = LocalFree(Some(HLOCAL(sid.0.cast())));
        }
        converted
    })();
    unsafe {
        let _ = CloseHandle(token);
    }
    result
}
