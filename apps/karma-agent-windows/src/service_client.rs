use std::{
    env,
    mem::size_of,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use karma_ipc::{
    AgentHeartbeat, ClientKind, ComponentState, MonitorHealth, RequestEnvelope, ServiceRequest,
    ServiceResult,
};
use karma_windows::{FrameWorkerReport, FrameWorkerStatus, MonitorSnapshot};
use karma_windows_ipc::send_request;
use rand::{RngCore, rngs::OsRng};
use serde_json::Value;
use thiserror::Error;
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree},
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY, TOKEN_USER,
            TokenUser,
        },
        System::Threading::{GetCurrentProcess, GetCurrentProcessId, OpenProcessToken},
    },
    core::PWSTR,
};
use zeroize::Zeroizing;

const AGENT_TOKEN_ENV: &str = "KARMA_AGENT_TOKEN";
const IPC_TIMEOUT_MS: u32 = 1500;

#[derive(Debug, Error)]
pub enum AgentServiceError {
    #[error("agent identity is unavailable")]
    IdentityUnavailable,
    #[error("service connection is unavailable")]
    ServiceUnavailable,
    #[error("service rejected the agent")]
    AgentRejected,
    #[error("service returned an invalid response")]
    InvalidResponse,
}

pub struct AgentServiceClient {
    token: Zeroizing<String>,
    instance_id: String,
    user_sid: String,
    request_sequence: AtomicU64,
}

pub struct PolicySnapshot {
    pub revision: u64,
    pub protection_enabled: bool,
    #[allow(dead_code)]
    pub policy: Value,
}

impl AgentServiceClient {
    pub fn from_environment() -> Result<Option<Self>, AgentServiceError> {
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
        Ok(Some(Self {
            token: Zeroizing::new(token),
            instance_id: random_opaque(),
            user_sid: current_user_sid()?,
            request_sequence: AtomicU64::new(1),
        }))
    }

    pub fn publish_health(
        &self,
        monitors: Vec<MonitorHealth>,
    ) -> Result<PolicySnapshot, AgentServiceError> {
        let sent_at_ms = unix_time_ms();
        let heartbeat = AgentHeartbeat {
            agent_instance_id: self.instance_id.clone(),
            user_sid: self.user_sid.clone(),
            process_id: unsafe { GetCurrentProcessId() },
            sent_at_ms,
            monitors,
        };
        match self.request(ServiceRequest::AgentHeartbeat {
            agent_token: self.token.to_string(),
            heartbeat,
        })? {
            ServiceResult::Acknowledged => {}
            _ => return Err(AgentServiceError::InvalidResponse),
        }
        match self.request(ServiceRequest::GetAgentPolicy {
            agent_token: self.token.to_string(),
        })? {
            ServiceResult::AgentPolicy { revision, policy } => Ok(PolicySnapshot {
                revision,
                protection_enabled: policy
                    .get("protectionEnabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                policy,
            }),
            _ => Err(AgentServiceError::InvalidResponse),
        }
    }

    fn request(&self, body: ServiceRequest) -> Result<ServiceResult, AgentServiceError> {
        let sequence = self.request_sequence.fetch_add(1, Ordering::Relaxed);
        let request = RequestEnvelope::new(
            format!("{}-{sequence}", self.instance_id),
            random_opaque(),
            ClientKind::Agent,
            body,
        );
        let response = send_request(&request, IPC_TIMEOUT_MS)
            .map_err(|_| AgentServiceError::ServiceUnavailable)?;
        response.result.map_err(|failure| match failure.code {
            karma_ipc::ServiceErrorCode::AgentUnauthorized => AgentServiceError::AgentRejected,
            _ => AgentServiceError::ServiceUnavailable,
        })
    }
}

pub fn monitor_health(
    monitor: &MonitorSnapshot,
    index: usize,
    worker: FrameWorkerReport,
    image: &crate::InferenceHealthHandle,
    ocr: &crate::InferenceHealthHandle,
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

fn inference_state(inferences: u64, available: bool) -> ComponentState {
    if !available {
        ComponentState::Unavailable
    } else if inferences == 0 {
        ComponentState::Starting
    } else {
        ComponentState::Healthy
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

fn random_opaque() -> String {
    let mut bytes = [0_u8; 24];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
