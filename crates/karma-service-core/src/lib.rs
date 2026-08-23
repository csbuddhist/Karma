#![forbid(unsafe_code)]

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt::Write as _,
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    sync::Mutex,
};

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng as PasswordOsRng},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use karma_evidence::EvidenceVault;
use karma_ipc::{
    BootstrapStatus, EvidenceMetadata, RequestEnvelope, ResponseEnvelope, ServiceErrorCode,
    ServiceFailure, ServiceRequest, ServiceResult, ServiceStatus,
};
use karma_policy::{ContextPolicy, ContextVerdict};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use subtle::ConstantTimeEq;
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

const STATE_SCHEMA: u32 = 1;
const MAX_STATE_BYTES: usize = 2 * 1024 * 1024;
const SESSION_LIFETIME_MS: i64 = 15 * 60 * 1000;
const AGENT_OFFLINE_AFTER_MS: i64 = 30 * 1000;
const MAX_OBSERVATION_AGE_MS: i64 = 10 * 1000;
const DISPOSITION_GRACE_MS: u32 = 0;
const MAX_FAILURES: u32 = 5;
const FAILURE_COOLDOWN_MS: i64 = 30 * 1000;
const MAX_REPLAY_ENTRIES: usize = 4096;
const MAX_AUDIT_ENTRIES: usize = 5000;

fn recognition_threshold_millis(policy: &Value) -> u16 {
    policy
        .pointer("/recognition/sensitivity")
        .or_else(|| policy.pointer("/recognition/immediateThreshold"))
        .and_then(Value::as_u64)
        .unwrap_or(82)
        .min(100) as u16
        * 10
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("service state storage is unavailable")]
    StorageUnavailable,
    #[error("service state lock is unavailable")]
    LockUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredState {
    schema_version: u32,
    password_hash: Option<String>,
    policy_revision: u64,
    policy: Value,
    audit: Vec<AuditRecord>,
    evidence: Vec<StoredEvidence>,
}

impl Default for StoredState {
    fn default() -> Self {
        Self {
            schema_version: STATE_SCHEMA,
            password_hash: None,
            policy_revision: 0,
            policy: json!({}),
            audit: Vec::new(),
            evidence: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditRecord {
    occurred_at_ms: i64,
    kind: String,
    outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredEvidence {
    id: String,
    captured_at_ms: i64,
    monitor_name: String,
    application_name: String,
    reason_code: String,
    risk_millis: u16,
    media_type: String,
}

#[derive(Debug)]
struct RuntimeState {
    stored: StoredState,
    sessions: HashMap<String, i64>,
    recent_nonces: VecDeque<String>,
    nonce_index: HashSet<String>,
    failures: u32,
    blocked_until_ms: Option<i64>,
    last_heartbeat: Option<karma_ipc::AgentHeartbeat>,
    pending_dispositions: HashMap<String, karma_ipc::ProcessIdentity>,
}

pub struct ServiceCore {
    state_path: PathBuf,
    started_at_ms: i64,
    agent_token: Zeroizing<String>,
    evidence_vault: EvidenceVault,
    runtime: Mutex<RuntimeState>,
}

impl ServiceCore {
    pub fn open(
        state_path: impl Into<PathBuf>,
        agent_token: String,
        evidence_directory: impl Into<PathBuf>,
        evidence_key: [u8; 32],
        started_at_ms: i64,
    ) -> Result<Self, CoreError> {
        let state_path = state_path.into();
        let stored = if state_path.exists() {
            load_state(&state_path)?
        } else {
            StoredState::default()
        };
        Ok(Self {
            state_path,
            started_at_ms,
            agent_token: Zeroizing::new(agent_token),
            evidence_vault: EvidenceVault::new(evidence_directory, evidence_key),
            runtime: Mutex::new(RuntimeState {
                stored,
                sessions: HashMap::new(),
                recent_nonces: VecDeque::new(),
                nonce_index: HashSet::new(),
                failures: 0,
                blocked_until_ms: None,
                last_heartbeat: None,
                pending_dispositions: HashMap::new(),
            }),
        })
    }

    pub fn handle(&self, request: RequestEnvelope, now_ms: i64) -> ResponseEnvelope {
        let request_id = request.request_id.clone();
        if let Err(error) = request.validate() {
            let code = match error {
                karma_ipc::ProtocolError::UnsupportedVersion => {
                    ServiceErrorCode::UnsupportedVersion
                }
                _ => ServiceErrorCode::InvalidRequest,
            };
            return failure(request_id, code);
        }
        let Ok(mut runtime) = self.runtime.lock() else {
            return failure(request_id, ServiceErrorCode::Internal);
        };
        if !remember_nonce(&mut runtime, request.nonce) {
            return failure(request_id, ServiceErrorCode::ReplayDetected);
        }
        let result = self.dispatch(&mut runtime, request.request, now_ms);
        match result {
            Ok(result) => ResponseEnvelope::success(request_id, result),
            Err(code) => failure(request_id, code),
        }
    }

    pub fn record_disposition(
        &self,
        report: &karma_ipc::DispositionReport,
        now_ms: i64,
    ) -> Result<(), ServiceErrorCode> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| ServiceErrorCode::Internal)?;
        record_disposition(&mut runtime, report, now_ms)?;
        self.persist(&runtime.stored)
    }

    fn dispatch(
        &self,
        runtime: &mut RuntimeState,
        request: ServiceRequest,
        now_ms: i64,
    ) -> Result<ServiceResult, ServiceErrorCode> {
        match request {
            ServiceRequest::GetBootstrap => Ok(ServiceResult::Bootstrap(
                if runtime.stored.password_hash.is_some() {
                    BootstrapStatus::Locked
                } else {
                    BootstrapStatus::SetupRequired
                },
            )),
            ServiceRequest::EnrollAdministrator { password } => {
                if runtime.stored.password_hash.is_some() {
                    return Err(ServiceErrorCode::AlreadyEnrolled);
                }
                if password.chars().count() < 10 {
                    return Err(ServiceErrorCode::InvalidRequest);
                }
                let password = Zeroizing::new(password);
                let salt = SaltString::generate(&mut PasswordOsRng);
                let hash = Argon2::default()
                    .hash_password(password.as_bytes(), &salt)
                    .map_err(|_| ServiceErrorCode::Internal)?
                    .to_string();
                runtime.stored.password_hash = Some(hash);
                audit(runtime, now_ms, "administrator_enrolled", "success");
                self.persist(&runtime.stored)?;
                Ok(new_session(runtime, now_ms))
            }
            ServiceRequest::Authenticate { password } => {
                self.authenticate(runtime, password, now_ms)?;
                audit(runtime, now_ms, "administrator_authenticated", "success");
                self.persist(&runtime.stored)?;
                Ok(new_session(runtime, now_ms))
            }
            ServiceRequest::ChangePassword {
                session_token,
                current_password,
                new_password,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                self.authenticate(runtime, current_password, now_ms)?;
                let new_password = Zeroizing::new(new_password);
                if new_password.chars().count() < 10 {
                    return Err(ServiceErrorCode::InvalidRequest);
                }
                let salt = SaltString::generate(&mut PasswordOsRng);
                let hash = Argon2::default()
                    .hash_password(new_password.as_bytes(), &salt)
                    .map_err(|_| ServiceErrorCode::Internal)?
                    .to_string();
                runtime.stored.password_hash = Some(hash);
                audit(runtime, now_ms, "administrator_password_changed", "success");
                self.persist(&runtime.stored)?;
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::LockSession { session_token } => {
                runtime.sessions.remove(&session_token);
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::GetStatus { session_token } => {
                authorize(runtime, &session_token, now_ms)?;
                let heartbeat = runtime.last_heartbeat.as_ref();
                let connected = heartbeat.is_some_and(|heartbeat| {
                    now_ms.saturating_sub(heartbeat.sent_at_ms) <= AGENT_OFFLINE_AFTER_MS
                });
                Ok(ServiceResult::Status(ServiceStatus {
                    service_started_at_ms: self.started_at_ms,
                    policy_revision: runtime.stored.policy_revision,
                    protection_enabled: runtime
                        .stored
                        .policy
                        .get("protectionEnabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    agent_connected: connected,
                    agent_last_seen_at_ms: heartbeat.map(|heartbeat| heartbeat.sent_at_ms),
                    monitors: heartbeat
                        .map(|heartbeat| heartbeat.monitors.clone())
                        .unwrap_or_default(),
                    evidence_count: runtime.stored.evidence.len() as u64,
                    audit_count: runtime.stored.audit.len() as u64,
                }))
            }
            ServiceRequest::GetPolicy { session_token } => {
                authorize(runtime, &session_token, now_ms)?;
                Ok(ServiceResult::Policy {
                    revision: runtime.stored.policy_revision,
                    policy: runtime.stored.policy.clone(),
                })
            }
            ServiceRequest::PutPolicy {
                session_token,
                expected_revision,
                policy,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                if expected_revision != runtime.stored.policy_revision {
                    return Err(ServiceErrorCode::RevisionConflict);
                }
                ContextPolicy::from_value(&policy).map_err(|_| ServiceErrorCode::InvalidRequest)?;
                runtime.stored.policy_revision = runtime.stored.policy_revision.saturating_add(1);
                runtime.stored.policy = policy;
                audit(runtime, now_ms, "policy_updated", "success");
                self.persist(&runtime.stored)?;
                Ok(ServiceResult::PolicySaved {
                    revision: runtime.stored.policy_revision,
                })
            }
            ServiceRequest::AgentHeartbeat {
                agent_token,
                heartbeat,
            } => {
                self.authorize_agent(&agent_token)?;
                runtime.last_heartbeat = Some(heartbeat);
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::GetAgentPolicy { agent_token } => {
                self.authorize_agent(&agent_token)?;
                Ok(ServiceResult::AgentPolicy {
                    revision: runtime.stored.policy_revision,
                    policy: runtime.stored.policy.clone(),
                })
            }
            ServiceRequest::AgentObservation {
                agent_token,
                observation,
            } => {
                self.authorize_agent(&agent_token)?;
                if now_ms.saturating_sub(observation.occurred_at_ms) > MAX_OBSERVATION_AGE_MS
                    || observation.occurred_at_ms > now_ms.saturating_add(1000)
                {
                    return Ok(ServiceResult::Acknowledged);
                }
                let threshold = recognition_threshold_millis(&runtime.stored.policy);
                let protection_enabled = runtime
                    .stored
                    .policy
                    .get("protectionEnabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let image_enabled = runtime
                    .stored
                    .policy
                    .pointer("/recognition/imageEnabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let Some(target) = observation.source else {
                    return Ok(ServiceResult::Acknowledged);
                };
                let context_policy = ContextPolicy::from_value(&runtime.stored.policy)
                    .map_err(|_| ServiceErrorCode::Internal)?;
                if context_policy.allows_host(observation.browser_host.as_deref()) {
                    return Ok(ServiceResult::Acknowledged);
                }
                if !protection_enabled || !image_enabled || observation.risk_millis < threshold {
                    return Ok(ServiceResult::Acknowledged);
                }
                runtime
                    .pending_dispositions
                    .insert(observation.event_id.clone(), target.clone());
                Ok(ServiceResult::DispositionRequired {
                    event_id: observation.event_id,
                    target,
                    grace_period_ms: DISPOSITION_GRACE_MS,
                })
            }
            ServiceRequest::AgentContextObservation {
                agent_token,
                observation,
            } => {
                self.authorize_agent(&agent_token)?;
                if now_ms.saturating_sub(observation.occurred_at_ms) > MAX_OBSERVATION_AGE_MS
                    || observation.occurred_at_ms > now_ms.saturating_add(1000)
                {
                    return Ok(ServiceResult::Acknowledged);
                }
                let protection_enabled = runtime
                    .stored
                    .policy
                    .get("protectionEnabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                if !protection_enabled {
                    return Ok(ServiceResult::Acknowledged);
                }
                let context_policy = ContextPolicy::from_value(&runtime.stored.policy)
                    .map_err(|_| ServiceErrorCode::Internal)?;
                if !matches!(
                    context_policy.evaluate(
                        observation.browser_host.as_deref(),
                        &observation.window_title,
                    ),
                    ContextVerdict::Blocklisted | ContextVerdict::TitleKeyword
                ) {
                    return Ok(ServiceResult::Acknowledged);
                }
                runtime
                    .pending_dispositions
                    .insert(observation.event_id.clone(), observation.source.clone());
                Ok(ServiceResult::DispositionRequired {
                    event_id: observation.event_id,
                    target: observation.source,
                    grace_period_ms: DISPOSITION_GRACE_MS,
                })
            }
            ServiceRequest::SubmitEvidence {
                agent_token,
                evidence,
            } => {
                self.authorize_agent(&agent_token)?;
                let evidence_enabled = runtime
                    .stored
                    .policy
                    .pointer("/recognition/evidenceEnabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let protection_enabled = runtime
                    .stored
                    .policy
                    .get("protectionEnabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let image_enabled = runtime
                    .stored
                    .policy
                    .pointer("/recognition/imageEnabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let threshold = recognition_threshold_millis(&runtime.stored.policy);
                if !protection_enabled
                    || !image_enabled
                    || !evidence_enabled
                    || evidence.risk_millis < threshold
                {
                    return Err(ServiceErrorCode::InvalidRequest);
                }
                if runtime
                    .stored
                    .evidence
                    .iter()
                    .any(|item| item.id == evidence.evidence_id)
                {
                    return Err(ServiceErrorCode::InvalidRequest);
                }
                let mut plaintext = BASE64
                    .decode(&evidence.bytes_base64)
                    .map_err(|_| ServiceErrorCode::InvalidRequest)?;
                if let Err(error) = validate_image(&evidence.media_type, &plaintext) {
                    plaintext.zeroize();
                    return Err(error);
                }
                self.evidence_vault
                    .store(&evidence.evidence_id, &mut plaintext)
                    .map_err(|_| ServiceErrorCode::StorageUnavailable)?;
                runtime.stored.evidence.push(StoredEvidence {
                    id: evidence.evidence_id.clone(),
                    captured_at_ms: evidence.captured_at_ms,
                    monitor_name: evidence.monitor_name.clone(),
                    application_name: evidence.application_name.clone(),
                    reason_code: evidence.reason_code.clone(),
                    risk_millis: evidence.risk_millis,
                    media_type: evidence.media_type.clone(),
                });
                audit(runtime, now_ms, "evidence_stored", "success");
                self.persist(&runtime.stored)?;
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::ReportDisposition {
                agent_token,
                report,
            } => {
                self.authorize_agent(&agent_token)?;
                record_disposition(runtime, &report, now_ms)?;
                self.persist(&runtime.stored)?;
                Ok(ServiceResult::DispositionCompleted { report })
            }
            ServiceRequest::ListEvidence { session_token } => {
                authorize(runtime, &session_token, now_ms)?;
                Ok(ServiceResult::EvidenceList {
                    items: runtime
                        .stored
                        .evidence
                        .iter()
                        .map(|item| EvidenceMetadata {
                            id: item.id.clone(),
                            captured_at_ms: item.captured_at_ms,
                            monitor_name: item.monitor_name.clone(),
                            application_name: item.application_name.clone(),
                            reason_code: item.reason_code.clone(),
                            risk_millis: item.risk_millis,
                            original_available: self.evidence_vault.exists(&item.id),
                        })
                        .collect(),
                })
            }
            ServiceRequest::RevealEvidence {
                session_token,
                password,
                evidence_id,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                self.authenticate(runtime, password, now_ms)?;
                let item = runtime
                    .stored
                    .evidence
                    .iter()
                    .find(|item| item.id == evidence_id)
                    .ok_or(ServiceErrorCode::EvidenceUnavailable)?;
                let bytes = self
                    .evidence_vault
                    .reveal(&evidence_id)
                    .map_err(|_| ServiceErrorCode::EvidenceUnavailable)?;
                Ok(ServiceResult::EvidenceImage {
                    media_type: item.media_type.clone(),
                    bytes_base64: BASE64.encode(&*bytes),
                })
            }
            ServiceRequest::DeleteEvidence {
                session_token,
                evidence_id,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                let index = runtime
                    .stored
                    .evidence
                    .iter()
                    .position(|item| item.id == evidence_id)
                    .ok_or(ServiceErrorCode::EvidenceUnavailable)?;
                self.evidence_vault
                    .delete(&evidence_id)
                    .map_err(|_| ServiceErrorCode::EvidenceUnavailable)?;
                runtime.stored.evidence.remove(index);
                audit(runtime, now_ms, "evidence_deleted", "success");
                self.persist(&runtime.stored)?;
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::RequestShutdown { password } => {
                self.authenticate(runtime, password, now_ms)?;
                audit(runtime, now_ms, "service_shutdown_requested", "success");
                self.persist(&runtime.stored)?;
                Ok(ServiceResult::Acknowledged)
            }
        }
    }

    fn authenticate(
        &self,
        runtime: &mut RuntimeState,
        mut password: String,
        now_ms: i64,
    ) -> Result<(), ServiceErrorCode> {
        if runtime.blocked_until_ms.is_some_and(|until| until > now_ms) {
            password.zeroize();
            return Err(ServiceErrorCode::RateLimited);
        }
        let Some(stored_hash) = runtime.stored.password_hash.as_ref() else {
            password.zeroize();
            return Err(ServiceErrorCode::NotEnrolled);
        };
        let parsed =
            PasswordHash::new(stored_hash).map_err(|_| ServiceErrorCode::StorageUnavailable)?;
        let valid = Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok();
        password.zeroize();
        if valid {
            runtime.failures = 0;
            runtime.blocked_until_ms = None;
            return Ok(());
        }
        runtime.failures = runtime.failures.saturating_add(1);
        audit(runtime, now_ms, "administrator_authentication", "denied");
        if runtime.failures >= MAX_FAILURES {
            runtime.failures = 0;
            runtime.blocked_until_ms = Some(now_ms.saturating_add(FAILURE_COOLDOWN_MS));
        }
        Err(ServiceErrorCode::AuthenticationFailed)
    }

    fn authorize_agent(&self, presented: &str) -> Result<(), ServiceErrorCode> {
        if self
            .agent_token
            .as_bytes()
            .ct_eq(presented.as_bytes())
            .into()
        {
            Ok(())
        } else {
            Err(ServiceErrorCode::AgentUnauthorized)
        }
    }

    fn persist(&self, state: &StoredState) -> Result<(), ServiceErrorCode> {
        save_state(&self.state_path, state).map_err(|_| ServiceErrorCode::StorageUnavailable)
    }
}

fn authorize(runtime: &mut RuntimeState, token: &str, now_ms: i64) -> Result<(), ServiceErrorCode> {
    runtime
        .sessions
        .retain(|_, expires_at| *expires_at > now_ms);
    let expires_at = runtime
        .sessions
        .get_mut(token)
        .ok_or(ServiceErrorCode::AuthenticationRequired)?;
    *expires_at = now_ms.saturating_add(SESSION_LIFETIME_MS);
    Ok(())
}

fn validate_image(media_type: &str, bytes: &[u8]) -> Result<(), ServiceErrorCode> {
    let valid = match media_type {
        "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "image/png" => bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(ServiceErrorCode::InvalidRequest)
    }
}

fn new_session(runtime: &mut RuntimeState, now_ms: i64) -> ServiceResult {
    let token = random_token();
    runtime.sessions.clear();
    runtime
        .sessions
        .insert(token.clone(), now_ms.saturating_add(SESSION_LIFETIME_MS));
    ServiceResult::Session {
        session_token: token,
        expires_in_seconds: (SESSION_LIFETIME_MS / 1000) as u32,
    }
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let mut token = String::with_capacity(64);
    for byte in bytes {
        write!(&mut token, "{byte:02x}").expect("writing to a String cannot fail");
    }
    token
}

fn remember_nonce(runtime: &mut RuntimeState, nonce: String) -> bool {
    if !runtime.nonce_index.insert(nonce.clone()) {
        return false;
    }
    runtime.recent_nonces.push_back(nonce);
    if runtime.recent_nonces.len() > MAX_REPLAY_ENTRIES {
        if let Some(expired) = runtime.recent_nonces.pop_front() {
            runtime.nonce_index.remove(&expired);
        }
    }
    true
}

fn audit(runtime: &mut RuntimeState, occurred_at_ms: i64, kind: &str, outcome: &str) {
    runtime.stored.audit.push(AuditRecord {
        occurred_at_ms,
        kind: kind.into(),
        outcome: outcome.into(),
    });
    if runtime.stored.audit.len() > MAX_AUDIT_ENTRIES {
        let remove = runtime.stored.audit.len() - MAX_AUDIT_ENTRIES;
        runtime.stored.audit.drain(..remove);
    }
}

fn record_disposition(
    runtime: &mut RuntimeState,
    report: &karma_ipc::DispositionReport,
    now_ms: i64,
) -> Result<(), ServiceErrorCode> {
    let expected = runtime
        .pending_dispositions
        .get(&report.event_id)
        .ok_or(ServiceErrorCode::InvalidRequest)?;
    if expected.process_id != report.process_id || expected.started_at_ms != report.started_at_ms {
        return Err(ServiceErrorCode::InvalidRequest);
    }
    runtime.pending_dispositions.remove(&report.event_id);
    let outcome = match report.outcome {
        karma_ipc::DispositionOutcome::ClosedGracefully => "closed_gracefully",
        karma_ipc::DispositionOutcome::Terminated => "terminated",
        karma_ipc::DispositionOutcome::IdentityChanged => "identity_changed",
        karma_ipc::DispositionOutcome::AccessDenied => "access_denied",
        karma_ipc::DispositionOutcome::SourceUncertain => "source_uncertain",
    };
    audit(runtime, now_ms, "application_disposition", outcome);
    Ok(())
}

fn load_state(path: &Path) -> Result<StoredState, CoreError> {
    let metadata = fs::metadata(path).map_err(|_| CoreError::StorageUnavailable)?;
    if metadata.len() > MAX_STATE_BYTES as u64 {
        return Err(CoreError::StorageUnavailable);
    }
    let bytes = fs::read(path).map_err(|_| CoreError::StorageUnavailable)?;
    let state: StoredState =
        serde_json::from_slice(&bytes).map_err(|_| CoreError::StorageUnavailable)?;
    if state.schema_version != STATE_SCHEMA {
        return Err(CoreError::StorageUnavailable);
    }
    Ok(state)
}

fn save_state(path: &Path, state: &StoredState) -> Result<(), CoreError> {
    let encoded = serde_json::to_vec(state).map_err(|_| CoreError::StorageUnavailable)?;
    if encoded.len() > MAX_STATE_BYTES {
        return Err(CoreError::StorageUnavailable);
    }
    let parent = path.parent().ok_or(CoreError::StorageUnavailable)?;
    fs::create_dir_all(parent).map_err(|_| CoreError::StorageUnavailable)?;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|_| CoreError::StorageUnavailable)?;
    file.write_all(&encoded)
        .map_err(|_| CoreError::StorageUnavailable)?;
    file.sync_all().map_err(|_| CoreError::StorageUnavailable)
}

fn failure(request_id: String, code: ServiceErrorCode) -> ResponseEnvelope {
    ResponseEnvelope::failure(request_id, ServiceFailure::new(code))
}

#[cfg(test)]
mod tests {
    use super::*;
    use karma_ipc::{
        AgentContextObservation, AgentHeartbeat, AgentObservation, ClientKind, DispositionOutcome,
        DispositionReport, EvidenceSubmission, ProcessIdentity, ServiceRequest,
    };

    #[test]
    fn sensitivity_precedes_legacy_immediate_threshold() {
        assert_eq!(
            recognition_threshold_millis(&json!({
                "recognition": {
                    "sensitivity": 67,
                    "immediateThreshold": 95
                }
            })),
            670
        );
        assert_eq!(recognition_threshold_millis(&json!({})), 820);
    }

    fn request(id: &str, nonce: &str, body: ServiceRequest) -> RequestEnvelope {
        RequestEnvelope::new(id, nonce, ClientKind::Ui, body)
    }

    fn success(response: ResponseEnvelope) -> ServiceResult {
        response.result.expect("request should succeed")
    }

    fn open_core(directory: &Path, started_at_ms: i64) -> ServiceCore {
        ServiceCore::open(
            directory.join("service.json"),
            "agent-secret".into(),
            directory.join("evidence"),
            [7; 32],
            started_at_ms,
        )
        .unwrap()
    }

    fn configure_policy(core: &ServiceCore, policy: Value) {
        let session = match success(core.handle(
            request(
                "enroll-policy",
                "enroll-policy-nonce",
                ServiceRequest::EnrollAdministrator {
                    password: "long-test-password".into(),
                },
            ),
            1,
        )) {
            ServiceResult::Session { session_token, .. } => session_token,
            _ => panic!("unexpected response"),
        };
        success(core.handle(
            request(
                "put-policy",
                "put-policy-nonce",
                ServiceRequest::PutPolicy {
                    session_token: session,
                    expected_revision: 0,
                    policy,
                },
            ),
            2,
        ));
    }

    fn context_observation(
        event_id: &str,
        title: &str,
        browser_host: Option<&str>,
    ) -> AgentContextObservation {
        AgentContextObservation {
            event_id: event_id.into(),
            agent_instance_id: "agent-1".into(),
            occurred_at_ms: 100,
            source: ProcessIdentity {
                process_id: 42,
                started_at_ms: 77,
                executable_name: r"C:\Browser\chrome.exe".into(),
                executable_sha256: None,
            },
            window_title: title.into(),
            browser_host: browser_host.map(str::to_owned),
        }
    }

    fn submit_context(
        core: &ServiceCore,
        nonce: &str,
        observation: AgentContextObservation,
    ) -> ServiceResult {
        success(core.handle(
            RequestEnvelope::new(
                format!("request-{nonce}"),
                nonce,
                ClientKind::Agent,
                ServiceRequest::AgentContextObservation {
                    agent_token: "agent-secret".into(),
                    observation,
                },
            ),
            101,
        ))
    }

    #[test]
    fn website_allowlist_precedes_blocklist_title_and_image_enforcement() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(
            &core,
            json!({
                "protectionEnabled": true,
                "recognition": {"imageEnabled": true, "sensitivity": 60},
                "websites": [
                    {"id":"block-parent","pattern":"example.com","action":"block","enabled":true},
                    {"id":"allow-child","pattern":"safe.example.com","action":"allow","enabled":true}
                ]
            }),
        );
        assert!(matches!(
            submit_context(
                &core,
                "allow-context",
                context_observation("allow-context", "Porn videos", Some("safe.example.com")),
            ),
            ServiceResult::Acknowledged
        ));
        let image = AgentObservation {
            event_id: "allow-image".into(),
            agent_instance_id: "agent-1".into(),
            occurred_at_ms: 100,
            monitor_id: "monitor-1".into(),
            risk_millis: 1000,
            reason_code: "image_immediate".into(),
            source: Some(context_observation("ignored", "ordinary", None).source),
            browser_host: Some("safe.example.com".into()),
            evidence_pending: false,
        };
        assert!(matches!(
            success(core.handle(
                RequestEnvelope::new(
                    "allow-image-request",
                    "allow-image-nonce",
                    ClientKind::Agent,
                    ServiceRequest::AgentObservation {
                        agent_token: "agent-secret".into(),
                        observation: image,
                    },
                ),
                101,
            )),
            ServiceResult::Acknowledged
        ));
    }

    #[test]
    fn blocklisted_host_and_multilingual_title_authorize_immediate_disposition() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(
            &core,
            json!({
                "protectionEnabled": true,
                "recognition": {"imageEnabled": false, "titleMatchingEnabled": true},
                "websites": [
                    {"id":"blocked","pattern":"blocked.example","action":"block","enabled":true}
                ]
            }),
        );
        assert!(matches!(
            submit_context(
                &core,
                "blocked-host-nonce",
                context_observation("blocked-host", "", Some("www.blocked.example")),
            ),
            ServiceResult::DispositionRequired { event_id, .. } if event_id == "blocked-host"
        ));
        assert!(matches!(
            submit_context(
                &core,
                "title-nonce",
                context_observation("title-match", "最新色情影片", None),
            ),
            ServiceResult::DispositionRequired { event_id, .. } if event_id == "title-match"
        ));
    }

    #[test]
    fn enrollment_authentication_policy_and_restart_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 100);
        assert!(matches!(
            success(core.handle(request("1", "n1", ServiceRequest::GetBootstrap), 100)),
            ServiceResult::Bootstrap(BootstrapStatus::SetupRequired)
        ));
        let session = match success(core.handle(
            request(
                "2",
                "n2",
                ServiceRequest::EnrollAdministrator {
                    password: "long-test-password".into(),
                },
            ),
            101,
        )) {
            ServiceResult::Session { session_token, .. } => session_token,
            _ => panic!("unexpected response"),
        };
        assert!(matches!(
            success(core.handle(
                request(
                    "3",
                    "n3",
                    ServiceRequest::PutPolicy {
                        session_token: session,
                        expected_revision: 0,
                        policy: json!({"protectionEnabled": false})
                    }
                ),
                102
            )),
            ServiceResult::PolicySaved { revision: 1 }
        ));
        drop(core);
        let reopened = open_core(directory.path(), 200);
        let session = match success(reopened.handle(
            request(
                "4",
                "n4",
                ServiceRequest::Authenticate {
                    password: "long-test-password".into(),
                },
            ),
            201,
        )) {
            ServiceResult::Session { session_token, .. } => session_token,
            _ => panic!("unexpected response"),
        };
        match success(reopened.handle(
            request(
                "5",
                "n5",
                ServiceRequest::GetPolicy {
                    session_token: session,
                },
            ),
            202,
        )) {
            ServiceResult::Policy { revision, policy } => {
                assert_eq!(revision, 1);
                assert_eq!(policy["protectionEnabled"], false);
            }
            _ => panic!("unexpected response"),
        }
    }

    #[test]
    fn password_change_requires_current_secret_and_persists_new_hash() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 100);
        let session = match success(core.handle(
            request(
                "1",
                "n1",
                ServiceRequest::EnrollAdministrator {
                    password: "long-test-password".into(),
                },
            ),
            101,
        )) {
            ServiceResult::Session { session_token, .. } => session_token,
            _ => panic!("unexpected response"),
        };
        let change = |nonce: &str, current: &str, new: &str, now: i64| {
            core.handle(
                request(
                    &format!("change-{nonce}"),
                    nonce,
                    ServiceRequest::ChangePassword {
                        session_token: session.clone(),
                        current_password: current.into(),
                        new_password: new.into(),
                    },
                ),
                now,
            )
        };
        assert_eq!(
            change("n2", "wrong-password", "replacement-password", 102)
                .result
                .unwrap_err()
                .code,
            ServiceErrorCode::AuthenticationFailed
        );
        assert_eq!(
            change("n3", "long-test-password", "short", 103)
                .result
                .unwrap_err()
                .code,
            ServiceErrorCode::InvalidRequest
        );
        assert!(matches!(
            success(change(
                "n4",
                "long-test-password",
                "replacement-password",
                104
            )),
            ServiceResult::Acknowledged
        ));
        assert_eq!(
            core.handle(
                request(
                    "5",
                    "n5",
                    ServiceRequest::Authenticate {
                        password: "long-test-password".into(),
                    },
                ),
                105,
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::AuthenticationFailed
        );
        drop(core);
        let reopened = open_core(directory.path(), 200);
        assert!(matches!(
            success(reopened.handle(
                request(
                    "6",
                    "n6",
                    ServiceRequest::Authenticate {
                        password: "replacement-password".into(),
                    },
                ),
                201,
            )),
            ServiceResult::Session { .. }
        ));
        let state: StoredState =
            serde_json::from_slice(&fs::read(directory.path().join("service.json")).unwrap())
                .unwrap();
        assert!(
            state
                .audit
                .iter()
                .any(|record| record.kind == "administrator_password_changed")
        );
    }

    #[test]
    fn custom_title_keywords_enforce_and_exemptions_suppress_bundled_matches() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(
            &core,
            json!({
                "protectionEnabled": true,
                "recognition": {"imageEnabled": false, "titleMatchingEnabled": true},
                "keywords": [
                    {"id":"custom-1","phrase":"赌球直播","category":"high_risk","enabled":true},
                    {"id":"exempt-1","phrase":"医学教育","category":"exemption","enabled":true}
                ]
            }),
        );
        assert!(matches!(
            submit_context(
                &core,
                "custom-keyword-nonce",
                context_observation("custom-keyword", "今晚赌球直播现场", None),
            ),
            ServiceResult::DispositionRequired { event_id, .. } if event_id == "custom-keyword"
        ));
        assert!(matches!(
            submit_context(
                &core,
                "exempt-keyword-nonce",
                context_observation("exempt-keyword", "色情内容医学教育课件", None),
            ),
            ServiceResult::Acknowledged
        ));
    }

    #[test]
    fn put_policy_rejects_invalid_custom_keyword_rules() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        let session = match success(core.handle(
            request(
                "enroll",
                "enroll-nonce",
                ServiceRequest::EnrollAdministrator {
                    password: "long-test-password".into(),
                },
            ),
            1,
        )) {
            ServiceResult::Session { session_token, .. } => session_token,
            _ => panic!("unexpected response"),
        };
        assert_eq!(
            core.handle(
                request(
                    "policy",
                    "policy-nonce",
                    ServiceRequest::PutPolicy {
                        session_token: session,
                        expected_revision: 0,
                        policy: json!({
                            "keywords": [
                                {"id":"bad","phrase":"","category":"high_risk","enabled":true}
                            ]
                        }),
                    },
                ),
                2,
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::InvalidRequest
        );
    }

    #[test]
    fn incorrect_uninstall_password_does_not_authorize_shutdown() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 100);
        success(core.handle(
            request(
                "1",
                "n1",
                ServiceRequest::EnrollAdministrator {
                    password: "long-test-password".into(),
                },
            ),
            101,
        ));

        let response = core.handle(
            RequestEnvelope::new(
                "2",
                "n2",
                ClientKind::Installer,
                ServiceRequest::RequestShutdown {
                    password: "wrong-password".into(),
                },
            ),
            102,
        );

        assert_eq!(
            response.result.unwrap_err().code,
            ServiceErrorCode::AuthenticationFailed
        );
        let state: StoredState =
            serde_json::from_slice(&fs::read(directory.path().join("service.json")).unwrap())
                .unwrap();
        assert!(
            state
                .audit
                .iter()
                .all(|record| record.kind != "service_shutdown_requested")
        );
    }

    #[test]
    fn nonce_replay_session_expiry_and_agent_auth_are_enforced() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        let first = core.handle(request("1", "same", ServiceRequest::GetBootstrap), 0);
        assert!(first.result.is_ok());
        let replay = core.handle(request("2", "same", ServiceRequest::GetBootstrap), 0);
        assert_eq!(
            replay.result.unwrap_err().code,
            ServiceErrorCode::ReplayDetected
        );

        let heartbeat = AgentHeartbeat {
            agent_instance_id: "agent-1".into(),
            user_sid: "S-1-5-21-test".into(),
            process_id: 10,
            sent_at_ms: 5,
            monitors: vec![],
        };
        let denied = RequestEnvelope::new(
            "3",
            "n3",
            ClientKind::Agent,
            ServiceRequest::AgentHeartbeat {
                agent_token: "wrong".into(),
                heartbeat: heartbeat.clone(),
            },
        );
        assert_eq!(
            core.handle(denied, 5).result.unwrap_err().code,
            ServiceErrorCode::AgentUnauthorized
        );
        let allowed = RequestEnvelope::new(
            "4",
            "n4",
            ClientKind::Agent,
            ServiceRequest::AgentHeartbeat {
                agent_token: "agent-secret".into(),
                heartbeat,
            },
        );
        assert!(core.handle(allowed, 5).result.is_ok());
    }

    #[test]
    fn only_recent_high_risk_observations_authorize_a_bound_disposition() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        let source = ProcessIdentity {
            process_id: 42,
            started_at_ms: 77,
            executable_name: r"C:\Browser\browser.exe".into(),
            executable_sha256: None,
        };
        let observation = AgentObservation {
            event_id: "event-1".into(),
            agent_instance_id: "agent-1".into(),
            occurred_at_ms: 100,
            monitor_id: "monitor-1".into(),
            risk_millis: 960,
            reason_code: "image_immediate".into(),
            source: Some(source.clone()),
            browser_host: None,
            evidence_pending: false,
        };
        let request = RequestEnvelope::new(
            "1",
            "n1",
            ClientKind::Agent,
            ServiceRequest::AgentObservation {
                agent_token: "agent-secret".into(),
                observation,
            },
        );
        assert!(matches!(
            success(core.handle(request, 101)),
            ServiceResult::DispositionRequired {
                event_id,
                target,
                grace_period_ms: 0,
            } if event_id == "event-1" && target == source
        ));
        let mismatched = DispositionReport {
            event_id: "event-1".into(),
            process_id: 42,
            started_at_ms: 78,
            outcome: DispositionOutcome::Terminated,
        };
        assert_eq!(
            core.record_disposition(&mismatched, 102),
            Err(ServiceErrorCode::InvalidRequest)
        );
    }

    #[test]
    fn disabled_image_recognition_does_not_authorize_disposition() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        let session = match success(core.handle(
            request(
                "enroll",
                "enroll-nonce",
                ServiceRequest::EnrollAdministrator {
                    password: "long-test-password".into(),
                },
            ),
            1,
        )) {
            ServiceResult::Session { session_token, .. } => session_token,
            _ => panic!("unexpected response"),
        };
        success(core.handle(
            request(
                "policy",
                "policy-nonce",
                ServiceRequest::PutPolicy {
                    session_token: session,
                    expected_revision: 0,
                    policy: json!({
                        "protectionEnabled": true,
                        "recognition": {
                            "imageEnabled": false,
                            "immediateThreshold": 60
                        }
                    }),
                },
            ),
            2,
        ));
        let observation = AgentObservation {
            event_id: "event-disabled-image".into(),
            agent_instance_id: "agent-1".into(),
            occurred_at_ms: 100,
            monitor_id: "monitor-1".into(),
            risk_millis: 990,
            reason_code: "image_immediate".into(),
            source: Some(ProcessIdentity {
                process_id: 42,
                started_at_ms: 77,
                executable_name: r"C:\Browser\browser.exe".into(),
                executable_sha256: None,
            }),
            browser_host: None,
            evidence_pending: false,
        };
        let response = success(core.handle(
            RequestEnvelope::new(
                "observation",
                "observation-nonce",
                ClientKind::Agent,
                ServiceRequest::AgentObservation {
                    agent_token: "agent-secret".into(),
                    observation,
                },
            ),
            101,
        ));

        assert!(matches!(response, ServiceResult::Acknowledged));
    }

    #[test]
    fn evidence_is_encrypted_listed_and_requires_password_to_reveal() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        let session = match success(core.handle(
            request(
                "1",
                "n1",
                ServiceRequest::EnrollAdministrator {
                    password: "long-test-password".into(),
                },
            ),
            1,
        )) {
            ServiceResult::Session { session_token, .. } => session_token,
            _ => panic!("unexpected response"),
        };
        assert!(matches!(
            success(core.handle(
                request(
                    "policy",
                    "policy-nonce",
                    ServiceRequest::PutPolicy {
                        session_token: session.clone(),
                        expected_revision: 0,
                        policy: json!({
                            "protectionEnabled": true,
                            "recognition": {
                                "evidenceEnabled": true,
                                "immediateThreshold": 95
                            }
                        }),
                    },
                ),
                1,
            )),
            ServiceResult::PolicySaved { revision: 1 }
        ));
        let image = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3];
        let submission = EvidenceSubmission {
            evidence_id: "evidence-1".into(),
            captured_at_ms: 2,
            monitor_name: "显示器 1".into(),
            application_name: "browser.exe".into(),
            reason_code: "image_immediate".into(),
            risk_millis: 980,
            media_type: "image/png".into(),
            bytes_base64: BASE64.encode(image),
        };
        let mut below_threshold = submission.clone();
        below_threshold.evidence_id = "evidence-low".into();
        below_threshold.risk_millis = 949;
        let rejected = RequestEnvelope::new(
            "low",
            "low-nonce",
            ClientKind::Agent,
            ServiceRequest::SubmitEvidence {
                agent_token: "agent-secret".into(),
                evidence: below_threshold,
            },
        );
        assert_eq!(
            core.handle(rejected, 2).result.unwrap_err().code,
            ServiceErrorCode::InvalidRequest
        );
        let submit = RequestEnvelope::new(
            "2",
            "n2",
            ClientKind::Agent,
            ServiceRequest::SubmitEvidence {
                agent_token: "agent-secret".into(),
                evidence: submission,
            },
        );
        assert!(matches!(
            success(core.handle(submit, 2)),
            ServiceResult::Acknowledged
        ));
        let encrypted = fs::read(directory.path().join("evidence/evidence-1.kme")).unwrap();
        assert!(!encrypted.windows(image.len()).any(|window| window == image));
        assert!(matches!(
            success(core.handle(
                request("3", "n3", ServiceRequest::ListEvidence { session_token: session.clone() }),
                3,
            )),
            ServiceResult::EvidenceList { items } if items.len() == 1 && items[0].original_available
        ));
        assert_eq!(
            core.handle(
                request(
                    "4",
                    "n4",
                    ServiceRequest::RevealEvidence {
                        session_token: session.clone(),
                        password: "wrong-password".into(),
                        evidence_id: "evidence-1".into(),
                    }
                ),
                4,
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::AuthenticationFailed
        );
        match success(core.handle(
            request(
                "5",
                "n5",
                ServiceRequest::RevealEvidence {
                    session_token: session,
                    password: "long-test-password".into(),
                    evidence_id: "evidence-1".into(),
                },
            ),
            5,
        )) {
            ServiceResult::EvidenceImage { bytes_base64, .. } => {
                assert_eq!(BASE64.decode(bytes_base64).unwrap(), image);
            }
            _ => panic!("unexpected response"),
        }
    }
}
