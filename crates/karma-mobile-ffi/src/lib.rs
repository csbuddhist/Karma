//! UniFFI export layer of the Karma core for mobile platforms.
//!
//! The platform captures frames, runs the image classifier, and calls the
//! Android APIs. This crate fuses risk across frames, applies the policy,
//! checks the administrator password, and keeps the audit trail. Only scores
//! and metadata cross the boundary — never pixels or recognized text — and
//! every call carries its own timestamp, so decisions are reproducible.

mod auth;
mod engine;
mod packages;
mod store;
mod types;
mod update;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::Value;
use zeroize::Zeroizing;

pub use engine::{CompiledPolicy, Engine, Evaluation, IMAGE_MODEL_VERSION, MAX_FOCUS_REGIONS};
pub use packages::{
    EXEMPT_PACKAGES, KARMA_PACKAGE, KNOWN_BROWSER_PACKAGES, KNOWN_VIDEO_PACKAGES,
    with_builtin_applications,
};
pub use types::{
    AdminSession, AuditEventRecord, AuthError, ClassifierProbabilities, CoreError, CoreStatus,
    Enrollment, ForegroundContext, MobileAction, MobileDecision, MobileReason, PackageClass,
};

pub use update::{AndroidRelease, ReleaseSource};

use crate::auth::Sessions;
use crate::store::{PersistedState, SealedStore};

uniffi::setup_scaffolding!();

const MAX_POLICY_BYTES: usize = 512 * 1024;
const MAX_AUDIT_QUERY: u32 = 500;
const MAX_AUDIT_TOKEN_CHARS: usize = 64;

struct Inner {
    store: SealedStore,
    state: PersistedState,
    engine: Engine,
    sessions: Sessions,
}

impl Inner {
    fn audit(
        &mut self,
        now_ms: i64,
        kind: &str,
        outcome: &str,
        package_name: Option<String>,
        decision: Option<&MobileDecision>,
    ) {
        self.state.push_audit(AuditEventRecord {
            occurred_at_ms: now_ms,
            kind: kind.into(),
            outcome: outcome.into(),
            package_name,
            action: decision.map(|value| value.action),
            reason: decision.map(|value| value.reason),
        });
    }

    fn persist(&mut self) -> Result<(), CoreError> {
        self.state.cooldowns = self.engine.cooldowns().clone();
        self.store.save(&self.state)
    }

    /// Audits and persists a new closure. Storage failures never weaken the
    /// decision already returned to the platform.
    fn finish(&mut self, evaluation: Evaluation, package: &str, now_ms: i64) -> MobileDecision {
        if evaluation.new_closure {
            self.audit(
                now_ms,
                "decision_applied",
                "applied",
                packages::normalize_package(package),
                Some(&evaluation.decision),
            );
            let _ = self.persist();
        }
        evaluation.decision
    }

    fn authenticate_password(&mut self, password: &str, now_ms: i64) -> Result<(), AuthError> {
        if let Err(error) = auth::check_limit(&self.state.auth_limit, now_ms) {
            self.audit(
                now_ms,
                "administrator_authentication",
                "rate_limited",
                None,
                None,
            );
            let _ = self.persist();
            return Err(error);
        }
        let Some(hash) = self.state.password_hash.clone() else {
            return Err(AuthError::NotEnrolled);
        };
        let valid = auth::secret_matches(&hash, password)?;
        self.finish_attempt(valid, "administrator_authentication", now_ms)
    }

    fn finish_attempt(&mut self, valid: bool, kind: &str, now_ms: i64) -> Result<(), AuthError> {
        auth::record_attempt(&mut self.state.auth_limit, valid, now_ms);
        self.audit(
            now_ms,
            kind,
            if valid { "success" } else { "denied" },
            None,
            None,
        );
        self.persist().map_err(|_| AuthError::Storage)?;
        if valid {
            Ok(())
        } else {
            Err(AuthError::Failed)
        }
    }

    fn new_recovery_codes(&mut self) -> Result<Vec<String>, AuthError> {
        let mut codes = Vec::with_capacity(auth::RECOVERY_CODE_COUNT);
        let mut hashes = Vec::with_capacity(auth::RECOVERY_CODE_COUNT);
        for _ in 0..auth::RECOVERY_CODE_COUNT {
            let code = auth::generate_recovery_code();
            hashes.push(auth::hash_secret(&auth::normalize_recovery_code(&code))?);
            codes.push(code.to_string());
        }
        self.state.recovery_code_hashes = hashes;
        Ok(codes)
    }
}

/// The Karma decision core for one device user.
#[derive(uniffi::Object)]
pub struct KarmaCore {
    inner: Mutex<Inner>,
}

impl KarmaCore {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panic mid-call leaves the state consistent enough to keep
        // enforcing; failing open would be worse.
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[uniffi::export]
impl KarmaCore {
    /// Opens (or creates) the sealed state in `data_dir` with the 32-byte
    /// `state_key`. A stored policy that no longer compiles falls back to the
    /// defaults instead of disabling protection.
    #[uniffi::constructor]
    pub fn open(data_dir: String, state_key: Vec<u8>) -> Result<Arc<Self>, CoreError> {
        let state_key = Zeroizing::new(state_key);
        let store = SealedStore::new(&PathBuf::from(data_dir), &state_key)?;
        let state = store.load()?;
        let policy = CompiledPolicy::compile(&state.policy)
            .or_else(|_| CompiledPolicy::compile(&Value::Null))?;
        let engine = Engine::new(policy, state.cooldowns.clone());
        Ok(Arc::new(Self {
            inner: Mutex::new(Inner {
                store,
                state,
                engine,
                sessions: Sessions::default(),
            }),
        }))
    }

    pub fn status(&self) -> CoreStatus {
        let inner = self.lock();
        CoreStatus {
            enrolled: inner.state.password_hash.is_some(),
            protection_enabled: inner.engine.policy().protection_enabled(),
            policy_revision: inner.state.policy_revision,
            recovery_codes_remaining: inner.state.recovery_code_hashes.len() as u32,
        }
    }

    pub fn classify_package(&self, package_name: String) -> PackageClass {
        self.lock().engine.classify_package(&package_name)
    }

    /// One classified frame plus its window context → decision.
    pub fn observe_frame(
        &self,
        probabilities: ClassifierProbabilities,
        context: ForegroundContext,
    ) -> Result<MobileDecision, CoreError> {
        let mut inner = self.lock();
        let evaluation = inner.engine.observe_frame(&probabilities, &context)?;
        Ok(inner.finish(evaluation, &context.package_name, context.captured_at_ms))
    }

    /// One classified frame, the classified crops of its moving regions (at
    /// most [`MAX_FOCUS_REGIONS`], such as a video in a small player), and its
    /// window context → decision. The riskiest result counts.
    pub fn observe_focused_frame(
        &self,
        probabilities: ClassifierProbabilities,
        focus: Vec<ClassifierProbabilities>,
        context: ForegroundContext,
    ) -> Result<MobileDecision, CoreError> {
        let mut inner = self.lock();
        let evaluation = inner
            .engine
            .observe_focused_frame(&probabilities, &focus, &context)?;
        Ok(inner.finish(evaluation, &context.package_name, context.captured_at_ms))
    }

    /// Window context alone → decision, for windows that were not captured.
    pub fn observe_context(&self, context: ForegroundContext) -> MobileDecision {
        let mut inner = self.lock();
        let evaluation = inner.engine.observe_context(&context);
        inner.finish(evaluation, &context.package_name, context.captured_at_ms)
    }

    /// Sets the first administrator password and returns one-time recovery codes.
    pub fn enroll_admin(&self, password: String, now_ms: i64) -> Result<Enrollment, AuthError> {
        let password = Zeroizing::new(password);
        let mut inner = self.lock();
        if inner.state.password_hash.is_some() {
            return Err(AuthError::AlreadyEnrolled);
        }
        auth::validate_new_password(&password)?;
        inner.state.password_hash = Some(auth::hash_secret(&password)?);
        let recovery_codes = inner.new_recovery_codes()?;
        inner.audit(now_ms, "administrator_enrolled", "success", None, None);
        inner.persist().map_err(|_| AuthError::Storage)?;
        let session = inner.sessions.issue(now_ms);
        Ok(Enrollment {
            session,
            recovery_codes,
        })
    }

    pub fn verify_admin_password(
        &self,
        password: String,
        now_ms: i64,
    ) -> Result<AdminSession, AuthError> {
        let password = Zeroizing::new(password);
        let mut inner = self.lock();
        inner.authenticate_password(&password, now_ms)?;
        Ok(inner.sessions.issue(now_ms))
    }

    /// Consumes one recovery code and opens a session, so the administrator
    /// can set a new password.
    pub fn redeem_recovery_code(
        &self,
        code: String,
        now_ms: i64,
    ) -> Result<AdminSession, AuthError> {
        let code = auth::normalize_recovery_code(&code);
        let mut inner = self.lock();
        if inner.state.password_hash.is_none() {
            return Err(AuthError::NotEnrolled);
        }
        if let Err(error) = auth::check_limit(&inner.state.auth_limit, now_ms) {
            inner.audit(now_ms, "recovery_code_redeemed", "rate_limited", None, None);
            let _ = inner.persist();
            return Err(error);
        }
        let mut matched = None;
        for (index, hash) in inner.state.recovery_code_hashes.iter().enumerate() {
            if auth::secret_matches(hash, &code)? {
                matched = Some(index);
                break;
            }
        }
        if let Some(index) = matched {
            inner.state.recovery_code_hashes.remove(index);
        }
        inner.finish_attempt(matched.is_some(), "recovery_code_redeemed", now_ms)?;
        Ok(inner.sessions.issue(now_ms))
    }

    /// Replaces the password. Other sessions end; the caller's stays valid.
    pub fn change_admin_password(
        &self,
        session_token: String,
        new_password: String,
        now_ms: i64,
    ) -> Result<(), AuthError> {
        let new_password = Zeroizing::new(new_password);
        let mut inner = self.lock();
        inner.sessions.authorize(&session_token, now_ms)?;
        auth::validate_new_password(&new_password)?;
        inner.state.password_hash = Some(auth::hash_secret(&new_password)?);
        inner.audit(
            now_ms,
            "administrator_password_changed",
            "success",
            None,
            None,
        );
        inner.persist().map_err(|_| AuthError::Storage)?;
        inner.sessions.revoke_others(&session_token);
        Ok(())
    }

    /// Replaces all recovery codes and returns the new ones.
    pub fn regenerate_recovery_codes(
        &self,
        session_token: String,
        now_ms: i64,
    ) -> Result<Vec<String>, AuthError> {
        let mut inner = self.lock();
        inner.sessions.authorize(&session_token, now_ms)?;
        let codes = inner.new_recovery_codes()?;
        inner.audit(now_ms, "recovery_codes_regenerated", "success", None, None);
        inner.persist().map_err(|_| AuthError::Storage)?;
        Ok(codes)
    }

    /// Checks a session before a protection-weakening platform operation,
    /// such as stopping monitoring or removing Device Owner, and audits it.
    pub fn authorize_privileged(
        &self,
        session_token: String,
        operation: String,
        now_ms: i64,
    ) -> Result<(), AuthError> {
        let mut inner = self.lock();
        let operation = audit_token(&operation).unwrap_or_else(|| "unknown".into());
        let result = inner.sessions.authorize(&session_token, now_ms);
        let outcome = if result.is_ok() { "success" } else { "denied" };
        inner.audit(
            now_ms,
            &format!("privileged:{operation}"),
            outcome,
            None,
            None,
        );
        inner.persist().map_err(|_| AuthError::Storage)?;
        result
    }

    pub fn end_session(&self, session_token: String) {
        self.lock().sessions.revoke(&session_token);
    }

    /// The stored policy document as JSON (administrator rules only; the
    /// built-in Android browser and player rules are added when compiled).
    pub fn get_policy(&self) -> String {
        self.lock().state.policy.to_string()
    }

    /// Validates and stores a policy document; returns the new revision.
    pub fn set_policy(
        &self,
        session_token: String,
        policy_json: String,
        now_ms: i64,
    ) -> Result<u64, CoreError> {
        if policy_json.len() > MAX_POLICY_BYTES {
            return Err(CoreError::InvalidPolicy);
        }
        let mut inner = self.lock();
        inner
            .sessions
            .authorize(&session_token, now_ms)
            .map_err(|_| CoreError::Unauthorized)?;
        let policy: Value =
            serde_json::from_str(&policy_json).map_err(|_| CoreError::InvalidPolicy)?;
        if !policy.is_object() {
            return Err(CoreError::InvalidPolicy);
        }
        let compiled = CompiledPolicy::compile(&policy)?;
        inner.engine.set_policy(compiled);
        inner.state.policy = policy;
        inner.state.policy_revision = inner.state.policy_revision.saturating_add(1);
        inner.audit(now_ms, "policy_changed", "success", None, None);
        inner.persist()?;
        Ok(inner.state.policy_revision)
    }

    /// Ends the cooldown of `package_name` early (administrator override).
    pub fn clear_cooldown(
        &self,
        session_token: String,
        package_name: String,
        now_ms: i64,
    ) -> Result<bool, CoreError> {
        let mut inner = self.lock();
        inner
            .sessions
            .authorize(&session_token, now_ms)
            .map_err(|_| CoreError::Unauthorized)?;
        let cleared = inner.engine.clear_cooldown(&package_name);
        if cleared {
            inner.audit(
                now_ms,
                "cooldown_cleared",
                "success",
                packages::normalize_package(&package_name),
                None,
            );
            inner.persist()?;
        }
        Ok(cleared)
    }

    /// Records a platform event (component health, enforcement outcome,
    /// settings watchdog). Free-form fields are reduced to short tokens.
    pub fn record_audit(&self, event: AuditEventRecord) -> Result<(), CoreError> {
        let kind = audit_token(&event.kind).ok_or(CoreError::InvalidRequest)?;
        let outcome = audit_token(&event.outcome).ok_or(CoreError::InvalidRequest)?;
        let mut inner = self.lock();
        inner.state.push_audit(AuditEventRecord {
            occurred_at_ms: event.occurred_at_ms,
            kind: format!("platform:{kind}"),
            outcome,
            package_name: event
                .package_name
                .as_deref()
                .and_then(packages::normalize_package),
            action: event.action,
            reason: event.reason,
        });
        inner.persist()
    }

    /// Up to `limit` events at or after `since_ms`, newest first.
    pub fn query_audit(&self, since_ms: i64, limit: u32) -> Vec<AuditEventRecord> {
        let inner = self.lock();
        inner
            .state
            .audit
            .iter()
            .rev()
            .filter(|event| event.occurred_at_ms >= since_ms)
            .take(limit.min(MAX_AUDIT_QUERY) as usize)
            .cloned()
            .collect()
    }
}

/// A lowercase `[a-z0-9_]` token of at most 64 characters, or `None`.
fn audit_token(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    let valid = !value.is_empty()
        && value.len() <= MAX_AUDIT_TOKEN_CHARS
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    valid.then_some(value)
}

/// The release-list API URL of `source`.
#[uniffi::export]
pub fn release_list_url(source: ReleaseSource) -> String {
    source.releases_url().to_owned()
}

/// Every update source, in download order.
#[uniffi::export]
pub fn release_sources() -> Vec<ReleaseSource> {
    ReleaseSource::ALL.to_vec()
}

/// The newest Android release in `releases_json` (a GitHub or Gitee release
/// list) that is newer than `installed_version`, or `None`.
#[uniffi::export]
pub fn select_android_release(
    source: ReleaseSource,
    releases_json: Vec<u8>,
    installed_version: String,
) -> Option<AndroidRelease> {
    update::select_release(source, &releases_json, &installed_version)
}

/// Newest version first, then GitHub before its Gitee mirror.
#[uniffi::export]
pub fn order_android_releases(mut releases: Vec<AndroidRelease>) -> Vec<AndroidRelease> {
    update::order_releases(&mut releases);
    releases
}

/// The SHA-256 (lowercase hex) that a checksum file lists for `file_name`.
#[uniffi::export]
pub fn checksum_for_file(checksums: String, file_name: String) -> Option<String> {
    update::checksum_for(&checksums, &file_name)
}

/// Size limits the platform enforces while downloading.
#[uniffi::export]
pub fn update_download_limits() -> UpdateLimits {
    UpdateLimits {
        release_list_bytes: update::MAX_RELEASE_LIST_BYTES,
        apk_bytes: update::MAX_APK_BYTES,
        checksum_bytes: update::MAX_CHECKSUM_BYTES,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct UpdateLimits {
    pub release_list_bytes: u64,
    pub apk_bytes: u64,
    pub checksum_bytes: u64,
}
