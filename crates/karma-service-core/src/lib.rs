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
use karma_ipc::{
    BootstrapStatus, RequestEnvelope, ResponseEnvelope, ServiceErrorCode, ServiceFailure,
    ServiceRequest, ServiceResult, ServiceStatus,
};
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
const DISPOSITION_GRACE_MS: u32 = 2000;
const MAX_FAILURES: u32 = 5;
const FAILURE_COOLDOWN_MS: i64 = 30 * 1000;
const MAX_REPLAY_ENTRIES: usize = 4096;
const MAX_AUDIT_ENTRIES: usize = 5000;

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
}

impl Default for StoredState {
    fn default() -> Self {
        Self {
            schema_version: STATE_SCHEMA,
            password_hash: None,
            policy_revision: 0,
            policy: json!({}),
            audit: Vec::new(),
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
    runtime: Mutex<RuntimeState>,
}

impl ServiceCore {
    pub fn open(
        state_path: impl Into<PathBuf>,
        agent_token: String,
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
                    evidence_count: 0,
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
                let threshold = runtime
                    .stored
                    .policy
                    .pointer("/recognition/immediateThreshold")
                    .and_then(Value::as_u64)
                    .unwrap_or(95)
                    .min(100) as u16
                    * 10;
                let protection_enabled = runtime
                    .stored
                    .policy
                    .get("protectionEnabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let Some(target) = observation.source else {
                    return Ok(ServiceResult::Acknowledged);
                };
                if !protection_enabled || observation.risk_millis < threshold {
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
                Ok(ServiceResult::EvidenceList { items: vec![] })
            }
            ServiceRequest::RevealEvidence {
                session_token,
                password,
                ..
            } => {
                authorize(runtime, &session_token, now_ms)?;
                self.authenticate(runtime, password, now_ms)?;
                Err(ServiceErrorCode::EvidenceUnavailable)
            }
            ServiceRequest::DeleteEvidence { session_token, .. } => {
                authorize(runtime, &session_token, now_ms)?;
                Err(ServiceErrorCode::EvidenceUnavailable)
            }
            ServiceRequest::RequestShutdown { session_token } => {
                authorize(runtime, &session_token, now_ms)?;
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
        AgentHeartbeat, AgentObservation, ClientKind, DispositionOutcome, DispositionReport,
        ProcessIdentity, ServiceRequest,
    };

    fn request(id: &str, nonce: &str, body: ServiceRequest) -> RequestEnvelope {
        RequestEnvelope::new(id, nonce, ClientKind::Ui, body)
    }

    fn success(response: ResponseEnvelope) -> ServiceResult {
        response.result.expect("request should succeed")
    }

    #[test]
    fn enrollment_authentication_policy_and_restart_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("service.json");
        let core = ServiceCore::open(&path, "agent-secret".into(), 100).unwrap();
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
        let reopened = ServiceCore::open(&path, "agent-secret".into(), 200).unwrap();
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
    fn nonce_replay_session_expiry_and_agent_auth_are_enforced() {
        let directory = tempfile::tempdir().unwrap();
        let core = ServiceCore::open(
            directory.path().join("state.json"),
            "agent-secret".into(),
            0,
        )
        .unwrap();
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
        let core = ServiceCore::open(
            directory.path().join("state.json"),
            "agent-secret".into(),
            0,
        )
        .unwrap();
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
            ServiceResult::DispositionRequired { event_id, target, .. }
                if event_id == "event-1" && target == source
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
}
