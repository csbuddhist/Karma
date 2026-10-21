#![forbid(unsafe_code)]

mod store;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt::Write as _,
    path::PathBuf,
    sync::Mutex,
};

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng as PasswordOsRng},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use karma_evidence::EvidenceVault;
use karma_ipc::{
    ActiveBan, AgentEnforcement, BootstrapStatus, EnforcementSummary, EvidenceMetadata,
    NetworkStatus, RememberedTitleMetadata, RequestEnvelope, ResponseEnvelope, ServiceErrorCode,
    ServiceFailure, ServiceRequest, ServiceResult, ServiceStatus, TitleReasonMetadata,
    UninstallStatus, UpdatePhase, UpdateStatus,
};
use karma_policy::{
    ApplicationEffect, ApplicationPolicy, ContextPolicy, MAX_MISJUDGED_TITLES, MINUTES_PER_DAY,
    MINUTES_PER_WEEK, NetworkRestriction, NetworkSchedulePolicy, RememberedTitle,
    RepeatOffenderRule, TitleReason, active_bans, allow_application, is_banned,
    normalize_executable, policy_weakenings, record_closure, remember_title,
    remember_titles_enabled, rememberable_title_key, title_digest, title_key,
};
use rand::{RngCore, rngs::OsRng};
use serde_json::Value;
use store::{
    AuditRecord, Change, MAX_AUDIT_ENTRIES, NetworkRestore, PersistedState, Store, StoredEvidence,
    UninstallRequest,
};
use subtle::ConstantTimeEq;
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

const SESSION_LIFETIME_MS: i64 = 15 * 60 * 1000;
const AGENT_OFFLINE_AFTER_MS: i64 = 30 * 1000;
const MAX_OBSERVATION_AGE_MS: i64 = 10 * 1000;
const DISPOSITION_GRACE_MS: u32 = 0;
const MAX_FAILURES: u32 = 5;
const FAILURE_COOLDOWN_MS: i64 = 30 * 1000;
const UNINSTALL_WAIT_MS: i64 = 72 * 60 * 60 * 1000;
const UNINSTALL_VALIDITY_MS: i64 = 24 * 60 * 60 * 1000;
const MAX_REPLAY_ENTRIES: usize = 4096;
const MS_PER_MINUTE: i64 = 60 * 1000;
/// 1970-01-01 was a Thursday; schedules number Monday as day 0.
const EPOCH_WEEKDAY_FROM_MONDAY: i64 = 3;

/// Returns the local UTC offset in seconds at a Unix millisecond instant.
pub type LocalOffsetFn = fn(i64) -> i32;

fn system_local_offset_seconds(now_ms: i64) -> i32 {
    use chrono::{Local, Offset, TimeZone};
    Local
        .timestamp_millis_opt(now_ms)
        .earliest()
        .map_or(0, |local| local.offset().fix().local_minus_utc())
}

fn local_minute_of_week(now_ms: i64, offset_seconds: i32) -> u16 {
    let local_minutes = now_ms
        .saturating_add(i64::from(offset_seconds) * 1000)
        .div_euclid(MS_PER_MINUTE);
    let minute = (local_minutes + EPOCH_WEEKDAY_FROM_MONDAY * i64::from(MINUTES_PER_DAY))
        .rem_euclid(i64::from(MINUTES_PER_WEEK));
    minute as u16
}

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
    #[error("an uninstall shutdown has already been authorized")]
    ShutdownAuthorized,
}

struct RuntimeState {
    /// In-memory copy of the persisted state; every mutation also queues a
    /// change in `pending`, which `persist` commits.
    stored: PersistedState,
    store: Store,
    pending: Vec<Change>,
    /// `title_digest` of every remembered title, for constant-time matching.
    title_digests: HashSet<String>,
    /// `title_digest` of every title marked as misjudged.
    misjudged_digests: HashSet<String>,
    sessions: HashMap<String, i64>,
    recent_nonces: VecDeque<String>,
    nonce_index: HashSet<String>,
    administrator_auth: AuthenticationLimit,
    super_auth: AuthenticationLimit,
    last_heartbeat: Option<karma_ipc::AgentHeartbeat>,
    pending_dispositions: HashMap<String, PendingDisposition>,
    network_restricted: Option<bool>,
    network_enforcement_succeeded: Option<bool>,
    pending_update: Option<UpdateAction>,
    update_status: UpdateStatus,
    shutdown_authorized: bool,
}

/// A disposition the service authorized and awaits a report for.
#[derive(Debug)]
struct PendingDisposition {
    target: karma_ipc::ProcessIdentity,
    /// Window title to remember once image-triggered enforcement succeeds.
    remember: Option<TitleCandidate>,
}

#[derive(Debug)]
struct TitleCandidate {
    title: String,
    browser_host: Option<String>,
    reason: TitleReason,
}

#[derive(Default)]
struct AuthenticationLimit {
    failures: u32,
    blocked_until_ms: Option<i64>,
}

#[derive(Clone, Copy)]
enum AuthenticationKind {
    Administrator,
    Super,
    Uninstall,
}

impl AuthenticationKind {
    fn audit_kind(self) -> &'static str {
        match self {
            Self::Administrator => "administrator_authentication",
            Self::Super => "super_password_authentication",
            Self::Uninstall => "uninstall_authentication",
        }
    }

    fn limit(self, runtime: &mut RuntimeState) -> &mut AuthenticationLimit {
        match self {
            Self::Administrator => &mut runtime.administrator_auth,
            Self::Super | Self::Uninstall => &mut runtime.super_auth,
        }
    }
}

/// What the network enforcement layer must apply right now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkEnforcement {
    pub status: NetworkStatus,
    /// Empty unless `status.restricted` is set.
    pub restriction: NetworkRestriction,
}

/// A bounded updater operation authorized by an IPC request. The host uses
/// only its fixed release source or protected update inbox, never caller paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateAction {
    CheckAndInstall,
    ApplyStaged,
}

pub struct ServiceCore {
    started_at_ms: i64,
    local_offset: LocalOffsetFn,
    agent_token: Zeroizing<String>,
    evidence_vault: EvidenceVault,
    runtime: Mutex<RuntimeState>,
}

impl ServiceCore {
    /// Opens the SQLite state database at `database_path`, importing a legacy
    /// JSON state file of the same name (`.json` extension) on first open.
    /// Remembered titles are sealed with a key derived from `evidence_key`.
    pub fn open(
        database_path: impl Into<PathBuf>,
        agent_token: String,
        evidence_directory: impl Into<PathBuf>,
        evidence_key: [u8; 32],
        started_at_ms: i64,
    ) -> Result<Self, CoreError> {
        let (store, stored) = Store::open(&database_path.into(), &evidence_key)
            .map_err(|_| CoreError::StorageUnavailable)?;
        let mut runtime = RuntimeState {
            stored,
            store,
            pending: Vec::new(),
            title_digests: HashSet::new(),
            misjudged_digests: HashSet::new(),
            sessions: HashMap::new(),
            recent_nonces: VecDeque::new(),
            nonce_index: HashSet::new(),
            administrator_auth: AuthenticationLimit::default(),
            super_auth: AuthenticationLimit::default(),
            last_heartbeat: None,
            pending_dispositions: HashMap::new(),
            network_restricted: None,
            network_enforcement_succeeded: None,
            pending_update: None,
            update_status: UpdateStatus::default(),
            shutdown_authorized: false,
        };
        runtime.rebuild_title_digests();
        Ok(Self {
            started_at_ms,
            local_offset: system_local_offset_seconds,
            agent_token: Zeroizing::new(agent_token),
            evidence_vault: EvidenceVault::new(evidence_directory, evidence_key),
            runtime: Mutex::new(runtime),
        })
    }

    /// Replaces the local time zone source, for hosts and tests that must not
    /// depend on the machine's configured zone.
    pub fn with_local_offset(mut self, local_offset: LocalOffsetFn) -> Self {
        self.local_offset = local_offset;
        self
    }

    /// Whether periodic signed updates may install automatically. Explicit
    /// authenticated update requests remain available when this is disabled.
    pub fn auto_install_updates(&self) -> Result<bool, CoreError> {
        let runtime = self
            .runtime
            .lock()
            .map_err(|_| CoreError::LockUnavailable)?;
        Ok(runtime
            .stored
            .policy
            .pointer("/updates/autoInstall")
            .and_then(Value::as_bool)
            .unwrap_or(true))
    }

    /// Takes one previously authenticated updater request. Only one operation
    /// may be queued or in progress at a time.
    pub fn take_update_action(&self) -> Result<Option<UpdateAction>, CoreError> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| CoreError::LockUnavailable)?;
        Ok(runtime.pending_update.take())
    }

    /// Reports host updater progress without accepting executable paths or
    /// process identities through the service protocol.
    pub fn record_update_status(&self, status: UpdateStatus, now_ms: i64) -> Result<(), CoreError> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| CoreError::LockUnavailable)?;
        if runtime.shutdown_authorized && update_phase_in_progress(status.phase) {
            return Err(CoreError::ShutdownAuthorized);
        }
        if runtime.update_status != status {
            let outcome = match status.phase {
                UpdatePhase::Idle => "idle",
                UpdatePhase::Checking => "checking",
                UpdatePhase::Downloading => "downloading",
                UpdatePhase::Installing => "installing",
                UpdatePhase::UpToDate => "up_to_date",
                UpdatePhase::Failed => "failed",
            };
            audit(&mut runtime, now_ms, "signed_update", outcome);
            persist(&mut runtime).map_err(|_| CoreError::StorageUnavailable)?;
            runtime.update_status = status;
        }
        Ok(())
    }

    /// The network schedule restriction to enforce at `now_ms`. Hosts call
    /// this periodically; transitions into and out of a restricted period are
    /// written to the audit log.
    pub fn network_enforcement(&self, now_ms: i64) -> Result<NetworkEnforcement, CoreError> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| CoreError::LockUnavailable)?;
        refresh_uninstall_request(&mut runtime, now_ms);
        let (status, restriction) = self.evaluate_network(&mut runtime, now_ms);
        if runtime.network_restricted != Some(status.restricted) {
            let previous = runtime.network_restricted.replace(status.restricted);
            if previous.is_some() || status.restricted {
                let kind = if status.restricted {
                    "network_schedule_started"
                } else {
                    "network_schedule_ended"
                };
                audit(&mut runtime, now_ms, kind, "success");
                persist(&mut runtime).map_err(|_| CoreError::StorageUnavailable)?;
            }
        }
        if !runtime.pending.is_empty() {
            persist(&mut runtime).map_err(|_| CoreError::StorageUnavailable)?;
        }
        Ok(NetworkEnforcement {
            status,
            restriction: if runtime.network_restricted == Some(true) {
                restriction
            } else {
                NetworkRestriction::default()
            },
        })
    }

    /// Records changes in the host's actual filter application result, separately
    /// from schedule transitions. No paths or traffic data enter the audit log.
    pub fn record_network_enforcement(
        &self,
        succeeded: bool,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| CoreError::LockUnavailable)?;
        if runtime.network_enforcement_succeeded != Some(succeeded) {
            audit(
                &mut runtime,
                now_ms,
                "network_enforcement",
                if succeeded { "success" } else { "failure" },
            );
            persist(&mut runtime).map_err(|_| CoreError::StorageUnavailable)?;
            runtime.network_enforcement_succeeded = Some(succeeded);
        }
        Ok(())
    }

    fn evaluate_network(
        &self,
        runtime: &mut RuntimeState,
        now_ms: i64,
    ) -> (NetworkStatus, NetworkRestriction) {
        if runtime
            .stored
            .network_restore
            .is_some_and(|restore| restore.until_ms <= now_ms)
        {
            runtime.set_network_restore(None);
        }
        let protection_enabled = runtime
            .stored
            .policy
            .get("protectionEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        // An unreadable stored schedule fails closed to "no schedule" only
        // because PutPolicy already rejects every invalid schedule.
        let schedule =
            NetworkSchedulePolicy::from_value(&runtime.stored.policy).unwrap_or_default();
        if !protection_enabled || schedule.is_empty() {
            return (NetworkStatus::default(), NetworkRestriction::default());
        }
        let minute = local_minute_of_week(now_ms, (self.local_offset)(now_ms));
        let restriction = schedule.restriction_at(minute);
        if !restriction.is_restricted() {
            return (NetworkStatus::default(), restriction);
        }
        let restricted_until_ms = schedule.minutes_until_clear(minute).map(|minutes| {
            now_ms
                .div_euclid(MS_PER_MINUTE)
                .saturating_add(i64::from(minutes))
                .saturating_mul(MS_PER_MINUTE)
        });
        let restored_until_ms = runtime
            .stored
            .network_restore
            .map(|restore| restore.until_ms);
        let status = NetworkStatus {
            restricted: restored_until_ms.is_none(),
            active_rule_ids: restriction.rule_ids.clone(),
            restricted_until_ms,
            restored_until_ms,
        };
        (status, restriction)
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
        refresh_uninstall_request(&mut runtime, now_ms);
        let mut result = self.dispatch(&mut runtime, request.request, now_ms);
        // Failed authentication and authorization attempts are audit events too.
        // Commit those events even when dispatch returned a failure.
        if !runtime.pending.is_empty() {
            if let Err(error) = persist(&mut runtime) {
                result = Err(error);
            }
        }
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
        persist(&mut runtime)
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
                runtime.set_password_hash(hash);
                audit(runtime, now_ms, "administrator_enrolled", "success");
                persist(runtime)?;
                Ok(new_session(runtime, now_ms))
            }
            ServiceRequest::Authenticate { password } => {
                self.authenticate(runtime, password, now_ms)?;
                persist(runtime)?;
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
                if new_password.chars().count() < 10
                    || password_matches(
                        runtime.stored.super_password_hash.as_deref(),
                        &new_password,
                    )?
                {
                    audit(runtime, now_ms, "administrator_password_changed", "denied");
                    return Err(ServiceErrorCode::InvalidRequest);
                }
                let salt = SaltString::generate(&mut PasswordOsRng);
                let hash = Argon2::default()
                    .hash_password(new_password.as_bytes(), &salt)
                    .map_err(|_| ServiceErrorCode::Internal)?
                    .to_string();
                runtime.set_password_hash(hash);
                audit(runtime, now_ms, "administrator_password_changed", "success");
                persist(runtime)?;
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::SetSuperPassword {
                session_token,
                admin_password,
                super_password,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                let super_password = Zeroizing::new(super_password);
                if runtime.stored.super_password_hash.is_some() {
                    audit(runtime, now_ms, "super_password_set", "denied");
                    return Err(ServiceErrorCode::AlreadyEnrolled);
                }
                self.authenticate(runtime, admin_password, now_ms)?;
                if super_password.chars().count() < 10
                    || password_matches(runtime.stored.password_hash.as_deref(), &super_password)?
                {
                    audit(runtime, now_ms, "super_password_set", "denied");
                    return Err(ServiceErrorCode::InvalidRequest);
                }
                runtime.set_super_password_hash(hash_password(&super_password)?);
                audit(runtime, now_ms, "super_password_set", "success");
                persist(runtime)?;
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::ChangeSuperPassword {
                session_token,
                current_super_password,
                new_super_password,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                self.authenticate_super(runtime, Some(current_super_password), now_ms)?;
                let new_super_password = Zeroizing::new(new_super_password);
                if new_super_password.chars().count() < 10
                    || password_matches(
                        runtime.stored.password_hash.as_deref(),
                        &new_super_password,
                    )?
                {
                    audit(runtime, now_ms, "super_password_changed", "denied");
                    return Err(ServiceErrorCode::InvalidRequest);
                }
                runtime.set_super_password_hash(hash_password(&new_super_password)?);
                audit(runtime, now_ms, "super_password_changed", "success");
                persist(runtime)?;
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
                    audit_count: runtime.stored.audit_count,
                    network: self.evaluate_network(runtime, now_ms).0,
                    product_version: karma_ipc::PRODUCT_VERSION.into(),
                    super_password_set: runtime.stored.super_password_hash.is_some(),
                    uninstall: runtime.stored.uninstall_request.map(uninstall_status),
                    update: runtime.update_status.clone(),
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
                super_password,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                if expected_revision != runtime.stored.policy_revision {
                    return Err(ServiceErrorCode::RevisionConflict);
                }
                ContextPolicy::from_value(&policy).map_err(|_| ServiceErrorCode::InvalidRequest)?;
                ApplicationPolicy::from_value(&policy)
                    .map_err(|_| ServiceErrorCode::InvalidRequest)?;
                NetworkSchedulePolicy::from_value(&policy)
                    .map_err(|_| ServiceErrorCode::InvalidRequest)?;
                RepeatOffenderRule::from_value(&policy)
                    .map_err(|_| ServiceErrorCode::InvalidRequest)?;
                if runtime.stored.super_password_hash.is_some()
                    && !policy_weakenings(&runtime.stored.policy, &policy).is_empty()
                {
                    if let Err(error) = self.authenticate_super(runtime, super_password, now_ms) {
                        audit(runtime, now_ms, "policy_weakening", "denied");
                        return Err(error);
                    }
                    audit(runtime, now_ms, "policy_weakening", "authorized");
                }
                runtime.set_policy(policy);
                audit(runtime, now_ms, "policy_updated", "success");
                persist(runtime)?;
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
                    enforcement: agent_enforcement(runtime, now_ms),
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
                if protection_enabled
                    && repeat_offender_banned(runtime, &target.executable_name, now_ms)
                {
                    return Ok(disposition_required(
                        runtime,
                        observation.event_id,
                        target,
                        None,
                    ));
                }
                let context_policy = ContextPolicy::from_value(&runtime.stored.policy)
                    .map_err(|_| ServiceErrorCode::Internal)?;
                if context_policy.allows_host(observation.browser_host.as_deref()) {
                    return Ok(ServiceResult::Acknowledged);
                }
                let application_policy = ApplicationPolicy::from_value(&runtime.stored.policy)
                    .map_err(|_| ServiceErrorCode::Internal)?;
                if application_policy.effect_for(&target.executable_name)
                    == ApplicationEffect::Allow
                {
                    return Ok(ServiceResult::Acknowledged);
                }
                if !protection_enabled || !image_enabled || observation.risk_millis < threshold {
                    return Ok(ServiceResult::Acknowledged);
                }
                let reason = TitleReason {
                    reason_code: observation.reason_code,
                    risk_millis: observation.risk_millis,
                    threshold_millis: threshold,
                };
                let remember = remember_titles_enabled(&runtime.stored.policy)
                    .then_some(observation.window_title)
                    .flatten()
                    .filter(|title| {
                        rememberable_title_key(&target.executable_name, title).is_some()
                    })
                    .map(|title| TitleCandidate {
                        title,
                        browser_host: observation.browser_host,
                        reason,
                    });
                Ok(disposition_required(
                    runtime,
                    observation.event_id,
                    target,
                    remember,
                ))
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
                if repeat_offender_banned(runtime, &observation.source.executable_name, now_ms) {
                    return Ok(disposition_required(
                        runtime,
                        observation.event_id,
                        observation.source,
                        None,
                    ));
                }
                let context_policy = ContextPolicy::from_value(&runtime.stored.policy)
                    .map_err(|_| ServiceErrorCode::Internal)?;
                let application_policy = ApplicationPolicy::from_value(&runtime.stored.policy)
                    .map_err(|_| ServiceErrorCode::Internal)?;
                // Remembered titles yield to the same exemptions that would
                // have prevented the original image enforcement.
                if remember_titles_enabled(&runtime.stored.policy)
                    && !context_policy.allows_host(observation.browser_host.as_deref())
                    && application_policy.effect_for(&observation.source.executable_name)
                        != ApplicationEffect::Allow
                    && runtime.matches_remembered_title(
                        &observation.source.executable_name,
                        &observation.window_title,
                    )
                {
                    return Ok(disposition_required(
                        runtime,
                        observation.event_id,
                        observation.source,
                        None,
                    ));
                }
                let verdict = context_policy.evaluate(
                    observation.browser_host.as_deref(),
                    &observation.window_title,
                );
                let application_effect =
                    application_policy.effect_for(&observation.source.executable_name);
                if !karma_policy::context_enforcement(verdict, application_effect) {
                    return Ok(ServiceResult::Acknowledged);
                }
                Ok(disposition_required(
                    runtime,
                    observation.event_id,
                    observation.source,
                    None,
                ))
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
                runtime.insert_evidence(StoredEvidence {
                    id: evidence.evidence_id.clone(),
                    captured_at_ms: evidence.captured_at_ms,
                    monitor_name: evidence.monitor_name.clone(),
                    application_name: evidence.application_name.clone(),
                    reason_code: evidence.reason_code.clone(),
                    risk_millis: evidence.risk_millis,
                    media_type: evidence.media_type.clone(),
                });
                audit(runtime, now_ms, "evidence_stored", "success");
                persist(runtime)?;
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::ReportDisposition {
                agent_token,
                report,
            } => {
                self.authorize_agent(&agent_token)?;
                record_disposition(runtime, &report, now_ms)?;
                persist(runtime)?;
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
                self.delete_evidence(runtime, &[evidence_id], now_ms)
            }
            ServiceRequest::DeleteEvidenceBatch {
                session_token,
                evidence_ids,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                self.delete_evidence(runtime, &evidence_ids, now_ms)
            }
            ServiceRequest::ListEnforcement {
                session_token,
                title_offset,
                title_limit,
                title_query,
                misjudged,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                Ok(ServiceResult::Enforcement(enforcement_summary(
                    runtime,
                    now_ms,
                    title_offset as usize,
                    title_limit as usize,
                    title_query.as_deref(),
                    misjudged,
                )))
            }
            ServiceRequest::MarkTitleMisjudged {
                session_token,
                title_id,
                super_password,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                if runtime.stored.super_password_hash.is_some() {
                    if let Err(error) = self.authenticate_super(runtime, super_password, now_ms) {
                        audit(runtime, now_ms, "remembered_title_misjudged", "denied");
                        return Err(error);
                    }
                }
                let index = runtime
                    .stored
                    .remembered_titles
                    .iter()
                    .position(|record| record.id == title_id)
                    .ok_or(ServiceErrorCode::InvalidRequest)?;
                runtime.mark_misjudged(index, now_ms);
                audit(runtime, now_ms, "remembered_title_misjudged", "success");
                persist(runtime)?;
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::RestoreMisjudgedTitle {
                session_token,
                title_id,
            } => {
                // Remembering the title again only strengthens protection.
                authorize(runtime, &session_token, now_ms)?;
                let index = runtime
                    .stored
                    .misjudged_titles
                    .iter()
                    .position(|record| record.id == title_id)
                    .ok_or(ServiceErrorCode::InvalidRequest)?;
                runtime.restore_misjudged(index);
                audit(runtime, now_ms, "misjudged_title_restored", "success");
                persist(runtime)?;
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::ForgetRememberedTitle {
                session_token,
                title_id,
                super_password,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                if runtime.stored.super_password_hash.is_some() {
                    if let Err(error) = self.authenticate_super(runtime, super_password, now_ms) {
                        audit(runtime, now_ms, "remembered_title_forgotten", "denied");
                        return Err(error);
                    }
                }
                let index = runtime
                    .stored
                    .remembered_titles
                    .iter()
                    .position(|record| record.id == title_id)
                    .ok_or(ServiceErrorCode::InvalidRequest)?;
                runtime.forget_title(index);
                audit(runtime, now_ms, "remembered_title_forgotten", "success");
                persist(runtime)?;
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::AllowBannedApplication {
                session_token,
                expected_revision,
                executable,
                super_password,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                // Only an active ban can be lifted here; any other allow rule
                // goes through the policy comparison in PutPolicy.
                let executable = normalize_executable(&executable);
                if !is_banned(&runtime.stored.repeat_offenders, &executable, now_ms) {
                    return Err(ServiceErrorCode::InvalidRequest);
                }
                if expected_revision != runtime.stored.policy_revision {
                    return Err(ServiceErrorCode::RevisionConflict);
                }
                if runtime.stored.super_password_hash.is_some() {
                    if let Err(error) = self.authenticate_super(runtime, super_password, now_ms) {
                        audit(runtime, now_ms, "banned_application_allowed", "denied");
                        return Err(error);
                    }
                }
                let policy = allow_application(
                    &runtime.stored.policy,
                    &executable,
                    &format!("app-{}", random_token()),
                )
                .map_err(|_| ServiceErrorCode::InvalidRequest)?;
                runtime.set_policy(policy);
                runtime.lift_ban(&executable);
                audit(runtime, now_ms, "banned_application_allowed", "success");
                persist(runtime)?;
                Ok(ServiceResult::Policy {
                    revision: runtime.stored.policy_revision,
                    policy: runtime.stored.policy.clone(),
                })
            }
            ServiceRequest::RestoreNetwork {
                session_token,
                password,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                if runtime.stored.super_password_hash.is_some() {
                    self.authenticate_super(runtime, Some(password), now_ms)?;
                } else {
                    self.authenticate(runtime, password, now_ms)?;
                }
                let (status, _) = self.evaluate_network(runtime, now_ms);
                if !status.restricted {
                    return Ok(ServiceResult::NetworkStatus(status));
                }
                let until_ms = status
                    .restricted_until_ms
                    .ok_or(ServiceErrorCode::Internal)?;
                runtime.set_network_restore(Some(NetworkRestore {
                    granted_at_ms: now_ms,
                    until_ms,
                }));
                audit(
                    runtime,
                    now_ms,
                    "network_restored_by_administrator",
                    "success",
                );
                persist(runtime)?;
                Ok(ServiceResult::NetworkStatus(
                    self.evaluate_network(runtime, now_ms).0,
                ))
            }
            ServiceRequest::RequestUninstall {
                session_token,
                password,
            } => {
                authorize(runtime, &session_token, now_ms)?;
                self.authenticate(runtime, password, now_ms)?;
                let effective_at_ms = now_ms
                    .checked_add(UNINSTALL_WAIT_MS)
                    .filter(|effective| effective.checked_add(UNINSTALL_VALIDITY_MS).is_some())
                    .ok_or(ServiceErrorCode::InvalidRequest)?;
                runtime.set_uninstall_request(Some(UninstallRequest {
                    requested_at_ms: now_ms,
                    effective_at_ms,
                    ready_audited: false,
                }));
                audit(runtime, now_ms, "uninstall_requested", "success");
                persist(runtime)?;
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::CancelUninstall { session_token } => {
                authorize(runtime, &session_token, now_ms)?;
                runtime.set_uninstall_request(None);
                audit(runtime, now_ms, "uninstall_cancelled", "success");
                persist(runtime)?;
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::CheckForUpdates { session_token } => {
                authorize(runtime, &session_token, now_ms)?;
                queue_update(runtime, UpdateAction::CheckAndInstall, now_ms)?;
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::ApplyStagedUpdate { password } => {
                self.authenticate(runtime, password, now_ms)?;
                queue_update(runtime, UpdateAction::ApplyStaged, now_ms)?;
                Ok(ServiceResult::Acknowledged)
            }
            ServiceRequest::RequestShutdown { password } => {
                if runtime.pending_update.is_some() || update_in_progress(runtime) {
                    audit(
                        runtime,
                        now_ms,
                        "uninstall_authorization",
                        "update_in_progress",
                    );
                    return Err(ServiceErrorCode::ServiceUnavailable);
                }
                if runtime.stored.super_password_hash.is_some() {
                    let password = Zeroizing::new(password);
                    self.check_authentication_limit(
                        runtime,
                        AuthenticationKind::Uninstall,
                        now_ms,
                    )?;
                    let admin_allowed = runtime
                        .stored
                        .uninstall_request
                        .map(uninstall_status)
                        .is_some_and(|request| {
                            now_ms >= request.effective_at_ms && now_ms < request.expires_at_ms
                        });
                    let valid =
                        password_matches(runtime.stored.super_password_hash.as_deref(), &password)?
                            || (admin_allowed
                                && password_matches(
                                    runtime.stored.password_hash.as_deref(),
                                    &password,
                                )?);
                    self.finish_authentication(
                        runtime,
                        valid,
                        AuthenticationKind::Uninstall,
                        now_ms,
                    )?;
                } else {
                    self.authenticate(runtime, password, now_ms)?;
                }
                audit(runtime, now_ms, "service_shutdown_requested", "success");
                persist(runtime)?;
                runtime.shutdown_authorized = true;
                Ok(ServiceResult::Acknowledged)
            }
        }
    }

    fn authenticate(
        &self,
        runtime: &mut RuntimeState,
        password: String,
        now_ms: i64,
    ) -> Result<(), ServiceErrorCode> {
        let password = Zeroizing::new(password);
        self.check_authentication_limit(runtime, AuthenticationKind::Administrator, now_ms)?;
        if runtime.stored.password_hash.is_none() {
            return Err(ServiceErrorCode::NotEnrolled);
        }
        let valid = password_matches(runtime.stored.password_hash.as_deref(), &password)?;
        self.finish_authentication(runtime, valid, AuthenticationKind::Administrator, now_ms)
    }

    /// Deletes the named evidence items, skipping ids that no longer exist. An
    /// item whose encrypted original is already gone still loses its record.
    fn delete_evidence(
        &self,
        runtime: &mut RuntimeState,
        evidence_ids: &[String],
        now_ms: i64,
    ) -> Result<ServiceResult, ServiceErrorCode> {
        let mut deleted = false;
        for evidence_id in evidence_ids {
            let Some(index) = runtime
                .stored
                .evidence
                .iter()
                .position(|item| &item.id == evidence_id)
            else {
                continue;
            };
            if self.evidence_vault.delete(evidence_id).is_err()
                && self.evidence_vault.exists(evidence_id)
            {
                return Err(ServiceErrorCode::EvidenceUnavailable);
            }
            runtime.remove_evidence(index);
            audit(runtime, now_ms, "evidence_deleted", "success");
            deleted = true;
        }
        if !deleted {
            return Err(ServiceErrorCode::EvidenceUnavailable);
        }
        persist(runtime)?;
        Ok(ServiceResult::Acknowledged)
    }

    fn authenticate_super(
        &self,
        runtime: &mut RuntimeState,
        password: Option<String>,
        now_ms: i64,
    ) -> Result<(), ServiceErrorCode> {
        let password = password.map(Zeroizing::new);
        self.check_authentication_limit(runtime, AuthenticationKind::Super, now_ms)?;
        let Some(password) = password.filter(|_| runtime.stored.super_password_hash.is_some())
        else {
            audit(runtime, now_ms, "super_password_authentication", "denied");
            return Err(ServiceErrorCode::SuperPasswordRequired);
        };
        let valid = password_matches(runtime.stored.super_password_hash.as_deref(), &password)?;
        self.finish_authentication(runtime, valid, AuthenticationKind::Super, now_ms)
    }

    fn check_authentication_limit(
        &self,
        runtime: &mut RuntimeState,
        kind: AuthenticationKind,
        now_ms: i64,
    ) -> Result<(), ServiceErrorCode> {
        if kind
            .limit(runtime)
            .blocked_until_ms
            .is_some_and(|until| until > now_ms)
        {
            audit(runtime, now_ms, kind.audit_kind(), "rate_limited");
            return Err(ServiceErrorCode::RateLimited);
        }
        Ok(())
    }

    fn finish_authentication(
        &self,
        runtime: &mut RuntimeState,
        valid: bool,
        kind: AuthenticationKind,
        now_ms: i64,
    ) -> Result<(), ServiceErrorCode> {
        if valid {
            let limit = kind.limit(runtime);
            limit.failures = 0;
            limit.blocked_until_ms = None;
            let kind = if matches!(kind, AuthenticationKind::Administrator) {
                "administrator_authenticated"
            } else {
                kind.audit_kind()
            };
            audit(runtime, now_ms, kind, "success");
            return Ok(());
        }
        let limit = kind.limit(runtime);
        limit.failures = limit.failures.saturating_add(1);
        if limit.failures >= MAX_FAILURES {
            limit.failures = 0;
            limit.blocked_until_ms = Some(now_ms.saturating_add(FAILURE_COOLDOWN_MS));
        }
        audit(runtime, now_ms, kind.audit_kind(), "denied");
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
}

fn password_matches(stored_hash: Option<&str>, password: &str) -> Result<bool, ServiceErrorCode> {
    let Some(stored_hash) = stored_hash else {
        return Ok(false);
    };
    let parsed =
        PasswordHash::new(stored_hash).map_err(|_| ServiceErrorCode::StorageUnavailable)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

fn hash_password(password: &str) -> Result<String, ServiceErrorCode> {
    let salt = SaltString::generate(&mut PasswordOsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| ServiceErrorCode::Internal)
}

fn uninstall_status(request: UninstallRequest) -> UninstallStatus {
    UninstallStatus {
        requested_at_ms: request.requested_at_ms,
        effective_at_ms: request.effective_at_ms,
        expires_at_ms: request
            .effective_at_ms
            .saturating_add(UNINSTALL_VALIDITY_MS),
    }
}

fn refresh_uninstall_request(runtime: &mut RuntimeState, now_ms: i64) {
    let Some(mut request) = runtime.stored.uninstall_request else {
        return;
    };
    if now_ms >= uninstall_status(request).expires_at_ms {
        runtime.set_uninstall_request(None);
        audit(runtime, now_ms, "uninstall_request_expired", "success");
    } else if now_ms >= request.effective_at_ms && !request.ready_audited {
        request.ready_audited = true;
        runtime.set_uninstall_request(Some(request));
        audit(runtime, now_ms, "uninstall_request_ready", "success");
    }
}

fn queue_update(
    runtime: &mut RuntimeState,
    action: UpdateAction,
    now_ms: i64,
) -> Result<(), ServiceErrorCode> {
    if runtime.shutdown_authorized
        || runtime.pending_update.is_some()
        || update_in_progress(runtime)
    {
        audit(runtime, now_ms, "signed_update_requested", "busy");
        return Err(ServiceErrorCode::ServiceUnavailable);
    }
    audit(
        runtime,
        now_ms,
        "signed_update_requested",
        match action {
            UpdateAction::CheckAndInstall => "check_and_install",
            UpdateAction::ApplyStaged => "apply_staged",
        },
    );
    persist(runtime)?;
    runtime.pending_update = Some(action);
    runtime.update_status.phase = UpdatePhase::Checking;
    runtime.update_status.last_error = None;
    Ok(())
}

fn update_in_progress(runtime: &RuntimeState) -> bool {
    update_phase_in_progress(runtime.update_status.phase)
}

fn update_phase_in_progress(phase: UpdatePhase) -> bool {
    matches!(
        phase,
        UpdatePhase::Checking | UpdatePhase::Downloading | UpdatePhase::Installing
    )
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

impl RuntimeState {
    fn set_password_hash(&mut self, hash: String) {
        self.stored.password_hash = Some(hash.clone());
        self.pending.push(Change::PasswordHash(hash));
    }

    fn set_super_password_hash(&mut self, hash: String) {
        self.stored.super_password_hash = Some(hash.clone());
        self.pending.push(Change::SuperPasswordHash(hash));
    }

    fn set_uninstall_request(&mut self, request: Option<UninstallRequest>) {
        self.stored.uninstall_request = request;
        self.pending.push(Change::UninstallRequest(request));
    }

    fn set_policy(&mut self, policy: Value) {
        self.stored.policy_revision = self.stored.policy_revision.saturating_add(1);
        self.stored.policy = policy.clone();
        self.pending.push(Change::Policy {
            revision: self.stored.policy_revision,
            policy,
        });
    }

    fn set_network_restore(&mut self, restore: Option<NetworkRestore>) {
        self.stored.network_restore = restore;
        self.pending.push(Change::NetworkRestore(restore));
    }

    fn insert_evidence(&mut self, item: StoredEvidence) {
        self.pending.push(Change::InsertEvidence(item.clone()));
        self.stored.evidence.push(item);
    }

    fn remove_evidence(&mut self, index: usize) {
        let item = self.stored.evidence.remove(index);
        self.pending.push(Change::DeleteEvidence(item.id));
    }

    /// Remembers `entry` and reports whether it is new; a known title only
    /// has its timestamp refreshed, and the cap may evict the oldest title.
    fn remember(&mut self, entry: RememberedTitle) -> bool {
        let executable = entry.executable.clone();
        let key = title_key(&entry.title);
        let before: HashSet<String> = self
            .stored
            .remembered_titles
            .iter()
            .map(|record| record.id.clone())
            .collect();
        let added = remember_title(&mut self.stored.remembered_titles, entry);
        let remaining: HashSet<&str> = self
            .stored
            .remembered_titles
            .iter()
            .map(|record| record.id.as_str())
            .collect();
        let evicted: Vec<String> = before
            .into_iter()
            .filter(|id| !remaining.contains(id.as_str()))
            .collect();
        self.pending
            .extend(evicted.into_iter().map(Change::DeleteTitle));
        if let Some(record) = self
            .stored
            .remembered_titles
            .iter()
            .find(|record| record.executable == executable && title_key(&record.title) == key)
        {
            self.pending.push(Change::UpsertTitle(record.clone()));
        }
        self.rebuild_title_digests();
        added
    }

    /// Clears the executable's ban and closure history, so closures before
    /// the ban cannot re-establish it.
    fn lift_ban(&mut self, executable: &str) {
        self.stored
            .repeat_offenders
            .retain(|record| record.executable != executable);
        self.pending.push(Change::RepeatOffenders(
            self.stored.repeat_offenders.clone(),
        ));
    }

    fn forget_title(&mut self, index: usize) {
        let record = self.stored.remembered_titles.remove(index);
        self.pending.push(Change::DeleteTitle(record.id));
        self.rebuild_title_digests();
    }

    /// Forgets the remembered title at `index` and records it as misjudged,
    /// dropping the oldest misjudged titles beyond the cap.
    fn mark_misjudged(&mut self, index: usize, now_ms: i64) {
        let record = self.stored.remembered_titles[index].clone();
        self.forget_title(index);
        if self.is_misjudged(&record.executable, &record.title) {
            return;
        }
        let misjudged = RememberedTitle {
            id: format!("misjudged-{}", random_token()),
            remembered_at_ms: now_ms,
            reason: None,
            ..record
        };
        self.pending
            .push(Change::InsertMisjudgedTitle(misjudged.clone()));
        self.stored.misjudged_titles.push(misjudged);
        while self.stored.misjudged_titles.len() > MAX_MISJUDGED_TITLES {
            let oldest = self
                .stored
                .misjudged_titles
                .iter()
                .enumerate()
                .min_by_key(|(_, record)| record.remembered_at_ms)
                .map_or(0, |(index, _)| index);
            let removed = self.stored.misjudged_titles.remove(oldest);
            self.pending.push(Change::DeleteMisjudgedTitle(removed.id));
        }
        self.rebuild_title_digests();
    }

    fn restore_misjudged(&mut self, index: usize) {
        let record = self.stored.misjudged_titles.remove(index);
        self.pending.push(Change::DeleteMisjudgedTitle(record.id));
        self.rebuild_title_digests();
    }

    fn is_misjudged(&self, executable: &str, title: &str) -> bool {
        title_digest(executable, title)
            .is_some_and(|digest| self.misjudged_digests.contains(&digest))
    }

    fn rebuild_title_digests(&mut self) {
        self.misjudged_digests = self
            .stored
            .misjudged_titles
            .iter()
            .filter_map(|record| title_digest(&record.executable, &record.title))
            .collect();
        self.title_digests = self
            .stored
            .remembered_titles
            .iter()
            .filter_map(|record| title_digest(&record.executable, &record.title))
            .collect();
    }

    fn matches_remembered_title(&self, executable_name: &str, title: &str) -> bool {
        title_digest(&normalize_executable(executable_name), title)
            .is_some_and(|digest| self.title_digests.contains(&digest))
    }
}

/// Commits every queued change in one transaction. A failed commit keeps the
/// queue so the next successful commit still writes it.
fn persist(runtime: &mut RuntimeState) -> Result<(), ServiceErrorCode> {
    runtime
        .store
        .apply(&runtime.pending)
        .map_err(|_| ServiceErrorCode::StorageUnavailable)?;
    runtime.pending.clear();
    Ok(())
}

fn audit(runtime: &mut RuntimeState, occurred_at_ms: i64, kind: &str, outcome: &str) {
    runtime.pending.push(Change::Audit(AuditRecord {
        occurred_at_ms,
        kind: kind.into(),
        outcome: outcome.into(),
    }));
    runtime.stored.audit_count = runtime
        .stored
        .audit_count
        .saturating_add(1)
        .min(MAX_AUDIT_ENTRIES);
}

fn repeat_offender_banned(runtime: &RuntimeState, executable_name: &str, now_ms: i64) -> bool {
    is_banned(
        &runtime.stored.repeat_offenders,
        &normalize_executable(executable_name),
        now_ms,
    )
}

/// Bans and remembered titles the agent must enforce; empty while the
/// corresponding protection is switched off.
fn agent_enforcement(runtime: &RuntimeState, now_ms: i64) -> AgentEnforcement {
    let policy = &runtime.stored.policy;
    if !policy
        .get("protectionEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        return AgentEnforcement::default();
    }
    AgentEnforcement {
        banned_executables: active_bans(&runtime.stored.repeat_offenders, now_ms)
            .map(|(executable, _)| executable.to_owned())
            .collect(),
        remembered_title_digests: if remember_titles_enabled(policy) {
            let mut digests: Vec<_> = runtime.title_digests.iter().cloned().collect();
            digests.sort_unstable();
            digests
        } else {
            Vec::new()
        },
    }
}

/// One page of remembered (or misjudged) titles, newest first, whose title,
/// host, or executable contains `query` (compared like titles are matched).
fn enforcement_summary(
    runtime: &RuntimeState,
    now_ms: i64,
    offset: usize,
    limit: usize,
    query: Option<&str>,
    misjudged: bool,
) -> EnforcementSummary {
    let query = query.map(title_key).filter(|query| !query.is_empty());
    let titles = if misjudged {
        &runtime.stored.misjudged_titles
    } else {
        &runtime.stored.remembered_titles
    };
    let mut matching: Vec<&RememberedTitle> = titles
        .iter()
        .filter(|record| {
            query.as_ref().is_none_or(|query| {
                title_key(&record.title).contains(query.as_str())
                    || record.executable.contains(query.as_str())
                    || record
                        .browser_host
                        .as_deref()
                        .is_some_and(|host| title_key(host).contains(query.as_str()))
            })
        })
        .collect();
    matching.sort_by(|left, right| {
        right
            .remembered_at_ms
            .cmp(&left.remembered_at_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    EnforcementSummary {
        remembered_title_total: matching.len() as u64,
        remembered_titles: matching
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|record| RememberedTitleMetadata {
                id: record.id.clone(),
                executable: record.executable.clone(),
                title: record.title.clone(),
                browser_host: record.browser_host.clone(),
                remembered_at_ms: record.remembered_at_ms,
                reason: record.reason.as_ref().map(|reason| TitleReasonMetadata {
                    reason_code: reason.reason_code.clone(),
                    risk_millis: reason.risk_millis,
                    threshold_millis: reason.threshold_millis,
                }),
            })
            .collect(),
        active_bans: active_bans(&runtime.stored.repeat_offenders, now_ms)
            .map(|(executable, banned_until_ms)| ActiveBan {
                executable: executable.to_owned(),
                banned_until_ms,
            })
            .collect(),
        misjudged_title_total: runtime.stored.misjudged_titles.len() as u64,
    }
}

fn disposition_required(
    runtime: &mut RuntimeState,
    event_id: String,
    target: karma_ipc::ProcessIdentity,
    remember: Option<TitleCandidate>,
) -> ServiceResult {
    runtime.pending_dispositions.insert(
        event_id.clone(),
        PendingDisposition {
            target: target.clone(),
            remember,
        },
    );
    ServiceResult::DispositionRequired {
        event_id,
        target,
        grace_period_ms: DISPOSITION_GRACE_MS,
    }
}

fn record_disposition(
    runtime: &mut RuntimeState,
    report: &karma_ipc::DispositionReport,
    now_ms: i64,
) -> Result<(), ServiceErrorCode> {
    let expected = &runtime
        .pending_dispositions
        .get(&report.event_id)
        .ok_or(ServiceErrorCode::InvalidRequest)?
        .target;
    if expected.process_id != report.process_id || expected.started_at_ms != report.started_at_ms {
        return Err(ServiceErrorCode::InvalidRequest);
    }
    let Some(PendingDisposition { target, remember }) =
        runtime.pending_dispositions.remove(&report.event_id)
    else {
        return Err(ServiceErrorCode::InvalidRequest);
    };
    let executable_name = target.executable_name;
    let outcome = match report.outcome {
        karma_ipc::DispositionOutcome::ClosedGracefully => "closed_gracefully",
        karma_ipc::DispositionOutcome::Terminated => "terminated",
        karma_ipc::DispositionOutcome::IdentityChanged => "identity_changed",
        karma_ipc::DispositionOutcome::AccessDenied => "access_denied",
        karma_ipc::DispositionOutcome::SourceUncertain => "source_uncertain",
    };
    audit(runtime, now_ms, "application_disposition", outcome);
    if matches!(
        report.outcome,
        karma_ipc::DispositionOutcome::ClosedGracefully | karma_ipc::DispositionOutcome::Terminated
    ) {
        let executable = normalize_executable(&executable_name);
        // The window is still closed; a misjudged title is just not remembered.
        if let Some(candidate) =
            remember.filter(|candidate| !runtime.is_misjudged(&executable, &candidate.title))
        {
            let entry = RememberedTitle {
                id: format!("title-{}", random_token()),
                executable: executable.clone(),
                title: candidate.title,
                browser_host: candidate.browser_host,
                remembered_at_ms: now_ms,
                reason: Some(candidate.reason),
            };
            if runtime.remember(entry) {
                audit(runtime, now_ms, "window_title_remembered", "success");
            }
        }
        let rule = RepeatOffenderRule::from_value(&runtime.stored.policy).unwrap_or_default();
        let banned = record_closure(
            &mut runtime.stored.repeat_offenders,
            rule,
            &executable,
            report.process_id,
            report.started_at_ms,
            now_ms,
        );
        runtime.pending.push(Change::RepeatOffenders(
            runtime.stored.repeat_offenders.clone(),
        ));
        if banned {
            audit(runtime, now_ms, "repeat_offender_banned", &executable);
        }
    }
    Ok(())
}

fn failure(request_id: String, code: ServiceErrorCode) -> ResponseEnvelope {
    ResponseEnvelope::failure(request_id, ServiceFailure::new(code))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::*;
    use karma_ipc::{
        AgentContextObservation, AgentHeartbeat, AgentObservation, ClientKind, DispositionOutcome,
        DispositionReport, EvidenceSubmission, ProcessIdentity, ServiceRequest,
    };
    use serde_json::json;

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

    /// What the database holds, read through a second connection.
    struct DiskState {
        audit: Vec<AuditRecord>,
        repeat_offenders: Vec<karma_policy::StrikeRecord>,
        remembered_titles: Vec<RememberedTitle>,
        misjudged_titles: Vec<RememberedTitle>,
    }

    fn disk_state(directory: &Path) -> DiskState {
        let path = directory.join("service.db");
        let (_, state) = Store::open(&path, &[7; 32]).unwrap();
        DiskState {
            audit: store::audit_records(&path),
            repeat_offenders: state.repeat_offenders,
            remembered_titles: state.remembered_titles,
            misjudged_titles: state.misjudged_titles,
        }
    }

    fn open_core(directory: &Path, started_at_ms: i64) -> ServiceCore {
        ServiceCore::open(
            directory.join("service.db"),
            "agent-secret".into(),
            directory.join("evidence"),
            [7; 32],
            started_at_ms,
        )
        .unwrap()
    }

    #[test]
    fn network_filter_failures_and_recovery_are_audited_without_poll_spam() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        core.record_network_enforcement(true, 1).unwrap();
        core.record_network_enforcement(true, 2).unwrap();
        core.record_network_enforcement(false, 3).unwrap();
        core.record_network_enforcement(false, 4).unwrap();
        core.record_network_enforcement(true, 5).unwrap();
        let stored = disk_state(directory.path());
        let events = stored
            .audit
            .iter()
            .filter(|event| event.kind == "network_enforcement")
            .map(|event| (event.occurred_at_ms, event.outcome.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(events, vec![(1, "success"), (3, "failure"), (5, "success")]);
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
                    super_password: None,
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

    fn context_observation_at(
        event_id: &str,
        occurred_at_ms: i64,
        title: &str,
        browser_host: Option<&str>,
        process_id: u32,
    ) -> AgentContextObservation {
        let mut observation = context_observation(event_id, title, browser_host);
        observation.occurred_at_ms = occurred_at_ms;
        observation.source.process_id = process_id;
        observation
    }

    fn submit_context_at(
        core: &ServiceCore,
        nonce: &str,
        observation: AgentContextObservation,
        now_ms: i64,
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
            now_ms,
        ))
    }

    fn repeat_offender_policy() -> Value {
        json!({
            "protectionEnabled": true,
            "recognition": {"imageEnabled": false, "titleMatchingEnabled": true},
            "websites": [
                {"id":"blocked","pattern":"blocked.example","action":"block","enabled":true},
                {"id":"allowed","pattern":"safe.example","action":"allow","enabled":true}
            ],
            "applications": [
                {"id":"browser","name":"浏览器","executable":"chrome.exe","category":"browser","action":"allow","enabled":true}
            ]
        })
    }

    /// Closures within the default 20-minute window that establish a ban.
    const DEFAULT_STRIKES: u32 = 5;

    fn establish_repeat_offender_ban(core: &ServiceCore, base_ms: i64) {
        for index in 0..DEFAULT_STRIKES {
            let at = base_ms + i64::from(index) * 1_000;
            let event_id = format!("strike-{index}");
            let observation =
                context_observation_at(&event_id, at, "", Some("www.blocked.example"), 100 + index);
            match submit_context_at(core, &format!("strike-nonce-{index}"), observation, at + 1) {
                ServiceResult::DispositionRequired {
                    event_id: struck, ..
                } => {
                    assert_eq!(struck, event_id);
                }
                _ => panic!("strike disposition should be required"),
            }
            core.record_disposition(
                &DispositionReport {
                    event_id,
                    process_id: 100 + index,
                    started_at_ms: 77,
                    outcome: DispositionOutcome::Terminated,
                },
                at + 2,
            )
            .unwrap();
        }
    }

    #[test]
    fn fifth_closed_program_is_terminated_on_sight_for_one_hour() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(&core, repeat_offender_policy());
        let base = 10_000_i64;
        establish_repeat_offender_ban(&core, base);

        // A benign relaunch on an allowlisted site of an allowed app is
        // still terminated immediately: the ban outranks all exemptions.
        let now = base + 30 * 60 * 1000;
        let relaunch = context_observation_at(
            "relaunch",
            now - 1,
            "项目进度评审会议",
            Some("safe.example"),
            200,
        );
        assert!(matches!(
            submit_context_at(&core, "relaunch-nonce", relaunch, now),
            ServiceResult::DispositionRequired { event_id, grace_period_ms: 0, .. }
                if event_id == "relaunch"
        ));

        // One hour after the fifth closure the program may run again.
        let now = base + 4_000 + 60 * 60 * 1000 + 5;
        let after_ban = context_observation_at(
            "after-ban",
            now - 1,
            "项目进度评审会议",
            Some("safe.example"),
            300,
        );
        assert!(matches!(
            submit_context_at(&core, "after-ban-nonce", after_ban, now),
            ServiceResult::Acknowledged
        ));
    }

    fn allow_banned(
        core: &ServiceCore,
        session: &str,
        nonce: &str,
        expected_revision: u64,
        executable: &str,
        super_password: Option<&str>,
        now_ms: i64,
    ) -> Result<ServiceResult, ServiceErrorCode> {
        core.handle(
            request(
                nonce,
                nonce,
                ServiceRequest::AllowBannedApplication {
                    session_token: session.into(),
                    expected_revision,
                    executable: executable.into(),
                    super_password: super_password.map(str::to_owned),
                },
            ),
            now_ms,
        )
        .result
        .map_err(|failure| failure.code)
    }

    #[test]
    fn allowing_a_banned_application_requires_super_password_and_lifts_the_ban() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(&core, repeat_offender_policy());
        establish_repeat_offender_ban(&core, 10_000);
        let session = authenticate_ui(&core, "auth-allow-banned", 20_000);
        set_super(&core, &session, 20_001);
        let banned = r"C:\Browser\chrome.exe";

        assert_eq!(
            allow_banned(&core, &session, "no-super", 1, banned, None, 20_002).err(),
            Some(ServiceErrorCode::SuperPasswordRequired)
        );
        assert_eq!(
            allow_banned(
                &core,
                &session,
                "not-banned",
                1,
                r"C:\Other\other.exe",
                Some("distinct-super-password"),
                20_003
            )
            .err(),
            Some(ServiceErrorCode::InvalidRequest)
        );
        assert_eq!(
            allow_banned(
                &core,
                &session,
                "stale",
                0,
                banned,
                Some("distinct-super-password"),
                20_004
            )
            .err(),
            Some(ServiceErrorCode::RevisionConflict)
        );
        assert_eq!(
            list_enforcement(&core, &session, "still-banned", 20_005)
                .active_bans
                .len(),
            1
        );

        let Ok(ServiceResult::Policy { revision, policy }) = allow_banned(
            &core,
            &session,
            "allowed",
            1,
            banned,
            Some("distinct-super-password"),
            20_006,
        ) else {
            panic!("expected the updated policy");
        };
        assert_eq!(revision, 2);
        let rule = policy["applications"]
            .as_array()
            .unwrap()
            .iter()
            .find(|rule| rule["executable"] == r"c:\browser\chrome.exe")
            .unwrap();
        assert_eq!(
            (&rule["action"], &rule["enabled"]),
            (&json!("allow"), &json!(true))
        );
        assert!(
            list_enforcement(&core, &session, "lifted", 20_007)
                .active_bans
                .is_empty()
        );
        assert!(
            agent_enforcement_at(&core, "agent-lifted", 20_008)
                .banned_executables
                .is_empty()
        );
        let relaunch = context_observation_at("relaunch", 20_008, "项目进度评审会议", None, 300);
        assert!(matches!(
            submit_context_at(&core, "relaunch-nonce", relaunch, 20_009),
            ServiceResult::Acknowledged
        ));
        let outcomes: Vec<_> = disk_state(directory.path())
            .audit
            .iter()
            .filter(|event| event.kind == "banned_application_allowed")
            .map(|event| event.outcome.clone())
            .collect();
        assert_eq!(outcomes, ["denied", "success"]);
    }

    #[test]
    fn allowing_a_banned_application_uses_the_session_without_super_password() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(&core, repeat_offender_policy());
        establish_repeat_offender_ban(&core, 10_000);
        let session = authenticate_ui(&core, "auth-allow-session", 20_000);

        assert!(matches!(
            allow_banned(
                &core,
                &session,
                "session-only",
                1,
                r"C:\Browser\chrome.exe",
                None,
                20_001
            ),
            Ok(ServiceResult::Policy { revision: 2, .. })
        ));
        assert!(
            list_enforcement(&core, &session, "session-lifted", 20_002)
                .active_bans
                .is_empty()
        );
    }

    #[test]
    fn repeat_offender_ban_survives_restart_and_is_audited() {
        let directory = tempfile::tempdir().unwrap();
        let base = 10_000_i64;
        {
            let core = open_core(directory.path(), 0);
            configure_policy(&core, repeat_offender_policy());
            establish_repeat_offender_ban(&core, base);
        }
        let reopened = open_core(directory.path(), base + 60 * 60 * 1000);
        let now = base + 30 * 60 * 1000;
        let relaunch = context_observation_at(
            "relaunch",
            now - 1,
            "项目进度评审会议",
            Some("safe.example"),
            200,
        );
        assert!(matches!(
            submit_context_at(&reopened, "relaunch-nonce", relaunch, now),
            ServiceResult::DispositionRequired { event_id, .. } if event_id == "relaunch"
        ));
        let state = disk_state(directory.path());
        assert_eq!(
            state
                .audit
                .iter()
                .find(|record| record.kind == "repeat_offender_banned")
                .map(|record| record.outcome.as_str()),
            Some(r"c:\browser\chrome.exe")
        );
        assert!(
            state
                .repeat_offenders
                .iter()
                .any(|record| record.executable == r"c:\browser\chrome.exe")
        );
    }

    #[test]
    fn disabled_protection_suspends_repeat_offender_enforcement() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(&core, repeat_offender_policy());
        let base = 10_000_i64;
        establish_repeat_offender_ban(&core, base);

        let session = match success(core.handle(
            request(
                "auth",
                "auth-nonce",
                ServiceRequest::Authenticate {
                    password: "long-test-password".into(),
                },
            ),
            base + 100,
        )) {
            ServiceResult::Session { session_token, .. } => session_token,
            _ => panic!("unexpected response"),
        };
        let mut disabled = repeat_offender_policy();
        disabled["protectionEnabled"] = json!(false);
        success(core.handle(
            request(
                "disable",
                "disable-nonce",
                ServiceRequest::PutPolicy {
                    super_password: None,
                    session_token: session,
                    expected_revision: 1,
                    policy: disabled,
                },
            ),
            base + 101,
        ));

        let now = base + 30 * 60 * 1000;
        let relaunch = context_observation_at(
            "relaunch",
            now - 1,
            "项目进度评审会议",
            Some("blocked.example"),
            200,
        );
        assert!(matches!(
            submit_context_at(&core, "relaunch-nonce", relaunch, now),
            ServiceResult::Acknowledged
        ));
    }

    fn authenticate_ui(core: &ServiceCore, nonce: &str, now_ms: i64) -> String {
        match success(core.handle(
            request(
                &format!("auth-{nonce}"),
                nonce,
                ServiceRequest::Authenticate {
                    password: "long-test-password".into(),
                },
            ),
            now_ms,
        )) {
            ServiceResult::Session { session_token, .. } => session_token,
            _ => panic!("unexpected response"),
        }
    }

    fn agent_enforcement_at(core: &ServiceCore, nonce: &str, now_ms: i64) -> AgentEnforcement {
        match success(core.handle(
            RequestEnvelope::new(
                format!("request-{nonce}"),
                nonce,
                ClientKind::Agent,
                ServiceRequest::GetAgentPolicy {
                    agent_token: "agent-secret".into(),
                },
            ),
            now_ms,
        )) {
            ServiceResult::AgentPolicy { enforcement, .. } => enforcement,
            _ => panic!("unexpected response"),
        }
    }

    fn list_enforcement(
        core: &ServiceCore,
        session: &str,
        nonce: &str,
        now_ms: i64,
    ) -> EnforcementSummary {
        match success(core.handle(
            request(
                &format!("list-{nonce}"),
                nonce,
                ServiceRequest::ListEnforcement {
                    session_token: session.into(),
                    title_offset: 0,
                    title_limit: 50,
                    title_query: None,
                    misjudged: false,
                },
            ),
            now_ms,
        )) {
            ServiceResult::Enforcement(summary) => summary,
            _ => panic!("unexpected response"),
        }
    }

    const PORN_TITLE: &str = "Hot Video 123 - Google Chrome";

    fn remembered_title_policy() -> Value {
        json!({
            "protectionEnabled": true,
            "recognition": {"imageEnabled": true, "sensitivity": 60, "titleMatchingEnabled": true},
            "websites": [
                {"id":"allowed","pattern":"safe.example","action":"allow","enabled":true}
            ]
        })
    }

    /// Submits a high-risk image observation for `process_id` showing
    /// `title` and reports the resulting closure.
    fn close_by_image(
        core: &ServiceCore,
        nonce: &str,
        title: Option<&str>,
        process_id: u32,
        now_ms: i64,
    ) {
        let observation = AgentObservation {
            event_id: format!("image-{nonce}"),
            agent_instance_id: "agent-1".into(),
            occurred_at_ms: now_ms - 1,
            monitor_id: "monitor-1".into(),
            risk_millis: 990,
            reason_code: "image_immediate".into(),
            source: Some(ProcessIdentity {
                process_id,
                started_at_ms: 77,
                executable_name: r"C:\Browser\chrome.exe".into(),
                executable_sha256: None,
            }),
            browser_host: Some("video.example".into()),
            evidence_pending: false,
            window_title: title.map(str::to_owned),
        };
        assert!(matches!(
            success(core.handle(
                RequestEnvelope::new(
                    format!("request-{nonce}"),
                    nonce,
                    ClientKind::Agent,
                    ServiceRequest::AgentObservation {
                        agent_token: "agent-secret".into(),
                        observation,
                    },
                ),
                now_ms,
            )),
            ServiceResult::DispositionRequired { .. }
        ));
        core.record_disposition(
            &DispositionReport {
                event_id: format!("image-{nonce}"),
                process_id,
                started_at_ms: 77,
                outcome: DispositionOutcome::Terminated,
            },
            now_ms + 1,
        )
        .unwrap();
    }

    #[test]
    fn image_closed_window_title_is_remembered_and_closed_on_sight() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(&core, remembered_title_policy());
        close_by_image(&core, "image-1", Some(PORN_TITLE), 42, 1_000);

        // The remembered title reaches the agent only as a digest.
        let enforcement = agent_enforcement_at(&core, "agent-policy-1", 1_100);
        assert_eq!(
            enforcement.remembered_title_digests,
            vec![title_digest(r"c:\browser\chrome.exe", PORN_TITLE).unwrap()]
        );
        assert_eq!(disk_state(directory.path()).remembered_titles.len(), 1);

        // The same title in a relaunched browser closes it without any image hit.
        let relaunch =
            context_observation_at("relaunch", 2_000, "hot video 123 - GOOGLE chrome", None, 43);
        assert!(matches!(
            submit_context_at(&core, "relaunch-nonce", relaunch, 2_001),
            ServiceResult::DispositionRequired { event_id, .. } if event_id == "relaunch"
        ));
        // Other pages of the same browser stay open.
        let other = context_observation_at("other", 2_100, "Homework - Google Chrome", None, 44);
        assert!(matches!(
            submit_context_at(&core, "other-nonce", other, 2_101),
            ServiceResult::Acknowledged
        ));
        // An allowlisted host suppresses the remembered title like any exemption.
        let allowed =
            context_observation_at("allowed", 2_200, PORN_TITLE, Some("www.safe.example"), 45);
        assert!(matches!(
            submit_context_at(&core, "allowed-nonce", allowed, 2_201),
            ServiceResult::Acknowledged
        ));
        let state = disk_state(directory.path());
        assert!(
            state
                .audit
                .iter()
                .any(|record| record.kind == "window_title_remembered")
        );
        // Titles never enter the audit log.
        assert!(
            state
                .audit
                .iter()
                .all(|record| !record.outcome.contains("Hot Video"))
        );
    }

    #[test]
    fn remembered_title_survives_restart_and_can_be_forgotten_by_the_administrator() {
        let directory = tempfile::tempdir().unwrap();
        {
            let core = open_core(directory.path(), 0);
            configure_policy(&core, remembered_title_policy());
            close_by_image(&core, "image-1", Some(PORN_TITLE), 42, 1_000);
        }
        let core = open_core(directory.path(), 5_000);
        let session = authenticate_ui(&core, "auth-forget", 5_000);
        let summary = list_enforcement(&core, &session, "list-1", 5_001);
        assert_eq!(summary.remembered_titles.len(), 1);
        let remembered = &summary.remembered_titles[0];
        assert_eq!(remembered.title, PORN_TITLE);
        assert_eq!(remembered.browser_host.as_deref(), Some("video.example"));
        assert_eq!(remembered.remembered_at_ms, 1_001);

        let relaunch = context_observation_at("relaunch", 5_100, PORN_TITLE, None, 43);
        assert!(matches!(
            submit_context_at(&core, "relaunch-nonce", relaunch, 5_101),
            ServiceResult::DispositionRequired { .. }
        ));

        match success(core.handle(
            request(
                "forget",
                "forget-nonce",
                ServiceRequest::ForgetRememberedTitle {
                    super_password: None,
                    session_token: session.clone(),
                    title_id: remembered.id.clone(),
                },
            ),
            5_200,
        )) {
            ServiceResult::Acknowledged => {}
            _ => panic!("unexpected response"),
        }
        assert!(
            list_enforcement(&core, &session, "list-2", 5_250)
                .remembered_titles
                .is_empty()
        );
        assert!(disk_state(directory.path()).remembered_titles.is_empty());
        let again = context_observation_at("again", 5_300, PORN_TITLE, None, 46);
        assert!(matches!(
            submit_context_at(&core, "again-nonce", again, 5_301),
            ServiceResult::Acknowledged
        ));
        assert_eq!(
            core.handle(
                request(
                    "forget-missing",
                    "forget-missing-nonce",
                    ServiceRequest::ForgetRememberedTitle {
                        super_password: None,
                        session_token: session,
                        title_id: remembered.id.clone(),
                    },
                ),
                5_400,
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::InvalidRequest
        );
    }

    #[test]
    fn titles_are_not_remembered_when_disabled_generic_or_not_closed() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        let mut policy = remembered_title_policy();
        policy["recognition"]["rememberTitlesEnabled"] = json!(false);
        configure_policy(&core, policy);
        close_by_image(&core, "disabled", Some(PORN_TITLE), 42, 1_000);
        assert!(
            agent_enforcement_at(&core, "agent-policy-1", 1_100)
                .remembered_title_digests
                .is_empty()
        );

        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(&core, remembered_title_policy());
        // The bare application name and a missing title are too generic.
        close_by_image(&core, "generic", Some("Chrome"), 42, 1_000);
        close_by_image(&core, "untitled", None, 43, 1_100);
        // A failed closure remembers nothing.
        let observation = AgentObservation {
            event_id: "denied".into(),
            agent_instance_id: "agent-1".into(),
            occurred_at_ms: 1_200,
            monitor_id: "monitor-1".into(),
            risk_millis: 990,
            reason_code: "image_immediate".into(),
            source: Some(context_observation("ignored", "x", None).source),
            browser_host: None,
            evidence_pending: false,
            window_title: Some(PORN_TITLE.into()),
        };
        success(core.handle(
            RequestEnvelope::new(
                "denied-request",
                "denied-nonce",
                ClientKind::Agent,
                ServiceRequest::AgentObservation {
                    agent_token: "agent-secret".into(),
                    observation,
                },
            ),
            1_201,
        ));
        core.record_disposition(
            &DispositionReport {
                event_id: "denied".into(),
                process_id: 42,
                started_at_ms: 77,
                outcome: DispositionOutcome::AccessDenied,
            },
            1_202,
        )
        .unwrap();
        assert!(
            agent_enforcement_at(&core, "agent-policy-2", 1_300)
                .remembered_title_digests
                .is_empty()
        );
    }

    #[test]
    fn configured_strike_limit_and_window_govern_bans_and_reach_the_agent() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        let mut policy = remembered_title_policy();
        policy["repeatOffender"] = json!({"windowMinutes": 1, "strikeLimit": 2});
        configure_policy(&core, policy);
        // Two closures more than a minute apart do not ban.
        close_by_image(&core, "strike-1", None, 50, 10_000);
        close_by_image(&core, "strike-2", None, 51, 10_000 + 61_000);
        assert!(
            agent_enforcement_at(&core, "agent-policy-1", 71_100)
                .banned_executables
                .is_empty()
        );
        // A second closure inside the minute bans the browser for one hour.
        close_by_image(&core, "strike-3", None, 52, 100_000);
        assert_eq!(
            agent_enforcement_at(&core, "agent-policy-2", 100_100).banned_executables,
            vec![r"c:\browser\chrome.exe".to_owned()]
        );
        let session = authenticate_ui(&core, "auth-bans", 100_200);
        assert_eq!(
            list_enforcement(&core, &session, "list-bans", 100_201).active_bans,
            vec![ActiveBan {
                executable: r"c:\browser\chrome.exe".into(),
                banned_until_ms: 100_001 + 60 * 60 * 1000,
            }]
        );
        assert!(
            agent_enforcement_at(&core, "agent-policy-3", 100_001 + 60 * 60 * 1000)
                .banned_executables
                .is_empty()
        );
    }

    #[test]
    fn disabled_protection_publishes_no_agent_enforcement() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(&core, repeat_offender_policy());
        establish_repeat_offender_ban(&core, 10_000);
        assert_eq!(
            agent_enforcement_at(&core, "agent-policy-on", 20_000).banned_executables,
            vec![r"c:\browser\chrome.exe".to_owned()]
        );
        let session = authenticate_ui(&core, "auth-off", 20_100);
        let mut disabled = repeat_offender_policy();
        disabled["protectionEnabled"] = json!(false);
        success(core.handle(
            request(
                "disable",
                "disable-nonce",
                ServiceRequest::PutPolicy {
                    super_password: None,
                    session_token: session,
                    expected_revision: 1,
                    policy: disabled,
                },
            ),
            20_101,
        ));
        assert_eq!(
            agent_enforcement_at(&core, "agent-policy-off", 20_200),
            AgentEnforcement::default()
        );
    }

    #[test]
    fn put_policy_rejects_out_of_range_repeat_offender_settings() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(&core, json!({}));
        let session = authenticate_ui(&core, "auth-invalid", 10);
        assert_eq!(
            core.handle(
                request(
                    "invalid-repeat",
                    "invalid-repeat-nonce",
                    ServiceRequest::PutPolicy {
                        super_password: None,
                        session_token: session,
                        expected_revision: 1,
                        policy: json!({"repeatOffender": {"windowMinutes": 0, "strikeLimit": 5}}),
                    },
                ),
                11,
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::InvalidRequest
        );
    }

    fn list_titles(
        core: &ServiceCore,
        session: &str,
        nonce: &str,
        offset: u32,
        limit: u32,
        query: Option<&str>,
    ) -> EnforcementSummary {
        match success(core.handle(
            request(
                &format!("list-{nonce}"),
                nonce,
                ServiceRequest::ListEnforcement {
                    session_token: session.into(),
                    title_offset: offset,
                    title_limit: limit,
                    title_query: query.map(str::to_owned),
                    misjudged: false,
                },
            ),
            90_000,
        )) {
            ServiceResult::Enforcement(summary) => summary,
            _ => panic!("unexpected response"),
        }
    }

    fn list_title_kind(
        core: &ServiceCore,
        session: &str,
        nonce: &str,
        misjudged: bool,
        now_ms: i64,
    ) -> EnforcementSummary {
        match success(core.handle(
            request(
                nonce,
                nonce,
                ServiceRequest::ListEnforcement {
                    session_token: session.into(),
                    title_offset: 0,
                    title_limit: 50,
                    title_query: None,
                    misjudged,
                },
            ),
            now_ms,
        )) {
            ServiceResult::Enforcement(summary) => summary,
            _ => panic!("unexpected response"),
        }
    }

    #[test]
    fn misjudged_titles_record_their_reason_and_are_never_remembered_again() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(&core, remembered_title_policy());
        close_by_image(
            &core,
            "image-1",
            Some("Homework - Google Chrome"),
            42,
            1_000,
        );
        let session = authenticate_ui(&core, "auth-misjudged", 1_100);
        set_super(&core, &session, 1_101);

        let listed = list_title_kind(&core, &session, "list-1", false, 1_102);
        let title = &listed.remembered_titles[0];
        assert_eq!(
            title.reason,
            Some(TitleReasonMetadata {
                reason_code: "image_immediate".into(),
                risk_millis: 990,
                threshold_millis: recognition_threshold_millis(&remembered_title_policy()),
            })
        );
        let mark = |nonce: &str, super_password: Option<&str>, now_ms| {
            core.handle(
                request(
                    nonce,
                    nonce,
                    ServiceRequest::MarkTitleMisjudged {
                        session_token: session.clone(),
                        title_id: title.id.clone(),
                        super_password: super_password.map(str::to_owned),
                    },
                ),
                now_ms,
            )
            .result
            .err()
            .map(|failure| failure.code)
        };
        assert_eq!(
            mark("mark-no-super", None, 1_103),
            Some(ServiceErrorCode::SuperPasswordRequired)
        );
        assert_eq!(mark("mark", Some("distinct-super-password"), 1_104), None);

        let after = list_title_kind(&core, &session, "list-2", false, 1_105);
        assert_eq!(
            (after.remembered_title_total, after.misjudged_title_total),
            (0, 1)
        );
        assert!(
            agent_enforcement_at(&core, "agent-misjudged", 1_106)
                .remembered_title_digests
                .is_empty()
        );
        // Another misjudged frame still closes the window but is not remembered.
        close_by_image(
            &core,
            "image-2",
            Some("homework - GOOGLE chrome"),
            43,
            2_000,
        );
        let misjudged = list_title_kind(&core, &session, "list-3", true, 2_100);
        assert_eq!(
            (
                misjudged.remembered_title_total,
                misjudged.remembered_titles[0].reason.clone()
            ),
            (1, None)
        );
        assert_eq!(
            list_title_kind(&core, &session, "list-4", false, 2_101).remembered_title_total,
            0
        );

        // Restoring needs only the session and lets the title be remembered again.
        success(core.handle(
            request(
                "restore",
                "restore",
                ServiceRequest::RestoreMisjudgedTitle {
                    session_token: session.clone(),
                    title_id: misjudged.remembered_titles[0].id.clone(),
                },
            ),
            2_200,
        ));
        close_by_image(
            &core,
            "image-3",
            Some("Homework - Google Chrome"),
            44,
            3_000,
        );
        let restored = list_title_kind(&core, &session, "list-5", false, 3_100);
        assert_eq!(
            (
                restored.remembered_title_total,
                restored.misjudged_title_total
            ),
            (1, 0)
        );
        let state = disk_state(directory.path());
        assert!(state.misjudged_titles.is_empty());
        let kinds: Vec<_> = state
            .audit
            .iter()
            .filter(|event| {
                event.kind == "remembered_title_misjudged"
                    || event.kind == "misjudged_title_restored"
            })
            .map(|event| (event.kind.as_str(), event.outcome.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("remembered_title_misjudged", "denied"),
                ("remembered_title_misjudged", "success"),
                ("misjudged_title_restored", "success")
            ]
        );
    }

    #[test]
    fn remembered_titles_are_paged_newest_first_and_searchable() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(&core, remembered_title_policy());
        for index in 0..5_u32 {
            let title = format!("Clip {index} - Google Chrome");
            close_by_image(
                &core,
                &format!("page-{index}"),
                Some(&title),
                60 + index,
                10_000 + i64::from(index) * 1_000,
            );
        }
        let session = authenticate_ui(&core, "auth-pages", 80_000);
        let first = list_titles(&core, &session, "page-a", 0, 2, None);
        assert_eq!(first.remembered_title_total, 5);
        assert_eq!(
            first
                .remembered_titles
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Clip 4 - Google Chrome", "Clip 3 - Google Chrome"]
        );
        let last = list_titles(&core, &session, "page-b", 4, 2, None);
        assert_eq!(last.remembered_titles.len(), 1);
        assert_eq!(last.remembered_titles[0].title, "Clip 0 - Google Chrome");
        let found = list_titles(&core, &session, "page-c", 0, 50, Some("CLIP 2"));
        assert_eq!(found.remembered_title_total, 1);
        assert_eq!(found.remembered_titles[0].title, "Clip 2 - Google Chrome");
        let by_host = list_titles(&core, &session, "page-d", 0, 50, Some("video.EXAMPLE"));
        assert_eq!(by_host.remembered_title_total, 5);
        assert_eq!(
            list_titles(&core, &session, "page-e", 0, 50, Some("nothing")).remembered_title_total,
            0
        );
    }

    #[test]
    fn database_never_holds_remembered_titles_in_plaintext() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(&core, remembered_title_policy());
        close_by_image(&core, "sealed", Some(PORN_TITLE), 42, 1_000);
        drop(core);
        let bytes = fs::read(directory.path().join("service.db")).unwrap();
        for secret in [PORN_TITLE.as_bytes(), b"video.example"] {
            assert!(!bytes.windows(secret.len()).any(|window| window == secret));
        }
        let reopened = open_core(directory.path(), 2_000);
        let relaunch = context_observation_at("relaunch", 2_100, PORN_TITLE, None, 43);
        assert!(matches!(
            submit_context_at(&reopened, "relaunch-nonce", relaunch, 2_101),
            ServiceResult::DispositionRequired { .. }
        ));
    }

    #[test]
    fn legacy_json_state_is_migrated_into_the_database() {
        let directory = tempfile::tempdir().unwrap();
        let salt = SaltString::generate(&mut PasswordOsRng);
        let hash = Argon2::default()
            .hash_password(b"long-test-password", &salt)
            .unwrap()
            .to_string();
        fs::write(
            directory.path().join("service.json"),
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "password_hash": hash,
                "policy_revision": 3,
                "policy": remembered_title_policy(),
                "audit": [{"occurred_at_ms": 1, "kind": "policy_updated", "outcome": "success"}],
                "evidence": [],
                "repeat_offenders": [{
                    "executable": "d:\\games\\game.exe",
                    "closed_at_ms": [],
                    "banned_until_ms": 60_000,
                    "last_process_id": 9,
                    "last_started_at_ms": 1
                }],
                "network_restore": null,
                "remembered_titles": [{
                    "id": "title-legacy",
                    "executable": "c:\\browser\\chrome.exe",
                    "title": PORN_TITLE,
                    "browser_host": null,
                    "remembered_at_ms": 5
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        let core = open_core(directory.path(), 10_000);
        let enforcement = agent_enforcement_at(&core, "migrated-policy", 10_000);
        assert_eq!(
            enforcement.banned_executables,
            vec![r"d:\games\game.exe".to_owned()]
        );
        assert_eq!(
            enforcement.remembered_title_digests,
            vec![title_digest(r"c:\browser\chrome.exe", PORN_TITLE).unwrap()]
        );
        let session = authenticate_ui(&core, "migrated-auth", 10_001);
        match success(core.handle(
            request(
                "migrated-status",
                "migrated-status-nonce",
                ServiceRequest::GetStatus {
                    session_token: session,
                },
            ),
            10_002,
        )) {
            ServiceResult::Status(status) => {
                assert_eq!(status.policy_revision, 3);
                assert_eq!(status.audit_count, 2);
            }
            _ => panic!("unexpected response"),
        }
        assert!(!directory.path().join("service.json").exists());
        assert!(directory.path().join("service.json.migrated").exists());
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
            window_title: None,
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
                        super_password: None,
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
        let state = disk_state(directory.path());
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
                        super_password: None,
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
    fn allowed_application_is_exempt_from_image_and_title_enforcement() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(
            &core,
            json!({
                "protectionEnabled": true,
                "recognition": {"imageEnabled": true, "sensitivity": 60, "titleMatchingEnabled": true},
                "applications": [
                    {"id":"zcode","name":"ZCode","executable":"ZCode.exe","category":"custom","action":"allow","enabled":true}
                ]
            }),
        );
        let image = AgentObservation {
            event_id: "image-allowed".into(),
            agent_instance_id: "agent-1".into(),
            occurred_at_ms: 100,
            monitor_id: "monitor-1".into(),
            risk_millis: 990,
            reason_code: "image_immediate".into(),
            source: Some(ProcessIdentity {
                process_id: 42,
                started_at_ms: 77,
                executable_name: r"C:\Users\wei\AppData\Local\Programs\ZCode.exe".into(),
                executable_sha256: None,
            }),
            browser_host: None,
            evidence_pending: false,
            window_title: None,
        };
        assert!(matches!(
            success(core.handle(
                RequestEnvelope::new(
                    "image-allowed-request",
                    "image-allowed-nonce",
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
        let mut context = context_observation("title-allowed", "最新色情影片", None);
        context.source.executable_name = r"C:\Tools\ZCode.exe".into();
        assert!(matches!(
            submit_context(&core, "title-allowed-nonce", context),
            ServiceResult::Acknowledged
        ));
    }

    #[test]
    fn blocklisted_host_still_closes_an_allowed_application() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(
            &core,
            json!({
                "protectionEnabled": true,
                "recognition": {"titleMatchingEnabled": true},
                "websites": [
                    {"id":"blocked","pattern":"blocked.example","action":"block","enabled":true}
                ],
                "applications": [
                    {"id":"browser","name":"浏览器","executable":"chrome.exe","category":"browser","action":"allow","enabled":true}
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
    }

    #[test]
    fn blocked_application_is_closed_whenever_observed() {
        let directory = tempfile::tempdir().unwrap();
        let core = open_core(directory.path(), 0);
        configure_policy(
            &core,
            json!({
                "protectionEnabled": true,
                "applications": [
                    {"id":"game","name":"游戏","executable":"game.exe","category":"game","action":"block","enabled":true}
                ]
            }),
        );
        let mut context = context_observation("blocked-app", "日常工作会议纪要", None);
        context.source.executable_name = r"D:\Games\Game.exe".into();
        assert!(matches!(
            submit_context(&core, "blocked-app-nonce", context),
            ServiceResult::DispositionRequired { event_id, .. } if event_id == "blocked-app"
        ));
    }

    #[test]
    fn put_policy_rejects_invalid_application_rules() {
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
                        super_password: None,
                        session_token: session,
                        expected_revision: 0,
                        policy: json!({
                            "applications": [
                                {"id":"bad","name":"X","executable":"x.exe","category":"custom","action":"terminate","enabled":true}
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
        let state = disk_state(directory.path());
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
            window_title: None,
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
                    super_password: None,
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
            window_title: None,
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
                        super_password: None,
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

    #[test]
    fn evidence_batch_delete_skips_missing_items_and_tolerates_lost_originals() {
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
        success(core.handle(
            request(
                "policy",
                "policy-nonce",
                ServiceRequest::PutPolicy {
                    super_password: None,
                    session_token: session.clone(),
                    expected_revision: 0,
                    policy: json!({
                        "protectionEnabled": true,
                        "recognition": {"evidenceEnabled": true, "immediateThreshold": 95}
                    }),
                },
            ),
            1,
        ));
        let image = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3];
        for id in ["evidence-1", "evidence-2", "evidence-3"] {
            let submit = RequestEnvelope::new(
                format!("submit-{id}"),
                format!("nonce-{id}"),
                ClientKind::Agent,
                ServiceRequest::SubmitEvidence {
                    agent_token: "agent-secret".into(),
                    evidence: EvidenceSubmission {
                        evidence_id: id.into(),
                        captured_at_ms: 2,
                        monitor_name: "显示器 1".into(),
                        application_name: "browser.exe".into(),
                        reason_code: "image_immediate".into(),
                        risk_millis: 980,
                        media_type: "image/png".into(),
                        bytes_base64: BASE64.encode(image),
                    },
                },
            );
            success(core.handle(submit, 2));
        }
        fs::remove_file(directory.path().join("evidence/evidence-2.kme")).unwrap();
        let delete = |id: &str, ids: &[&str]| {
            core.handle(
                request(
                    id,
                    &format!("nonce-{id}"),
                    ServiceRequest::DeleteEvidenceBatch {
                        session_token: session.clone(),
                        evidence_ids: ids.iter().map(|id| (*id).to_owned()).collect(),
                    },
                ),
                3,
            )
        };
        assert!(matches!(
            success(delete("d1", &["evidence-1", "evidence-2", "evidence-gone"])),
            ServiceResult::Acknowledged
        ));
        assert!(!directory.path().join("evidence/evidence-1.kme").exists());
        assert_eq!(
            delete("d2", &["evidence-1"]).result.unwrap_err().code,
            ServiceErrorCode::EvidenceUnavailable
        );
        assert!(matches!(
            success(core.handle(
                request("list", "list-nonce", ServiceRequest::ListEvidence { session_token: session.clone() }),
                4,
            )),
            ServiceResult::EvidenceList { items } if items.len() == 1 && items[0].id == "evidence-3"
        ));
        // Deletions survive a restart.
        drop(core);
        let reopened = open_core(directory.path(), 0);
        let session = match success(reopened.handle(
            request(
                "auth",
                "auth-nonce",
                ServiceRequest::Authenticate {
                    password: "long-test-password".into(),
                },
            ),
            5,
        )) {
            ServiceResult::Session { session_token, .. } => session_token,
            _ => panic!("unexpected response"),
        };
        assert!(matches!(
            success(reopened.handle(
                request("list2", "list2-nonce", ServiceRequest::ListEvidence { session_token: session }),
                6,
            )),
            ServiceResult::EvidenceList { items } if items.len() == 1
        ));
    }

    const UTC_PLUS_EIGHT: LocalOffsetFn = |_| 8 * 3600;
    /// 1970-01-05 00:00 in UTC+8 was a Monday; shifted forward by whole weeks.
    const MONDAY_MIDNIGHT_UTC8_MS: i64 = 4 * 86_400_000 - 8 * 3_600_000 + 2_900 * 604_800_000;

    fn local_ms(day: i64, hour: i64, minute: i64) -> i64 {
        MONDAY_MIDNIGHT_UTC8_MS + ((day * 24 + hour) * 60 + minute) * 60_000
    }

    fn network_policy() -> Value {
        json!({
            "protectionEnabled": true,
            "schedules": [{
                "id": "homework",
                "name": "作业时间",
                "target": "browsers",
                "blocks": [
                    {"day": 0, "start": "19:00", "end": "21:00"},
                    {"day": 0, "start": "22:00", "end": "07:00"}
                ],
                "enabled": true
            }]
        })
    }

    fn network_core(directory: &Path, policy: Value, now_ms: i64) -> (ServiceCore, String) {
        let core = open_core(directory, now_ms).with_local_offset(UTC_PLUS_EIGHT);
        let session = match success(core.handle(
            request(
                "net-enroll",
                "net-enroll-nonce",
                ServiceRequest::EnrollAdministrator {
                    password: "long-test-password".into(),
                },
            ),
            now_ms,
        )) {
            ServiceResult::Session { session_token, .. } => session_token,
            _ => panic!("unexpected response"),
        };
        success(core.handle(
            request(
                "net-policy",
                "net-policy-nonce",
                ServiceRequest::PutPolicy {
                    super_password: None,
                    session_token: session.clone(),
                    expected_revision: 0,
                    policy,
                },
            ),
            now_ms,
        ));
        (core, session)
    }

    fn restore(
        core: &ServiceCore,
        session: &str,
        password: &str,
        nonce: &str,
        now_ms: i64,
    ) -> ResponseEnvelope {
        core.handle(
            request(
                &format!("restore-{nonce}"),
                nonce,
                ServiceRequest::RestoreNetwork {
                    session_token: session.into(),
                    password: password.into(),
                },
            ),
            now_ms,
        )
    }

    #[test]
    fn local_minute_of_week_starts_on_monday() {
        assert_eq!(local_minute_of_week(local_ms(0, 0, 0), 8 * 3600), 0);
        assert_eq!(local_minute_of_week(local_ms(6, 23, 59), 8 * 3600), 10_079);
        assert_eq!(local_minute_of_week(local_ms(7, 0, 1), 8 * 3600), 1);
    }

    #[test]
    fn network_is_restricted_only_inside_schedule_blocks() {
        let directory = tempfile::tempdir().unwrap();
        let start = local_ms(0, 18, 0);
        let (core, _) = network_core(directory.path(), network_policy(), start);

        let outside = core.network_enforcement(start).unwrap();
        assert!(!outside.status.restricted);
        assert!(!outside.restriction.is_restricted());

        let inside = core.network_enforcement(local_ms(0, 19, 30)).unwrap();
        assert!(inside.status.restricted);
        assert_eq!(inside.status.active_rule_ids, vec!["homework"]);
        assert_eq!(inside.status.restricted_until_ms, Some(local_ms(0, 21, 0)));
        assert!(inside.restriction.block_browsers);
        assert!(
            inside
                .restriction
                .blocks_executable(r"C:\Program Files\Mozilla Firefox\firefox.exe")
        );
        assert!(
            !inside
                .restriction
                .blocks_executable(r"C:\Windows\System32\svchost.exe")
        );

        assert!(
            !core
                .network_enforcement(local_ms(0, 21, 30))
                .unwrap()
                .status
                .restricted
        );
        let overnight = core.network_enforcement(local_ms(1, 6, 59)).unwrap();
        assert!(overnight.status.restricted);
        assert_eq!(
            overnight.status.restricted_until_ms,
            Some(local_ms(1, 7, 0))
        );
    }

    #[test]
    fn restoring_network_requires_the_administrator_password() {
        let directory = tempfile::tempdir().unwrap();
        let start = local_ms(0, 19, 0);
        let (core, session) = network_core(directory.path(), network_policy(), start);
        let now = local_ms(0, 19, 10);
        assert!(core.network_enforcement(now).unwrap().status.restricted);

        let denied = restore(&core, &session, "wrong-password", "restore-denied", now);
        assert_eq!(
            denied.result.unwrap_err().code,
            ServiceErrorCode::AuthenticationFailed
        );
        assert!(core.network_enforcement(now).unwrap().status.restricted);

        let invalid_session = restore(
            &core,
            "unknown-session",
            "long-test-password",
            "restore-session",
            now,
        );
        assert_eq!(
            invalid_session.result.unwrap_err().code,
            ServiceErrorCode::AuthenticationRequired
        );

        match success(restore(
            &core,
            &session,
            "long-test-password",
            "restore-ok",
            now,
        )) {
            ServiceResult::NetworkStatus(status) => {
                assert!(!status.restricted);
                assert_eq!(status.restored_until_ms, Some(local_ms(0, 21, 0)));
            }
            _ => panic!("unexpected response"),
        }
        let lifted = core.network_enforcement(local_ms(0, 20, 0)).unwrap();
        assert!(!lifted.status.restricted);
        assert!(!lifted.restriction.is_restricted());

        // The restore ends with the block it lifted; the next block applies.
        assert!(
            core.network_enforcement(local_ms(0, 22, 30))
                .unwrap()
                .status
                .restricted
        );
    }

    #[test]
    fn network_restore_survives_restart_and_is_audited() {
        let directory = tempfile::tempdir().unwrap();
        let start = local_ms(0, 19, 0);
        {
            let (core, session) = network_core(directory.path(), network_policy(), start);
            core.network_enforcement(start).unwrap();
            success(restore(
                &core,
                &session,
                "long-test-password",
                "restore-ok",
                start,
            ));
        }
        let reopened = open_core(directory.path(), start).with_local_offset(UTC_PLUS_EIGHT);
        assert!(
            !reopened
                .network_enforcement(local_ms(0, 20, 0))
                .unwrap()
                .status
                .restricted
        );
        let state = disk_state(directory.path());
        let kinds: Vec<&str> = state
            .audit
            .iter()
            .map(|record| record.kind.as_str())
            .collect();
        assert!(kinds.contains(&"network_schedule_started"));
        assert!(kinds.contains(&"network_restored_by_administrator"));
    }

    #[test]
    fn restore_outside_a_block_changes_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let start = local_ms(0, 12, 0);
        let (core, session) = network_core(directory.path(), network_policy(), start);
        match success(restore(
            &core,
            &session,
            "long-test-password",
            "restore-idle",
            start,
        )) {
            ServiceResult::NetworkStatus(status) => assert_eq!(status, NetworkStatus::default()),
            _ => panic!("unexpected response"),
        }
        assert!(
            core.network_enforcement(local_ms(0, 19, 30))
                .unwrap()
                .status
                .restricted
        );
    }

    #[test]
    fn disabled_protection_lifts_network_schedules() {
        let directory = tempfile::tempdir().unwrap();
        let mut policy = network_policy();
        policy["protectionEnabled"] = json!(false);
        let start = local_ms(0, 19, 30);
        let (core, _) = network_core(directory.path(), policy, start);
        assert!(!core.network_enforcement(start).unwrap().status.restricted);
    }

    #[test]
    fn invalid_network_schedules_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let start = local_ms(0, 12, 0);
        let (core, session) = network_core(directory.path(), json!({}), start);
        let response = core.handle(
            request(
                "bad-schedule",
                "bad-schedule-nonce",
                ServiceRequest::PutPolicy {
                    super_password: None,
                    session_token: session,
                    expected_revision: 1,
                    policy: json!({"schedules": [{
                        "id": "x", "name": "x", "target": "applications",
                        "applications": [], "blocks": [], "enabled": true
                    }]}),
                },
            ),
            start,
        );
        assert_eq!(
            response.result.unwrap_err().code,
            ServiceErrorCode::InvalidRequest
        );
    }

    #[test]
    fn status_reports_the_network_schedule() {
        let directory = tempfile::tempdir().unwrap();
        let now = local_ms(0, 20, 0);
        let (core, session) = network_core(directory.path(), network_policy(), now);
        match success(core.handle(
            request(
                "status",
                "status-nonce",
                ServiceRequest::GetStatus {
                    session_token: session,
                },
            ),
            now,
        )) {
            ServiceResult::Status(status) => {
                assert!(status.network.restricted);
                assert_eq!(status.network.restricted_until_ms, Some(local_ms(0, 21, 0)));
            }
            _ => panic!("unexpected response"),
        }
    }
    fn set_super(core: &ServiceCore, session: &str, now_ms: i64) {
        success(core.handle(
            request(
                "set-super",
                "set-super",
                ServiceRequest::SetSuperPassword {
                    session_token: session.into(),
                    admin_password: "long-test-password".into(),
                    super_password: "distinct-super-password".into(),
                },
            ),
            now_ms,
        ));
    }

    fn status_at(core: &ServiceCore, session: &str, nonce: &str, now_ms: i64) -> ServiceStatus {
        match success(core.handle(
            request(
                nonce,
                nonce,
                ServiceRequest::GetStatus {
                    session_token: session.into(),
                },
            ),
            now_ms,
        )) {
            ServiceResult::Status(status) => status,
            _ => panic!("expected service status"),
        }
    }

    fn installer(
        core: &ServiceCore,
        body: ServiceRequest,
        nonce: &str,
        now_ms: i64,
    ) -> ResponseEnvelope {
        core.handle(
            RequestEnvelope::new(nonce, nonce, ClientKind::Installer, body),
            now_ms,
        )
    }

    fn shutdown(core: &ServiceCore, password: &str, nonce: &str, now_ms: i64) -> ResponseEnvelope {
        installer(
            core,
            ServiceRequest::RequestShutdown {
                password: password.into(),
            },
            nonce,
            now_ms,
        )
    }

    #[test]
    fn super_password_setup_requires_session_admin_and_distinct_long_secret() {
        let directory = tempfile::tempdir().unwrap();
        let (core, session) = network_core(directory.path(), json!({}), 0);
        for (nonce, token, admin, secret, expected) in [
            (
                "bad-session",
                "unknown",
                "long-test-password",
                "distinct-super-password",
                ServiceErrorCode::AuthenticationRequired,
            ),
            (
                "bad-admin",
                session.as_str(),
                "wrong-password",
                "distinct-super-password",
                ServiceErrorCode::AuthenticationFailed,
            ),
            (
                "short-super",
                session.as_str(),
                "long-test-password",
                "short",
                ServiceErrorCode::InvalidRequest,
            ),
            (
                "equal-super",
                session.as_str(),
                "long-test-password",
                "long-test-password",
                ServiceErrorCode::InvalidRequest,
            ),
        ] {
            let response = core.handle(
                request(
                    nonce,
                    nonce,
                    ServiceRequest::SetSuperPassword {
                        session_token: token.into(),
                        admin_password: admin.into(),
                        super_password: secret.into(),
                    },
                ),
                1,
            );
            assert_eq!(response.result.unwrap_err().code, expected, "{nonce}");
        }
        assert!(!status_at(&core, &session, "before-super", 2).super_password_set);
        set_super(&core, &session, 3);
        let status = status_at(&core, &session, "after-super", 4);
        assert!(status.super_password_set);
        assert_eq!(status.product_version, karma_ipc::PRODUCT_VERSION);
        let reset = core.handle(
            request(
                "reset",
                "reset",
                ServiceRequest::SetSuperPassword {
                    session_token: session,
                    admin_password: "long-test-password".into(),
                    super_password: "replacement-super".into(),
                },
            ),
            5,
        );
        assert_eq!(
            reset.result.unwrap_err().code,
            ServiceErrorCode::AlreadyEnrolled
        );
        drop(core);
        let reopened = open_core(directory.path(), 6);
        success(shutdown(
            &reopened,
            "distinct-super-password",
            "persisted-super",
            7,
        ));
        let events = disk_state(directory.path()).audit;
        assert!(
            events
                .iter()
                .any(|event| event.kind == "super_password_set" && event.outcome == "denied")
        );
        assert!(!format!("{events:?}").contains("distinct-super-password"));
    }

    #[test]
    fn changing_either_password_preserves_distinct_secrets_and_requires_current_secret() {
        let directory = tempfile::tempdir().unwrap();
        let (core, session) = network_core(directory.path(), json!({}), 0);
        set_super(&core, &session, 1);
        let admin_equal = core.handle(
            request(
                "admin-equal",
                "admin-equal",
                ServiceRequest::ChangePassword {
                    session_token: session.clone(),
                    current_password: "long-test-password".into(),
                    new_password: "distinct-super-password".into(),
                },
            ),
            2,
        );
        assert_eq!(
            admin_equal.result.unwrap_err().code,
            ServiceErrorCode::InvalidRequest
        );
        for (nonce, current, new, expected) in [
            (
                "super-wrong",
                "long-test-password",
                "replacement-super",
                ServiceErrorCode::AuthenticationFailed,
            ),
            (
                "super-short",
                "distinct-super-password",
                "short",
                ServiceErrorCode::InvalidRequest,
            ),
            (
                "super-equal",
                "distinct-super-password",
                "long-test-password",
                ServiceErrorCode::InvalidRequest,
            ),
        ] {
            let result = core.handle(
                request(
                    nonce,
                    nonce,
                    ServiceRequest::ChangeSuperPassword {
                        session_token: session.clone(),
                        current_super_password: current.into(),
                        new_super_password: new.into(),
                    },
                ),
                3,
            );
            assert_eq!(result.result.unwrap_err().code, expected, "{nonce}");
        }
        success(core.handle(
            request(
                "super-change",
                "super-change",
                ServiceRequest::ChangeSuperPassword {
                    session_token: session,
                    current_super_password: "distinct-super-password".into(),
                    new_super_password: "replacement-super".into(),
                },
            ),
            4,
        ));
        drop(core);
        let reopened = open_core(directory.path(), 5);
        assert_eq!(
            shutdown(&reopened, "distinct-super-password", "old-super", 6)
                .result
                .unwrap_err()
                .code,
            ServiceErrorCode::AuthenticationFailed
        );
        success(shutdown(&reopened, "replacement-super", "new-super", 7));
    }

    fn guarded_policy() -> Value {
        json!({
            "protectionEnabled": true,
            "recognition": {"imageEnabled": true, "ocrEnabled": true, "titleMatchingEnabled": true, "rememberTitlesEnabled": true, "sensitivity": 80},
            "repeatOffender": {"windowMinutes": 20, "strikeLimit": 5},
            "websites": [{"id":"b", "pattern":"blocked.example", "action":"block", "enabled":true}],
            "applications": [{"id":"g", "name":"Game", "executable":"game.exe", "category":"game", "action":"block", "enabled":true}],
            "keywords": [{"id":"k", "phrase":"赌球直播", "category":"high_risk", "enabled":true}],
            "schedules": [{"id":"night", "name":"night", "target":"browsers", "blocks":[{"day":0,"start":"21:00","end":"23:00"}], "enabled":true}]
        })
    }

    #[test]
    fn service_rejects_every_policy_weakening_without_super_authorization() {
        let directory = tempfile::tempdir().unwrap();
        let original = guarded_policy();
        let (core, session) = network_core(directory.path(), original.clone(), 0);
        set_super(&core, &session, 1);
        let mut weakened = Vec::new();
        for pointer in [
            "/protectionEnabled",
            "/recognition/imageEnabled",
            "/recognition/ocrEnabled",
            "/recognition/titleMatchingEnabled",
            "/recognition/rememberTitlesEnabled",
        ] {
            let mut policy = original.clone();
            *policy.pointer_mut(pointer).unwrap() = json!(false);
            weakened.push(policy);
        }
        for (pointer, value) in [
            ("/recognition/sensitivity", json!(90)),
            ("/websites", json!([])),
            ("/applications", json!([])),
            ("/keywords", json!([])),
            ("/schedules", json!([])),
            ("/repeatOffender/windowMinutes", json!(10)),
            ("/repeatOffender/strikeLimit", json!(6)),
        ] {
            let mut policy = original.clone();
            *policy.pointer_mut(pointer).unwrap() = value;
            weakened.push(policy);
        }
        for (index, policy) in weakened.into_iter().enumerate() {
            let nonce = format!("weaken-{index}");
            let response = core.handle(
                request(
                    &nonce,
                    &nonce,
                    ServiceRequest::PutPolicy {
                        session_token: session.clone(),
                        expected_revision: 1,
                        policy,
                        super_password: None,
                    },
                ),
                2,
            );
            assert_eq!(
                response.result.unwrap_err().code,
                ServiceErrorCode::SuperPasswordRequired,
                "{index}"
            );
        }
        assert_eq!(
            status_at(&core, &session, "unchanged-revision", 3).policy_revision,
            1
        );
        let mut disabled = original.clone();
        disabled["protectionEnabled"] = json!(false);
        let wrong = core.handle(
            request(
                "wrong-weakening",
                "wrong-weakening",
                ServiceRequest::PutPolicy {
                    session_token: session.clone(),
                    expected_revision: 1,
                    policy: disabled.clone(),
                    super_password: Some("long-test-password".into()),
                },
            ),
            4,
        );
        assert_eq!(
            wrong.result.unwrap_err().code,
            ServiceErrorCode::AuthenticationFailed
        );
        let mut tightened = original;
        tightened["recognition"]["sensitivity"] = json!(70);
        tightened["updates"] = json!({"autoInstall":false});
        success(core.handle(
            request(
                "tighten",
                "tighten",
                ServiceRequest::PutPolicy {
                    session_token: session.clone(),
                    expected_revision: 1,
                    policy: tightened,
                    super_password: None,
                },
            ),
            5,
        ));
        assert!(!core.auto_install_updates().unwrap());
        success(core.handle(
            request(
                "authorized-weaken",
                "authorized-weaken",
                ServiceRequest::PutPolicy {
                    session_token: session,
                    expected_revision: 2,
                    policy: disabled,
                    super_password: Some("distinct-super-password".into()),
                },
            ),
            6,
        ));
        assert_eq!(
            disk_state(directory.path())
                .audit
                .iter()
                .filter(|event| event.kind == "policy_weakening" && event.outcome == "denied")
                .count(),
            13
        );
    }

    #[test]
    fn remembered_title_and_network_restore_require_super_once_configured() {
        let directory = tempfile::tempdir().unwrap();
        let now = local_ms(0, 19, 0);
        let (core, session) = network_core(directory.path(), network_policy(), now);
        set_super(&core, &session, now + 1);
        {
            let mut runtime = core.runtime.lock().unwrap();
            runtime.remember(RememberedTitle {
                id: "remembered".into(),
                executable: "chrome.exe".into(),
                title: "A remembered title".into(),
                browser_host: None,
                remembered_at_ms: now,
                reason: None,
            });
            persist(&mut runtime).unwrap();
        }
        let forget = |secret: Option<&str>, nonce: &str| {
            core.handle(
                request(
                    nonce,
                    nonce,
                    ServiceRequest::ForgetRememberedTitle {
                        session_token: session.clone(),
                        title_id: "remembered".into(),
                        super_password: secret.map(str::to_owned),
                    },
                ),
                now + 2,
            )
        };
        assert_eq!(
            forget(None, "forget-missing").result.unwrap_err().code,
            ServiceErrorCode::SuperPasswordRequired
        );
        assert_eq!(
            forget(Some("long-test-password"), "forget-admin")
                .result
                .unwrap_err()
                .code,
            ServiceErrorCode::AuthenticationFailed
        );
        assert_eq!(disk_state(directory.path()).remembered_titles.len(), 1);
        success(forget(Some("distinct-super-password"), "forget-super"));
        assert!(disk_state(directory.path()).remembered_titles.is_empty());
        assert_eq!(
            restore(
                &core,
                &session,
                "long-test-password",
                "restore-admin-with-super",
                now + 3
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::AuthenticationFailed
        );
        assert!(core.network_enforcement(now + 3).unwrap().status.restricted);
        success(restore(
            &core,
            &session,
            "distinct-super-password",
            "restore-super",
            now + 4,
        ));
        assert!(!core.network_enforcement(now + 4).unwrap().status.restricted);
    }

    #[test]
    fn delayed_uninstall_survives_restart_and_has_exact_wait_and_expiry_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let (core, session) = network_core(directory.path(), json!({}), 0);
        set_super(&core, &session, 1);
        assert_eq!(
            shutdown(&core, "long-test-password", "admin-no-request", 2)
                .result
                .unwrap_err()
                .code,
            ServiceErrorCode::AuthenticationFailed
        );
        assert_eq!(
            core.handle(
                request(
                    "request-wrong",
                    "request-wrong",
                    ServiceRequest::RequestUninstall {
                        session_token: session.clone(),
                        password: "distinct-super-password".into(),
                    }
                ),
                3
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::AuthenticationFailed
        );
        success(core.handle(
            request(
                "request-uninstall",
                "request-uninstall",
                ServiceRequest::RequestUninstall {
                    session_token: session.clone(),
                    password: "long-test-password".into(),
                },
            ),
            100,
        ));
        let requested = status_at(&core, &session, "requested-status", 101)
            .uninstall
            .unwrap();
        assert_eq!(requested.requested_at_ms, 100);
        assert_eq!(requested.effective_at_ms, 100 + UNINSTALL_WAIT_MS);
        assert_eq!(
            requested.expires_at_ms,
            100 + UNINSTALL_WAIT_MS + UNINSTALL_VALIDITY_MS
        );
        drop(core);
        let core = open_core(directory.path(), 102);
        assert_eq!(
            shutdown(
                &core,
                "long-test-password",
                "too-soon",
                requested.effective_at_ms - 1
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::AuthenticationFailed
        );
        success(shutdown(
            &core,
            "long-test-password",
            "effective-boundary",
            requested.effective_at_ms,
        ));
        drop(core);
        let core = open_core(directory.path(), requested.effective_at_ms + 1);
        success(shutdown(
            &core,
            "long-test-password",
            "before-expiry",
            requested.expires_at_ms - 1,
        ));
        assert_eq!(
            shutdown(
                &core,
                "long-test-password",
                "expired-boundary",
                requested.expires_at_ms
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::AuthenticationFailed
        );
        success(shutdown(
            &core,
            "distinct-super-password",
            "super-after-expiry",
            requested.expires_at_ms + 1,
        ));
        let session = authenticate_ui(&core, "post-expiry-auth", requested.expires_at_ms + 2);
        assert!(
            status_at(
                &core,
                &session,
                "expired-status",
                requested.expires_at_ms + 3
            )
            .uninstall
            .is_none()
        );
        drop(core);
        let (_, stored) = Store::open(&directory.path().join("service.db"), &[7; 32]).unwrap();
        assert!(stored.uninstall_request.is_none());
        let audit = disk_state(directory.path()).audit;
        assert_eq!(
            audit
                .iter()
                .filter(|event| event.kind == "uninstall_request_ready")
                .count(),
            1
        );
        assert_eq!(
            audit
                .iter()
                .filter(|event| event.kind == "uninstall_request_expired")
                .count(),
            1
        );
        assert!(
            audit
                .iter()
                .any(|event| event.kind == "uninstall_authentication" && event.outcome == "denied")
        );
    }

    #[test]
    fn administrator_can_cancel_uninstall_any_time_without_super_password() {
        let directory = tempfile::tempdir().unwrap();
        let (core, initial_session) = network_core(directory.path(), json!({}), 0);
        set_super(&core, &initial_session, 1);
        for (index, cancel_after) in [
            1,
            UNINSTALL_WAIT_MS,
            UNINSTALL_WAIT_MS + UNINSTALL_VALIDITY_MS,
        ]
        .into_iter()
        .enumerate()
        {
            let request_time = index as i64 * (UNINSTALL_WAIT_MS + UNINSTALL_VALIDITY_MS + 10);
            let session = authenticate_ui(&core, &format!("cancel-auth-{index}"), request_time);
            let nonce = format!("cancel-request-{index}");
            success(core.handle(
                request(
                    &nonce,
                    &nonce,
                    ServiceRequest::RequestUninstall {
                        session_token: session,
                        password: "long-test-password".into(),
                    },
                ),
                request_time,
            ));
            let cancel_time = request_time + cancel_after;
            let session = authenticate_ui(&core, &format!("cancel-session-{index}"), cancel_time);
            let denied_nonce = format!("cancel-denied-{index}");
            assert_eq!(
                core.handle(
                    request(
                        &denied_nonce,
                        &denied_nonce,
                        ServiceRequest::CancelUninstall {
                            session_token: "unknown".into()
                        }
                    ),
                    cancel_time
                )
                .result
                .unwrap_err()
                .code,
                ServiceErrorCode::AuthenticationRequired
            );
            let cancel_nonce = format!("cancel-{index}");
            success(core.handle(
                request(
                    &cancel_nonce,
                    &cancel_nonce,
                    ServiceRequest::CancelUninstall {
                        session_token: session.clone(),
                    },
                ),
                cancel_time,
            ));
            assert!(
                status_at(
                    &core,
                    &session,
                    &format!("cancel-status-{index}"),
                    cancel_time
                )
                .uninstall
                .is_none()
            );
            assert_eq!(
                shutdown(
                    &core,
                    "long-test-password",
                    &format!("cancel-shutdown-{index}"),
                    cancel_time
                )
                .result
                .unwrap_err()
                .code,
                ServiceErrorCode::AuthenticationFailed
            );
        }
    }

    #[test]
    fn no_super_password_keeps_immediate_administrator_uninstall() {
        let directory = tempfile::tempdir().unwrap();
        let (core, _) = network_core(directory.path(), json!({}), 0);
        success(shutdown(&core, "long-test-password", "legacy-uninstall", 1));
        assert!(core.auto_install_updates().unwrap());
    }

    #[test]
    fn update_actions_require_authentication_and_have_a_bounded_host_queue() {
        let directory = tempfile::tempdir().unwrap();
        let (core, session) = network_core(directory.path(), json!({}), 0);
        set_super(&core, &session, 1);
        let denied = core.handle(
            request(
                "update-no-session",
                "update-no-session",
                ServiceRequest::CheckForUpdates {
                    session_token: "unknown".into(),
                },
            ),
            2,
        );
        assert_eq!(
            denied.result.unwrap_err().code,
            ServiceErrorCode::AuthenticationRequired
        );
        assert_eq!(
            installer(
                &core,
                ServiceRequest::ApplyStagedUpdate {
                    password: "distinct-super-password".into()
                },
                "update-super",
                3
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::AuthenticationFailed
        );
        assert!(core.take_update_action().unwrap().is_none());
        success(core.handle(
            request(
                "check-one",
                "check-one",
                ServiceRequest::CheckForUpdates {
                    session_token: session.clone(),
                },
            ),
            4,
        ));
        assert_eq!(
            core.handle(
                request(
                    "check-two",
                    "check-two",
                    ServiceRequest::CheckForUpdates {
                        session_token: session.clone(),
                    }
                ),
                4
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::ServiceUnavailable
        );
        assert_eq!(
            status_at(&core, &session, "update-checking", 5)
                .update
                .phase,
            UpdatePhase::Checking
        );
        assert_eq!(
            core.take_update_action().unwrap(),
            Some(UpdateAction::CheckAndInstall)
        );
        assert_eq!(core.take_update_action().unwrap(), None);
        core.record_update_status(
            UpdateStatus {
                phase: UpdatePhase::UpToDate,
                ..UpdateStatus::default()
            },
            5,
        )
        .unwrap();
        success(installer(
            &core,
            ServiceRequest::ApplyStagedUpdate {
                password: "long-test-password".into(),
            },
            "stage-admin",
            6,
        ));
        assert_eq!(
            core.take_update_action().unwrap(),
            Some(UpdateAction::ApplyStaged)
        );
        let updated = UpdateStatus {
            phase: UpdatePhase::Failed,
            last_checked_at_ms: Some(7),
            available_version: None,
            last_error: Some("signature_invalid".into()),
        };
        core.record_update_status(updated.clone(), 7).unwrap();
        core.record_update_status(updated.clone(), 8).unwrap();
        assert_eq!(
            status_at(&core, &session, "update-failed", 9).update,
            updated
        );
        assert_eq!(
            disk_state(directory.path())
                .audit
                .iter()
                .filter(|event| event.kind == "signed_update" && event.outcome == "failed")
                .count(),
            1
        );
        assert_eq!(
            disk_state(directory.path())
                .audit
                .iter()
                .filter(|event| event.kind == "signed_update_requested" && event.outcome != "busy")
                .count(),
            2
        );
    }

    #[test]
    fn valid_admin_authentication_cannot_reset_super_password_rate_limits() {
        let directory = tempfile::tempdir().unwrap();
        let (core, session) = network_core(directory.path(), json!({}), 0);
        set_super(&core, &session, 1);
        for index in 0..MAX_FAILURES {
            let session = authenticate_ui(&core, &format!("alternating-admin-{index}"), 2);
            let nonce = format!("alternating-super-{index}");
            let denied = core.handle(
                request(
                    &nonce,
                    &nonce,
                    ServiceRequest::ChangeSuperPassword {
                        session_token: session,
                        current_super_password: "wrong-secret".into(),
                        new_super_password: "replacement-super".into(),
                    },
                ),
                2,
            );
            assert_eq!(
                denied.result.unwrap_err().code,
                ServiceErrorCode::AuthenticationFailed
            );
        }
        let session = authenticate_ui(&core, "admin-during-super-cooldown", 3);
        assert_eq!(
            shutdown(&core, "distinct-super-password", "limited-super", 3)
                .result
                .unwrap_err()
                .code,
            ServiceErrorCode::RateLimited
        );
        success(core.handle(
            request(
                "check-during-super-cooldown",
                "check-during-super-cooldown",
                ServiceRequest::CheckForUpdates {
                    session_token: session,
                },
            ),
            3,
        ));
        assert_eq!(
            core.take_update_action().unwrap(),
            Some(UpdateAction::CheckAndInstall)
        );
        core.record_update_status(
            UpdateStatus {
                phase: UpdatePhase::UpToDate,
                ..UpdateStatus::default()
            },
            3,
        )
        .unwrap();
        success(installer(
            &core,
            ServiceRequest::ApplyStagedUpdate {
                password: "long-test-password".into(),
            },
            "stage-during-super-cooldown",
            3,
        ));
        assert_eq!(
            core.take_update_action().unwrap(),
            Some(UpdateAction::ApplyStaged)
        );
        core.record_update_status(
            UpdateStatus {
                phase: UpdatePhase::UpToDate,
                ..UpdateStatus::default()
            },
            3,
        )
        .unwrap();
        success(shutdown(
            &core,
            "distinct-super-password",
            "cooldown-super",
            2 + FAILURE_COOLDOWN_MS,
        ));
    }

    #[test]
    fn administrator_cooldown_does_not_block_valid_super_authentication() {
        let directory = tempfile::tempdir().unwrap();
        let (core, session) = network_core(directory.path(), json!({}), 0);
        set_super(&core, &session, 1);
        for index in 0..MAX_FAILURES {
            let nonce = format!("wrong-admin-{index}");
            let denied = core.handle(
                request(
                    &nonce,
                    &nonce,
                    ServiceRequest::Authenticate {
                        password: "wrong-admin-password".into(),
                    },
                ),
                2,
            );
            assert_eq!(
                denied.result.unwrap_err().code,
                ServiceErrorCode::AuthenticationFailed
            );
        }
        success(shutdown(
            &core,
            "distinct-super-password",
            "unlimited-super",
            3,
        ));
        let denied = core.handle(
            request(
                "limited-admin",
                "limited-admin",
                ServiceRequest::Authenticate {
                    password: "long-test-password".into(),
                },
            ),
            3,
        );
        assert_eq!(
            denied.result.unwrap_err().code,
            ServiceErrorCode::RateLimited
        );
        authenticate_ui(&core, "admin-after-cooldown", 2 + FAILURE_COOLDOWN_MS);
    }
    #[test]
    fn uninstall_and_update_handoffs_are_mutually_exclusive_under_the_core_lock() {
        let directory = tempfile::tempdir().unwrap();
        let (core, session) = network_core(directory.path(), json!({}), 0);
        set_super(&core, &session, 1);
        success(core.handle(
            request(
                "queued-update",
                "queued-update",
                ServiceRequest::CheckForUpdates {
                    session_token: session.clone(),
                },
            ),
            2,
        ));
        assert_eq!(
            shutdown(
                &core,
                "distinct-super-password",
                "uninstall-while-queued",
                3
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::ServiceUnavailable
        );
        assert_eq!(
            core.take_update_action().unwrap(),
            Some(UpdateAction::CheckAndInstall)
        );
        for (index, phase) in [
            UpdatePhase::Checking,
            UpdatePhase::Downloading,
            UpdatePhase::Installing,
        ]
        .into_iter()
        .enumerate()
        {
            core.record_update_status(
                UpdateStatus {
                    phase,
                    ..UpdateStatus::default()
                },
                4,
            )
            .unwrap();
            assert_eq!(
                shutdown(
                    &core,
                    "distinct-super-password",
                    &format!("uninstall-while-busy-{index}"),
                    5
                )
                .result
                .unwrap_err()
                .code,
                ServiceErrorCode::ServiceUnavailable
            );
            assert_eq!(
                installer(
                    &core,
                    ServiceRequest::ApplyStagedUpdate {
                        password: "long-test-password".into()
                    },
                    &format!("update-while-busy-{index}"),
                    5
                )
                .result
                .unwrap_err()
                .code,
                ServiceErrorCode::ServiceUnavailable
            );
            assert!(core.take_update_action().unwrap().is_none());
        }
        core.record_update_status(
            UpdateStatus {
                phase: UpdatePhase::Failed,
                ..UpdateStatus::default()
            },
            6,
        )
        .unwrap();
        success(shutdown(
            &core,
            "distinct-super-password",
            "authorized-uninstall",
            7,
        ));
        for phase in [
            UpdatePhase::Checking,
            UpdatePhase::Downloading,
            UpdatePhase::Installing,
        ] {
            assert!(matches!(
                core.record_update_status(
                    UpdateStatus {
                        phase,
                        ..UpdateStatus::default()
                    },
                    8
                ),
                Err(CoreError::ShutdownAuthorized)
            ));
        }
        assert_eq!(
            core.handle(
                request(
                    "update-after-uninstall",
                    "update-after-uninstall",
                    ServiceRequest::CheckForUpdates {
                        session_token: session
                    }
                ),
                9
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::ServiceUnavailable
        );
        assert_eq!(
            installer(
                &core,
                ServiceRequest::ApplyStagedUpdate {
                    password: "long-test-password".into()
                },
                "staged-after-uninstall",
                9
            )
            .result
            .unwrap_err()
            .code,
            ServiceErrorCode::ServiceUnavailable
        );
        assert!(core.take_update_action().unwrap().is_none());
    }
}
