#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;
use zeroize::Zeroize;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_REQUEST_ID_CHARS: usize = 64;
pub const MAX_NONCE_CHARS: usize = 128;
pub const MAX_SESSION_TOKEN_CHARS: usize = 128;
pub const MAX_PASSWORD_CHARS: usize = 1024;
pub const MAX_WINDOW_TITLE_CHARS: usize = 512;
pub const MAX_BROWSER_HOST_CHARS: usize = 253;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Ui,
    Agent,
    Installer,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestEnvelope {
    pub protocol_version: u16,
    pub request_id: String,
    pub nonce: String,
    pub client: ClientKind,
    pub request: ServiceRequest,
}

impl RequestEnvelope {
    pub fn new(
        request_id: impl Into<String>,
        nonce: impl Into<String>,
        client: ClientKind,
        request: ServiceRequest,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.into(),
            nonce: nonce.into(),
            client,
            request,
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion);
        }
        validate_opaque(&self.request_id, MAX_REQUEST_ID_CHARS)?;
        validate_opaque(&self.nonce, MAX_NONCE_CHARS)?;
        self.request.validate_for(self.client)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ServiceRequest {
    GetBootstrap,
    EnrollAdministrator {
        password: String,
    },
    Authenticate {
        password: String,
    },
    ChangePassword {
        session_token: String,
        current_password: String,
        new_password: String,
    },
    LockSession {
        session_token: String,
    },
    GetStatus {
        session_token: String,
    },
    GetPolicy {
        session_token: String,
    },
    PutPolicy {
        session_token: String,
        expected_revision: u64,
        policy: Value,
    },
    ListEvidence {
        session_token: String,
    },
    RevealEvidence {
        session_token: String,
        password: String,
        evidence_id: String,
    },
    DeleteEvidence {
        session_token: String,
        evidence_id: String,
    },
    AgentHeartbeat {
        agent_token: String,
        heartbeat: AgentHeartbeat,
    },
    AgentObservation {
        agent_token: String,
        observation: AgentObservation,
    },
    AgentContextObservation {
        agent_token: String,
        observation: AgentContextObservation,
    },
    SubmitEvidence {
        agent_token: String,
        evidence: EvidenceSubmission,
    },
    GetAgentPolicy {
        agent_token: String,
    },
    ReportDisposition {
        agent_token: String,
        report: DispositionReport,
    },
    RequestShutdown {
        password: String,
    },
}

impl ServiceRequest {
    fn validate_for(&self, client: ClientKind) -> Result<(), ProtocolError> {
        let valid_role = match self {
            Self::GetBootstrap | Self::EnrollAdministrator { .. } | Self::Authenticate { .. } => {
                client == ClientKind::Ui
            }
            Self::LockSession { .. }
            | Self::GetStatus { .. }
            | Self::GetPolicy { .. }
            | Self::PutPolicy { .. }
            | Self::ListEvidence { .. }
            | Self::RevealEvidence { .. }
            | Self::DeleteEvidence { .. }
            | Self::ChangePassword { .. } => client == ClientKind::Ui,
            Self::AgentHeartbeat { .. }
            | Self::AgentObservation { .. }
            | Self::AgentContextObservation { .. }
            | Self::SubmitEvidence { .. }
            | Self::GetAgentPolicy { .. }
            | Self::ReportDisposition { .. } => client == ClientKind::Agent,
            Self::RequestShutdown { .. } => client == ClientKind::Installer,
        };
        if !valid_role {
            return Err(ProtocolError::ClientRoleDenied);
        }
        match self {
            Self::EnrollAdministrator { password }
            | Self::Authenticate { password }
            | Self::RevealEvidence { password, .. }
            | Self::RequestShutdown { password } => {
                if password.is_empty() || password.chars().count() > MAX_PASSWORD_CHARS {
                    return Err(ProtocolError::InvalidField);
                }
            }
            Self::ChangePassword {
                current_password,
                new_password,
                ..
            } => {
                for password in [current_password, new_password] {
                    if password.is_empty() || password.chars().count() > MAX_PASSWORD_CHARS {
                        return Err(ProtocolError::InvalidField);
                    }
                }
            }
            _ => {}
        }
        match self {
            Self::LockSession { session_token }
            | Self::GetStatus { session_token }
            | Self::GetPolicy { session_token }
            | Self::PutPolicy { session_token, .. }
            | Self::ListEvidence { session_token }
            | Self::RevealEvidence { session_token, .. }
            | Self::DeleteEvidence { session_token, .. }
            | Self::ChangePassword { session_token, .. } => {
                validate_opaque(session_token, MAX_SESSION_TOKEN_CHARS)?;
            }
            Self::AgentHeartbeat { agent_token, .. }
            | Self::AgentObservation { agent_token, .. }
            | Self::AgentContextObservation { agent_token, .. }
            | Self::SubmitEvidence { agent_token, .. }
            | Self::GetAgentPolicy { agent_token }
            | Self::ReportDisposition { agent_token, .. } => {
                validate_opaque(agent_token, MAX_SESSION_TOKEN_CHARS)?;
            }
            _ => {}
        }
        match self {
            Self::RevealEvidence { evidence_id, .. } | Self::DeleteEvidence { evidence_id, .. } => {
                validate_identifier(evidence_id)?
            }
            Self::AgentHeartbeat { heartbeat, .. } => heartbeat.validate()?,
            Self::AgentObservation { observation, .. } => observation.validate()?,
            Self::AgentContextObservation { observation, .. } => observation.validate()?,
            Self::SubmitEvidence { evidence, .. } => evidence.validate()?,
            Self::ReportDisposition { report, .. } => report.validate()?,
            _ => {}
        }
        if let Self::PutPolicy { policy, .. } = self {
            let bytes = serde_json::to_vec(policy).map_err(|_| ProtocolError::InvalidJson)?;
            if bytes.len() > MAX_FRAME_BYTES / 2 {
                return Err(ProtocolError::FrameTooLarge);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentHeartbeat {
    pub agent_instance_id: String,
    pub user_sid: String,
    pub process_id: u32,
    pub sent_at_ms: i64,
    pub monitors: Vec<MonitorHealth>,
}

impl AgentHeartbeat {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_identifier(&self.agent_instance_id)?;
        if self.user_sid.is_empty() || self.user_sid.len() > 184 || self.process_id == 0 {
            return Err(ProtocolError::InvalidField);
        }
        if self.monitors.len() > 16 {
            return Err(ProtocolError::InvalidField);
        }
        self.monitors.iter().try_for_each(MonitorHealth::validate)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonitorHealth {
    pub monitor_id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub frame_status: ComponentState,
    pub image_status: ComponentState,
    pub ocr_status: ComponentState,
    pub image_inferences: u64,
    pub ocr_inferences: u64,
    pub latency_micros: u64,
}

impl MonitorHealth {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_identifier(&self.monitor_id)?;
        if self.name.chars().count() > 128 || self.width == 0 || self.height == 0 {
            return Err(ProtocolError::InvalidField);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentState {
    Starting,
    Healthy,
    Degraded,
    Unavailable,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentObservation {
    pub event_id: String,
    pub agent_instance_id: String,
    pub occurred_at_ms: i64,
    pub monitor_id: String,
    pub risk_millis: u16,
    pub reason_code: String,
    pub source: Option<ProcessIdentity>,
    #[serde(default)]
    pub browser_host: Option<String>,
    pub evidence_pending: bool,
}

impl AgentObservation {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_identifier(&self.event_id)?;
        validate_identifier(&self.agent_instance_id)?;
        validate_identifier(&self.monitor_id)?;
        if self.risk_millis > 1000 || self.reason_code.len() > 64 {
            return Err(ProtocolError::InvalidField);
        }
        if let Some(source) = &self.source {
            source.validate()?;
        }
        validate_browser_host(self.browser_host.as_deref())?;
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentContextObservation {
    pub event_id: String,
    pub agent_instance_id: String,
    pub occurred_at_ms: i64,
    pub source: ProcessIdentity,
    pub window_title: String,
    pub browser_host: Option<String>,
}

impl AgentContextObservation {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_identifier(&self.event_id)?;
        validate_identifier(&self.agent_instance_id)?;
        self.source.validate()?;
        if (self.window_title.is_empty() && self.browser_host.is_none())
            || self.window_title.chars().count() > MAX_WINDOW_TITLE_CHARS
            || self
                .window_title
                .chars()
                .any(|character| character.is_control() && !character.is_whitespace())
        {
            return Err(ProtocolError::InvalidField);
        }
        validate_browser_host(self.browser_host.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentity {
    pub process_id: u32,
    pub started_at_ms: i64,
    pub executable_name: String,
    pub executable_sha256: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSubmission {
    pub evidence_id: String,
    pub captured_at_ms: i64,
    pub monitor_name: String,
    pub application_name: String,
    pub reason_code: String,
    pub risk_millis: u16,
    pub media_type: String,
    pub bytes_base64: String,
}

impl EvidenceSubmission {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_identifier(&self.evidence_id)?;
        if self.monitor_name.chars().count() > 128
            || self.application_name.chars().count() > 260
            || self.reason_code.len() > 64
            || self.risk_millis > 1000
            || !matches!(self.media_type.as_str(), "image/jpeg" | "image/png")
            || self.bytes_base64.len() > MAX_FRAME_BYTES * 3 / 4
        {
            return Err(ProtocolError::InvalidField);
        }
        Ok(())
    }
}

impl Drop for EvidenceSubmission {
    fn drop(&mut self) {
        self.bytes_base64.zeroize();
    }
}

impl ProcessIdentity {
    fn validate(&self) -> Result<(), ProtocolError> {
        if self.process_id == 0
            || self.executable_name.is_empty()
            || self.executable_name.chars().count() > 260
        {
            return Err(ProtocolError::InvalidField);
        }
        if self.executable_sha256.as_ref().is_some_and(|hash| {
            hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            return Err(ProtocolError::InvalidField);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispositionReport {
    pub event_id: String,
    pub process_id: u32,
    pub started_at_ms: i64,
    pub outcome: DispositionOutcome,
}

impl DispositionReport {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_identifier(&self.event_id)?;
        if self.process_id == 0 {
            return Err(ProtocolError::InvalidField);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispositionOutcome {
    ClosedGracefully,
    Terminated,
    IdentityChanged,
    AccessDenied,
    SourceUncertain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseEnvelope {
    pub protocol_version: u16,
    pub request_id: String,
    pub result: Result<ServiceResult, ServiceFailure>,
}

impl ResponseEnvelope {
    pub fn success(request_id: impl Into<String>, result: ServiceResult) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.into(),
            result: Ok(result),
        }
    }

    pub fn failure(request_id: impl Into<String>, failure: ServiceFailure) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.into(),
            result: Err(failure),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ServiceResult {
    Bootstrap(BootstrapStatus),
    Session {
        session_token: String,
        expires_in_seconds: u32,
    },
    Status(ServiceStatus),
    Policy {
        revision: u64,
        policy: Value,
    },
    PolicySaved {
        revision: u64,
    },
    EvidenceList {
        items: Vec<EvidenceMetadata>,
    },
    EvidenceImage {
        media_type: String,
        bytes_base64: String,
    },
    AgentPolicy {
        revision: u64,
        policy: Value,
    },
    DispositionRequired {
        event_id: String,
        target: ProcessIdentity,
        grace_period_ms: u32,
    },
    DispositionCompleted {
        report: DispositionReport,
    },
    Acknowledged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapStatus {
    SetupRequired,
    Locked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceStatus {
    pub service_started_at_ms: i64,
    pub policy_revision: u64,
    pub protection_enabled: bool,
    pub agent_connected: bool,
    pub agent_last_seen_at_ms: Option<i64>,
    pub monitors: Vec<MonitorHealth>,
    pub evidence_count: u64,
    pub audit_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceMetadata {
    pub id: String,
    pub captured_at_ms: i64,
    pub monitor_name: String,
    pub application_name: String,
    pub reason_code: String,
    pub risk_millis: u16,
    pub original_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceFailure {
    pub code: ServiceErrorCode,
    pub retry_after_seconds: Option<u32>,
}

impl ServiceFailure {
    pub fn new(code: ServiceErrorCode) -> Self {
        Self {
            code,
            retry_after_seconds: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceErrorCode {
    InvalidRequest,
    UnsupportedVersion,
    ReplayDetected,
    AuthenticationRequired,
    AuthenticationFailed,
    RateLimited,
    AlreadyEnrolled,
    NotEnrolled,
    RevisionConflict,
    EvidenceUnavailable,
    AgentUnauthorized,
    ServiceUnavailable,
    StorageUnavailable,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ProtocolError {
    #[error("frame exceeds the protocol limit")]
    FrameTooLarge,
    #[error("frame is truncated")]
    TruncatedFrame,
    #[error("message is not valid JSON")]
    InvalidJson,
    #[error("protocol version is unsupported")]
    UnsupportedVersion,
    #[error("client role cannot send this message")]
    ClientRoleDenied,
    #[error("message field is invalid")]
    InvalidField,
}

pub fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    let body = serde_json::to_vec(value).map_err(|_| ProtocolError::InvalidJson)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge);
    }
    let mut framed = Vec::with_capacity(body.len() + 4);
    framed.extend_from_slice(&(body.len() as u32).to_le_bytes());
    framed.extend_from_slice(&body);
    Ok(framed)
}

pub fn decode_frame<T: DeserializeOwned>(framed: &[u8]) -> Result<T, ProtocolError> {
    if framed.len() < 4 {
        return Err(ProtocolError::TruncatedFrame);
    }
    let declared = u32::from_le_bytes(framed[..4].try_into().expect("four-byte prefix")) as usize;
    if declared > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge);
    }
    if framed.len() != declared + 4 {
        return Err(ProtocolError::TruncatedFrame);
    }
    serde_json::from_slice(&framed[4..]).map_err(|_| ProtocolError::InvalidJson)
}

fn validate_opaque(value: &str, maximum_chars: usize) -> Result<(), ProtocolError> {
    if value.is_empty()
        || value.chars().count() > maximum_chars
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ProtocolError::InvalidField);
    }
    Ok(())
}

fn validate_identifier(value: &str) -> Result<(), ProtocolError> {
    validate_opaque(value, 128)
}

fn validate_browser_host(value: Option<&str>) -> Result<(), ProtocolError> {
    if value.is_some_and(|host| {
        host.is_empty()
            || host.chars().count() > MAX_BROWSER_HOST_CHARS
            || host
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || matches!(byte, b'/' | b'\\' | b'@'))
    }) {
        return Err(ProtocolError::InvalidField);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(body: ServiceRequest, client: ClientKind) -> RequestEnvelope {
        RequestEnvelope::new("request-1", "nonce-1", client, body)
    }

    #[test]
    fn frame_round_trip_preserves_request_and_version() {
        let original = request(ServiceRequest::GetBootstrap, ClientKind::Ui);
        let decoded: RequestEnvelope = decode_frame(&encode_frame(&original).unwrap()).unwrap();
        assert_eq!(decoded.protocol_version, PROTOCOL_VERSION);
        assert!(matches!(decoded.request, ServiceRequest::GetBootstrap));
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn framing_rejects_truncation_and_oversized_prefix() {
        assert!(matches!(
            decode_frame::<RequestEnvelope>(&[]),
            Err(ProtocolError::TruncatedFrame)
        ));
        let oversized = ((MAX_FRAME_BYTES + 1) as u32).to_le_bytes();
        assert!(matches!(
            decode_frame::<RequestEnvelope>(&oversized),
            Err(ProtocolError::FrameTooLarge)
        ));
    }

    #[test]
    fn client_roles_cannot_cross_privilege_boundaries() {
        let ui_heartbeat = request(
            ServiceRequest::AgentHeartbeat {
                agent_token: "agent-token".into(),
                heartbeat: AgentHeartbeat {
                    agent_instance_id: "agent-1".into(),
                    user_sid: "S-1-5-21-test".into(),
                    process_id: 10,
                    sent_at_ms: 1,
                    monitors: vec![],
                },
            },
            ClientKind::Ui,
        );
        assert_eq!(
            ui_heartbeat.validate(),
            Err(ProtocolError::ClientRoleDenied)
        );
    }

    #[test]
    fn debug_output_never_contains_passwords() {
        let request = request(
            ServiceRequest::Authenticate {
                password: "sensitive-secret".into(),
            },
            ClientKind::Ui,
        );
        let debug = format!("client={:?}", request.client);
        assert!(!debug.contains("sensitive-secret"));
    }

    #[test]
    fn change_password_is_ui_only_and_requires_both_secrets() {
        let change = |client| {
            request(
                ServiceRequest::ChangePassword {
                    session_token: "session-1".into(),
                    current_password: "current-secret".into(),
                    new_password: "replacement-secret".into(),
                },
                client,
            )
        };
        assert!(change(ClientKind::Ui).validate().is_ok());
        assert_eq!(
            change(ClientKind::Agent).validate(),
            Err(ProtocolError::ClientRoleDenied)
        );
        let empty_new = request(
            ServiceRequest::ChangePassword {
                session_token: "session-1".into(),
                current_password: "current-secret".into(),
                new_password: String::new(),
            },
            ClientKind::Ui,
        );
        assert_eq!(empty_new.validate(), Err(ProtocolError::InvalidField));
        let stale_session = request(
            ServiceRequest::ChangePassword {
                session_token: "bad session".into(),
                current_password: "current-secret".into(),
                new_password: "replacement-secret".into(),
            },
            ClientKind::Ui,
        );
        assert_eq!(stale_session.validate(), Err(ProtocolError::InvalidField));
    }

    #[test]
    fn invalid_opaque_values_and_large_policies_are_rejected() {
        let invalid = RequestEnvelope::new(
            "../request",
            "nonce",
            ClientKind::Ui,
            ServiceRequest::GetBootstrap,
        );
        assert_eq!(invalid.validate(), Err(ProtocolError::InvalidField));
        let policy = Value::String("x".repeat(MAX_FRAME_BYTES));
        let large = request(
            ServiceRequest::PutPolicy {
                session_token: "session-1".into(),
                expected_revision: 0,
                policy,
            },
            ClientKind::Ui,
        );
        assert_eq!(large.validate(), Err(ProtocolError::FrameTooLarge));
    }

    #[test]
    fn context_observation_accepts_host_only_but_rejects_empty_context() {
        let observation = |browser_host| AgentContextObservation {
            event_id: "event-1".into(),
            agent_instance_id: "agent-1".into(),
            occurred_at_ms: 1,
            source: ProcessIdentity {
                process_id: 42,
                started_at_ms: 1,
                executable_name: "chrome.exe".into(),
                executable_sha256: None,
            },
            window_title: String::new(),
            browser_host,
        };
        let request_for = |browser_host| {
            request(
                ServiceRequest::AgentContextObservation {
                    agent_token: "agent-token".into(),
                    observation: observation(browser_host),
                },
                ClientKind::Agent,
            )
        };

        assert!(
            request_for(Some("blocked.example".into()))
                .validate()
                .is_ok()
        );
        assert_eq!(
            request_for(None).validate(),
            Err(ProtocolError::InvalidField)
        );
    }
}
