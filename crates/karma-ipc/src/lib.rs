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
/// Largest page of remembered titles one console request may read.
pub const MAX_TITLE_PAGE: u32 = 100;
pub const MAX_TITLE_QUERY_CHARS: usize = 128;
/// Longest executable path an agent may report, and so the longest ban key.
pub const MAX_EXECUTABLE_CHARS: usize = 260;
/// Most evidence items one `DeleteEvidenceBatch` request may name.
pub const MAX_EVIDENCE_BATCH: usize = 200;
/// The product version this build reports and updates from. Kept equal to the
/// console's `tauri.conf.json` version by the installer contract test.
pub const PRODUCT_VERSION: &str = "0.3.2";

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
    /// Replace the policy. When a super password is configured and the new
    /// policy weakens protection, `super_password` must carry it.
    PutPolicy {
        session_token: String,
        expected_revision: u64,
        policy: Value,
        #[serde(default)]
        super_password: Option<String>,
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
    /// Delete several evidence items at once. Items that no longer exist are
    /// skipped; the request fails only when none of them exists.
    DeleteEvidenceBatch {
        session_token: String,
        evidence_ids: Vec<String>,
    },
    /// One page of remembered window titles, newest first, optionally
    /// filtered by a case-insensitive query, plus active repeat-offender bans.
    ListEnforcement {
        session_token: String,
        #[serde(default)]
        title_offset: u32,
        #[serde(default = "default_title_page")]
        title_limit: u32,
        #[serde(default)]
        title_query: Option<String>,
        /// Page the titles marked as misjudged instead of remembered titles.
        #[serde(default)]
        misjudged: bool,
    },
    /// Stop closing windows that show one remembered title. Requires the
    /// super password once one is configured.
    ForgetRememberedTitle {
        session_token: String,
        title_id: String,
        #[serde(default)]
        super_password: Option<String>,
    },
    /// Always allow one currently banned executable and lift its ban, in one
    /// step against `expected_revision`. Requires the super password once one
    /// is configured.
    AllowBannedApplication {
        session_token: String,
        expected_revision: u64,
        executable: String,
        #[serde(default)]
        super_password: Option<String>,
    },
    /// Always allow the executable an evidence item was attributed to, and
    /// lift any ban on it, in one step against `expected_revision`. The
    /// Service reads the executable from its own evidence record, so the
    /// console never names a path. Requires the super password once one is
    /// configured.
    AllowEvidenceApplication {
        session_token: String,
        expected_revision: u64,
        evidence_id: String,
        #[serde(default)]
        super_password: Option<String>,
    },
    /// Forget one remembered title and never remember it again, because image
    /// recognition misjudged it. Requires the super password once one is
    /// configured.
    MarkTitleMisjudged {
        session_token: String,
        title_id: String,
        #[serde(default)]
        super_password: Option<String>,
    },
    /// Let a title marked as misjudged be remembered again.
    RestoreMisjudgedTitle {
        session_token: String,
        title_id: String,
    },
    /// Lift the active network schedule until its current block ends. The
    /// password is verified again even with a valid session: the super
    /// password once one is configured, otherwise the administrator password.
    RestoreNetwork {
        session_token: String,
        password: String,
    },
    /// Configure the super password for the first time, confirmed with the
    /// administrator password.
    SetSuperPassword {
        session_token: String,
        admin_password: String,
        super_password: String,
    },
    ChangeSuperPassword {
        session_token: String,
        current_super_password: String,
        new_super_password: String,
    },
    /// Start the waiting period after which the administrator password alone
    /// may authorize an uninstall.
    RequestUninstall {
        session_token: String,
        password: String,
    },
    CancelUninstall {
        session_token: String,
    },
    /// Check for, download, and install a newer signed release now.
    CheckForUpdates {
        session_token: String,
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
    /// Authorize an uninstall: the super password, or the administrator
    /// password once a requested uninstall waiting period has elapsed.
    RequestShutdown {
        password: String,
    },
    /// Apply the signed bundle an installer placed in the update inbox, in
    /// place and without removing protection; the administrator password
    /// suffices because only newer, signed releases are accepted.
    ApplyStagedUpdate {
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
            | Self::DeleteEvidenceBatch { .. }
            | Self::ListEnforcement { .. }
            | Self::ForgetRememberedTitle { .. }
            | Self::AllowBannedApplication { .. }
            | Self::AllowEvidenceApplication { .. }
            | Self::MarkTitleMisjudged { .. }
            | Self::RestoreMisjudgedTitle { .. }
            | Self::RestoreNetwork { .. }
            | Self::SetSuperPassword { .. }
            | Self::ChangeSuperPassword { .. }
            | Self::RequestUninstall { .. }
            | Self::CancelUninstall { .. }
            | Self::CheckForUpdates { .. }
            | Self::ChangePassword { .. } => client == ClientKind::Ui,
            Self::AgentHeartbeat { .. }
            | Self::AgentObservation { .. }
            | Self::AgentContextObservation { .. }
            | Self::SubmitEvidence { .. }
            | Self::GetAgentPolicy { .. }
            | Self::ReportDisposition { .. } => client == ClientKind::Agent,
            Self::RequestShutdown { .. } | Self::ApplyStagedUpdate { .. } => {
                client == ClientKind::Installer
            }
        };
        if !valid_role {
            return Err(ProtocolError::ClientRoleDenied);
        }
        match self {
            Self::EnrollAdministrator { password }
            | Self::Authenticate { password }
            | Self::RevealEvidence { password, .. }
            | Self::RestoreNetwork { password, .. }
            | Self::RequestUninstall { password, .. }
            | Self::RequestShutdown { password }
            | Self::ApplyStagedUpdate { password } => validate_password(password)?,
            Self::ChangePassword {
                current_password: first,
                new_password: second,
                ..
            }
            | Self::SetSuperPassword {
                admin_password: first,
                super_password: second,
                ..
            }
            | Self::ChangeSuperPassword {
                current_super_password: first,
                new_super_password: second,
                ..
            } => {
                validate_password(first)?;
                validate_password(second)?;
            }
            Self::PutPolicy {
                super_password: Some(password),
                ..
            }
            | Self::ForgetRememberedTitle {
                super_password: Some(password),
                ..
            }
            | Self::AllowBannedApplication {
                super_password: Some(password),
                ..
            }
            | Self::AllowEvidenceApplication {
                super_password: Some(password),
                ..
            }
            | Self::MarkTitleMisjudged {
                super_password: Some(password),
                ..
            } => validate_password(password)?,
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
            | Self::DeleteEvidenceBatch { session_token, .. }
            | Self::ListEnforcement { session_token, .. }
            | Self::ForgetRememberedTitle { session_token, .. }
            | Self::AllowBannedApplication { session_token, .. }
            | Self::AllowEvidenceApplication { session_token, .. }
            | Self::MarkTitleMisjudged { session_token, .. }
            | Self::RestoreMisjudgedTitle { session_token, .. }
            | Self::RestoreNetwork { session_token, .. }
            | Self::SetSuperPassword { session_token, .. }
            | Self::ChangeSuperPassword { session_token, .. }
            | Self::RequestUninstall { session_token, .. }
            | Self::CancelUninstall { session_token }
            | Self::CheckForUpdates { session_token }
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
            Self::RevealEvidence { evidence_id, .. }
            | Self::DeleteEvidence { evidence_id, .. }
            | Self::AllowEvidenceApplication { evidence_id, .. } => {
                validate_identifier(evidence_id)?
            }
            Self::DeleteEvidenceBatch { evidence_ids, .. } => {
                if evidence_ids.is_empty() || evidence_ids.len() > MAX_EVIDENCE_BATCH {
                    return Err(ProtocolError::InvalidField);
                }
                let mut unique = std::collections::HashSet::with_capacity(evidence_ids.len());
                for evidence_id in evidence_ids {
                    validate_identifier(evidence_id)?;
                    if !unique.insert(evidence_id.as_str()) {
                        return Err(ProtocolError::InvalidField);
                    }
                }
            }
            Self::ForgetRememberedTitle { title_id, .. }
            | Self::MarkTitleMisjudged { title_id, .. }
            | Self::RestoreMisjudgedTitle { title_id, .. } => validate_identifier(title_id)?,
            Self::AllowBannedApplication { executable, .. } => {
                if executable.trim().is_empty()
                    || executable.chars().count() > MAX_EXECUTABLE_CHARS
                    || executable.chars().any(char::is_control)
                {
                    return Err(ProtocolError::InvalidField);
                }
            }
            Self::ListEnforcement {
                title_limit,
                title_query,
                ..
            } => {
                if !(1..=MAX_TITLE_PAGE).contains(title_limit)
                    || title_query.as_ref().is_some_and(|query| {
                        query.chars().count() > MAX_TITLE_QUERY_CHARS
                            || query.chars().any(char::is_control)
                    })
                {
                    return Err(ProtocolError::InvalidField);
                }
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
    /// Title of the attributed window when the frame was captured.
    #[serde(default)]
    pub window_title: Option<String>,
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
        if let Some(title) = &self.window_title {
            validate_window_title(title)?;
        }
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
        if self.window_title.is_empty() && self.browser_host.is_none() {
            return Err(ProtocolError::InvalidField);
        }
        validate_window_title(&self.window_title)?;
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
            || self.executable_name.chars().count() > MAX_EXECUTABLE_CHARS
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
        #[serde(default)]
        enforcement: AgentEnforcement,
    },
    DispositionRequired {
        event_id: String,
        target: ProcessIdentity,
        grace_period_ms: u32,
    },
    DispositionCompleted {
        report: DispositionReport,
    },
    NetworkStatus(NetworkStatus),
    Enforcement(EnforcementSummary),
    Acknowledged,
}

fn default_title_page() -> u32 {
    50
}

/// Service-derived enforcement state the agent applies to foreground
/// windows in addition to the policy. Empty while protection is disabled.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentEnforcement {
    /// Normalized executable paths that are closed whenever observed.
    pub banned_executables: Vec<String>,
    /// `karma_policy::title_digest` of each remembered executable and title;
    /// the titles themselves stay in the service.
    pub remembered_title_digests: Vec<String>,
}

/// What the console shows about learned enforcement.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnforcementSummary {
    /// The requested page of remembered titles.
    pub remembered_titles: Vec<RememberedTitleMetadata>,
    /// Titles in the requested list matching the query, across all pages.
    pub remembered_title_total: u64,
    pub active_bans: Vec<ActiveBan>,
    /// All titles marked as misjudged, regardless of the query.
    #[serde(default)]
    pub misjudged_title_total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RememberedTitleMetadata {
    pub id: String,
    pub executable: String,
    pub title: String,
    pub browser_host: Option<String>,
    /// When it was remembered, or marked as misjudged.
    pub remembered_at_ms: i64,
    /// The recognition result that remembered it; absent for titles from
    /// builds that did not record one, and for misjudged titles.
    #[serde(default)]
    pub reason: Option<TitleReasonMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TitleReasonMetadata {
    pub reason_code: String,
    pub risk_millis: u16,
    pub threshold_millis: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveBan {
    pub executable: String,
    pub banned_until_ms: i64,
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
    #[serde(default)]
    pub network: NetworkStatus,
    #[serde(default)]
    pub product_version: String,
    #[serde(default)]
    pub super_password_set: bool,
    #[serde(default)]
    pub uninstall: Option<UninstallStatus>,
    #[serde(default)]
    pub update: UpdateStatus,
}

/// A requested uninstall: the administrator password may authorize it from
/// `effective_at_ms` until `expires_at_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UninstallStatus {
    pub requested_at_ms: i64,
    pub effective_at_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    #[default]
    Idle,
    Checking,
    Downloading,
    Installing,
    UpToDate,
    Failed,
}

/// Progress of the signed in-place updater.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateStatus {
    pub phase: UpdatePhase,
    pub last_checked_at_ms: Option<i64>,
    /// Newest version the last check found, when newer than this build.
    pub available_version: Option<String>,
    /// Short machine-readable reason for the last failure.
    pub last_error: Option<String>,
}

/// The weekly network schedule as the Service currently enforces it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkStatus {
    /// A schedule block is active and has not been lifted by an administrator.
    pub restricted: bool,
    /// Identifiers of the schedule rules whose blocks are active right now.
    pub active_rule_ids: Vec<String>,
    /// When the active blocks end (Unix milliseconds).
    pub restricted_until_ms: Option<i64>,
    /// When an administrator restore expires (Unix milliseconds).
    pub restored_until_ms: Option<i64>,
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
    /// The change weakens protection and needs the super password.
    SuperPasswordRequired,
    /// The offered update is not a newer, correctly signed release.
    UpdateRejected,
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

fn validate_password(password: &str) -> Result<(), ProtocolError> {
    if password.is_empty() || password.chars().count() > MAX_PASSWORD_CHARS {
        return Err(ProtocolError::InvalidField);
    }
    Ok(())
}

fn validate_window_title(title: &str) -> Result<(), ProtocolError> {
    if title.chars().count() > MAX_WINDOW_TITLE_CHARS
        || title
            .chars()
            .any(|character| character.is_control() && !character.is_whitespace())
    {
        return Err(ProtocolError::InvalidField);
    }
    Ok(())
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
    fn evidence_batch_delete_is_bounded_unique_and_ui_only() {
        let batch = |ids: Vec<String>, client| {
            request(
                ServiceRequest::DeleteEvidenceBatch {
                    session_token: "session-1".into(),
                    evidence_ids: ids,
                },
                client,
            )
        };
        let ids = |count: usize| {
            (0..count)
                .map(|index| format!("evidence-{index}"))
                .collect()
        };
        assert!(
            batch(ids(MAX_EVIDENCE_BATCH), ClientKind::Ui)
                .validate()
                .is_ok()
        );
        assert_eq!(
            batch(ids(1), ClientKind::Agent).validate(),
            Err(ProtocolError::ClientRoleDenied)
        );
        for invalid in [
            Vec::new(),
            ids(MAX_EVIDENCE_BATCH + 1),
            vec!["evidence-1".into(), "evidence-1".into()],
            vec!["../evidence".into()],
        ] {
            assert_eq!(
                batch(invalid, ClientKind::Ui).validate(),
                Err(ProtocolError::InvalidField)
            );
        }
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
    fn restore_network_is_ui_only_and_requires_a_password() {
        let restore = |client, password: &str| {
            request(
                ServiceRequest::RestoreNetwork {
                    session_token: "session-1".into(),
                    password: password.into(),
                },
                client,
            )
        };
        assert!(
            restore(ClientKind::Ui, "administrator-secret")
                .validate()
                .is_ok()
        );
        assert_eq!(
            restore(ClientKind::Agent, "administrator-secret").validate(),
            Err(ProtocolError::ClientRoleDenied)
        );
        assert_eq!(
            restore(ClientKind::Installer, "administrator-secret").validate(),
            Err(ProtocolError::ClientRoleDenied)
        );
        assert_eq!(
            restore(ClientKind::Ui, "").validate(),
            Err(ProtocolError::InvalidField)
        );
    }

    #[test]
    fn status_without_network_field_still_decodes() {
        let status: ServiceStatus = serde_json::from_value(serde_json::json!({
            "service_started_at_ms": 1,
            "policy_revision": 2,
            "protection_enabled": true,
            "agent_connected": false,
            "agent_last_seen_at_ms": null,
            "monitors": [],
            "evidence_count": 0,
            "audit_count": 0
        }))
        .unwrap();
        assert_eq!(status.network, NetworkStatus::default());
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
                super_password: None,
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

    #[test]
    fn allow_banned_application_is_ui_only_and_bounds_the_executable() {
        let allow = |client, executable: String| {
            request(
                ServiceRequest::AllowBannedApplication {
                    session_token: "session-1".into(),
                    expected_revision: 3,
                    executable,
                    super_password: None,
                },
                client,
            )
        };
        assert!(
            allow(ClientKind::Ui, r"c:\games\game.exe".into())
                .validate()
                .is_ok()
        );
        assert_eq!(
            allow(ClientKind::Agent, "game.exe".into()).validate(),
            Err(ProtocolError::ClientRoleDenied)
        );
        for invalid in [
            " ".to_owned(),
            "game\n.exe".to_owned(),
            "a".repeat(MAX_EXECUTABLE_CHARS + 1),
        ] {
            assert_eq!(
                allow(ClientKind::Ui, invalid).validate(),
                Err(ProtocolError::InvalidField)
            );
        }
    }

    #[test]
    fn allow_evidence_application_is_ui_only_and_validates_the_evidence_id() {
        let allow = |client, evidence_id: &str| {
            request(
                ServiceRequest::AllowEvidenceApplication {
                    session_token: "session-1".into(),
                    expected_revision: 3,
                    evidence_id: evidence_id.into(),
                    super_password: Some("distinct-super-password".into()),
                },
                client,
            )
        };
        assert!(allow(ClientKind::Ui, "evidence-1").validate().is_ok());
        assert_eq!(
            allow(ClientKind::Agent, "evidence-1").validate(),
            Err(ProtocolError::ClientRoleDenied)
        );
        assert_eq!(
            allow(ClientKind::Ui, "../evidence").validate(),
            Err(ProtocolError::InvalidField)
        );
    }

    #[test]
    fn enforcement_requests_are_ui_only_and_validate_the_title_id() {
        let forget = |client, title_id: &str| {
            request(
                ServiceRequest::ForgetRememberedTitle {
                    session_token: "session-1".into(),
                    title_id: title_id.into(),
                    super_password: None,
                },
                client,
            )
        };
        assert!(forget(ClientKind::Ui, "title-1").validate().is_ok());
        assert_eq!(
            forget(ClientKind::Agent, "title-1").validate(),
            Err(ProtocolError::ClientRoleDenied)
        );
        assert_eq!(
            forget(ClientKind::Ui, "../title").validate(),
            Err(ProtocolError::InvalidField)
        );
        let list = |client, title_limit, title_query: Option<&str>| {
            request(
                ServiceRequest::ListEnforcement {
                    session_token: "session-1".into(),
                    title_offset: 0,
                    title_limit,
                    title_query: title_query.map(str::to_owned),
                    misjudged: false,
                },
                client,
            )
        };
        assert!(list(ClientKind::Ui, 50, Some("视频")).validate().is_ok());
        let mark = |client, title_id: &str| {
            request(
                ServiceRequest::MarkTitleMisjudged {
                    session_token: "session-1".into(),
                    title_id: title_id.into(),
                    super_password: None,
                },
                client,
            )
        };
        assert!(mark(ClientKind::Ui, "title-1").validate().is_ok());
        assert_eq!(
            mark(ClientKind::Agent, "title-1").validate(),
            Err(ProtocolError::ClientRoleDenied)
        );
        assert_eq!(
            mark(ClientKind::Ui, "../title").validate(),
            Err(ProtocolError::InvalidField)
        );
        let restore = request(
            ServiceRequest::RestoreMisjudgedTitle {
                session_token: "session-1".into(),
                title_id: "../title".into(),
            },
            ClientKind::Ui,
        );
        assert_eq!(restore.validate(), Err(ProtocolError::InvalidField));
        assert_eq!(
            list(ClientKind::Installer, 50, None).validate(),
            Err(ProtocolError::ClientRoleDenied)
        );
        for invalid in [
            list(ClientKind::Ui, 0, None),
            list(ClientKind::Ui, MAX_TITLE_PAGE + 1, None),
            list(
                ClientKind::Ui,
                50,
                Some(&"x".repeat(MAX_TITLE_QUERY_CHARS + 1)),
            ),
            list(ClientKind::Ui, 50, Some("bad\nquery")),
        ] {
            assert_eq!(invalid.validate(), Err(ProtocolError::InvalidField));
        }
        let defaulted: ServiceRequest = serde_json::from_value(serde_json::json!({
            "type": "list_enforcement",
            "payload": {"session_token": "session-1"}
        }))
        .unwrap();
        assert!(matches!(
            defaulted,
            ServiceRequest::ListEnforcement {
                title_offset: 0,
                title_limit: 50,
                title_query: None,
                ..
            }
        ));
    }

    #[test]
    fn image_observation_title_is_optional_and_bounded() {
        let observation = |window_title: Option<String>| {
            request(
                ServiceRequest::AgentObservation {
                    agent_token: "agent-token".into(),
                    observation: AgentObservation {
                        event_id: "event-1".into(),
                        agent_instance_id: "agent-1".into(),
                        occurred_at_ms: 1,
                        monitor_id: "monitor-1".into(),
                        risk_millis: 900,
                        reason_code: "image_immediate".into(),
                        source: None,
                        browser_host: None,
                        evidence_pending: false,
                        window_title,
                    },
                },
                ClientKind::Agent,
            )
        };
        assert!(observation(None).validate().is_ok());
        assert!(
            observation(Some("视频 - Google Chrome".into()))
                .validate()
                .is_ok()
        );
        assert_eq!(
            observation(Some("x".repeat(MAX_WINDOW_TITLE_CHARS + 1))).validate(),
            Err(ProtocolError::InvalidField)
        );
        assert_eq!(
            observation(Some("bad\u{0007}title".into())).validate(),
            Err(ProtocolError::InvalidField)
        );
        let legacy: AgentObservation = serde_json::from_value(serde_json::json!({
            "event_id": "event-1",
            "agent_instance_id": "agent-1",
            "occurred_at_ms": 1,
            "monitor_id": "monitor-1",
            "risk_millis": 900,
            "reason_code": "image_immediate",
            "source": null,
            "evidence_pending": false
        }))
        .unwrap();
        assert_eq!(legacy.window_title, None);
    }

    #[test]
    fn agent_policy_without_enforcement_still_decodes() {
        let result: ServiceResult = serde_json::from_value(serde_json::json!({
            "type": "agent_policy",
            "payload": {"revision": 3, "policy": {}}
        }))
        .unwrap();
        assert!(matches!(
            result,
            ServiceResult::AgentPolicy { revision: 3, enforcement, .. }
                if enforcement == AgentEnforcement::default()
        ));
    }

    #[test]
    fn super_password_and_uninstall_requests_are_ui_only_and_validated() {
        let ui_requests = [
            ServiceRequest::SetSuperPassword {
                session_token: "session-1".into(),
                admin_password: "admin-secret".into(),
                super_password: "super-secret".into(),
            },
            ServiceRequest::ChangeSuperPassword {
                session_token: "session-1".into(),
                current_super_password: "super-secret".into(),
                new_super_password: "super-secret-2".into(),
            },
            ServiceRequest::RequestUninstall {
                session_token: "session-1".into(),
                password: "admin-secret".into(),
            },
            ServiceRequest::CancelUninstall {
                session_token: "session-1".into(),
            },
            ServiceRequest::CheckForUpdates {
                session_token: "session-1".into(),
            },
        ];
        for body in ui_requests {
            assert!(request(body.clone(), ClientKind::Ui).validate().is_ok());
            assert_eq!(
                request(body, ClientKind::Agent).validate(),
                Err(ProtocolError::ClientRoleDenied)
            );
        }
        assert_eq!(
            request(
                ServiceRequest::SetSuperPassword {
                    session_token: "session-1".into(),
                    admin_password: "admin-secret".into(),
                    super_password: String::new(),
                },
                ClientKind::Ui,
            )
            .validate(),
            Err(ProtocolError::InvalidField)
        );
        assert_eq!(
            request(
                ServiceRequest::PutPolicy {
                    session_token: "session-1".into(),
                    expected_revision: 0,
                    policy: serde_json::json!({}),
                    super_password: Some(String::new()),
                },
                ClientKind::Ui,
            )
            .validate(),
            Err(ProtocolError::InvalidField)
        );
        let legacy_put: ServiceRequest = serde_json::from_value(serde_json::json!({
            "type": "put_policy",
            "payload": {"session_token": "session-1", "expected_revision": 0, "policy": {}}
        }))
        .unwrap();
        assert!(matches!(
            legacy_put,
            ServiceRequest::PutPolicy {
                super_password: None,
                ..
            }
        ));
    }

    #[test]
    fn staged_updates_are_installer_only() {
        let update = |client| {
            request(
                ServiceRequest::ApplyStagedUpdate {
                    password: "admin-secret".into(),
                },
                client,
            )
        };
        assert!(update(ClientKind::Installer).validate().is_ok());
        assert_eq!(
            update(ClientKind::Ui).validate(),
            Err(ProtocolError::ClientRoleDenied)
        );
        assert_eq!(
            request(
                ServiceRequest::ApplyStagedUpdate {
                    password: String::new(),
                },
                ClientKind::Installer,
            )
            .validate(),
            Err(ProtocolError::InvalidField)
        );
    }

    #[test]
    fn status_from_older_services_defaults_new_fields() {
        let status: ServiceStatus = serde_json::from_value(serde_json::json!({
            "service_started_at_ms": 1,
            "policy_revision": 2,
            "protection_enabled": true,
            "agent_connected": false,
            "agent_last_seen_at_ms": null,
            "monitors": [],
            "evidence_count": 0,
            "audit_count": 0
        }))
        .unwrap();
        assert!(!status.super_password_set);
        assert_eq!(status.uninstall, None);
        assert_eq!(status.update, UpdateStatus::default());
        assert_eq!(status.product_version, "");
    }
}
