//! Values that cross the Kotlin/Rust boundary. They carry scores and
//! metadata only, never pixels or recognized text.

use karma_domain::{Action, ReasonCode};
use serde::{Deserialize, Serialize};

/// Softmax output of the viddexa-nano classifier, produced on the platform.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct ClassifierProbabilities {
    pub normal: f32,
    pub hentai: f32,
    pub porn: f32,
    pub sexy: f32,
    pub drawing: f32,
}

/// What the accessibility service knows about the foreground window.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ForegroundContext {
    pub package_name: String,
    /// Window, page, or video title read from the accessibility tree.
    pub window_title: String,
    /// Host read from a browser address bar; `None` outside browsers.
    pub browser_host: Option<String>,
    /// The screenshot was refused because the window sets `FLAG_SECURE`.
    pub secure_window: bool,
    pub captured_at_ms: i64,
    /// Local minutes since Monday 00:00, `0..10080`.
    pub minute_of_week: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum MobileAction {
    Allow,
    Warn,
    CloseGracefully,
    /// Close and, under Device Owner, suspend the package until the decision expires.
    Terminate,
    /// Reserved for a future `VpnService` layer; never produced in v0.1.
    BlockNetwork,
}

impl From<Action> for MobileAction {
    fn from(value: Action) -> Self {
        match value {
            Action::Allow => Self::Allow,
            Action::Warn => Self::Warn,
            Action::CloseGracefully => Self::CloseGracefully,
            Action::Terminate => Self::Terminate,
            Action::BlockNetwork => Self::BlockNetwork,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum MobileReason {
    DefaultAllow,
    ProtectionDisabled,
    ExemptApplication,
    AllowedApplication,
    AllowedWebsite,
    TimeWindowBlocked,
    ApplicationBlocked,
    BlockedWebsite,
    TitleKeyword,
    SecureWindow,
    ImageImmediate,
    ImageRepeated,
    OcrImageCombined,
    OcrOnlyWarning,
    SourceUncertain,
}

impl From<ReasonCode> for MobileReason {
    fn from(value: ReasonCode) -> Self {
        match value {
            ReasonCode::DefaultAllow => Self::DefaultAllow,
            ReasonCode::TimeWindowBlocked => Self::TimeWindowBlocked,
            ReasonCode::ApplicationBlocked => Self::ApplicationBlocked,
            ReasonCode::ImageImmediate => Self::ImageImmediate,
            ReasonCode::ImageRepeated => Self::ImageRepeated,
            ReasonCode::OcrImageCombined => Self::OcrImageCombined,
            ReasonCode::OcrOnlyWarning => Self::OcrOnlyWarning,
            ReasonCode::SourceUncertain => Self::SourceUncertain,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileDecision {
    pub action: MobileAction,
    pub reason: MobileReason,
    pub policy_id: String,
    /// End of the cooldown during which the package is closed again on sight.
    pub expires_at_ms: Option<i64>,
    /// Risk score of the frame, `0..=1000`, when a frame was scored.
    pub image_score_millis: Option<u32>,
    /// The decision repeats an earlier closure still in its cooldown.
    pub from_cooldown: bool,
}

impl MobileDecision {
    pub fn allow(reason: MobileReason, policy_id: impl Into<String>) -> Self {
        Self {
            action: MobileAction::Allow,
            reason,
            policy_id: policy_id.into(),
            expires_at_ms: None,
            image_score_millis: None,
            from_cooldown: false,
        }
    }

    pub fn closes(&self) -> bool {
        matches!(
            self.action,
            MobileAction::CloseGracefully | MobileAction::Terminate
        )
    }
}

/// How the frame scheduler should treat a foreground package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PackageClass {
    /// Never captured or closed.
    Exempt,
    /// The administrator allows it: never captured.
    Allowed,
    /// The administrator blocks it: closed without capturing.
    Blocked,
    /// A browser: address bar read, capture on content events.
    Browser,
    /// A video player: sampled at a fixed rate.
    Video,
    /// Any other application: capture on content events.
    Monitored,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AdminSession {
    pub token: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Enrollment {
    pub session: AdminSession,
    /// One-time recovery codes, shown once and stored only as hashes.
    pub recovery_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct AuditEventRecord {
    pub occurred_at_ms: i64,
    /// Short machine-readable kind, such as `decision_applied`.
    pub kind: String,
    /// `success`, `denied`, `rate_limited`, `applied`, or a platform outcome.
    pub outcome: String,
    pub package_name: Option<String>,
    pub action: Option<MobileAction>,
    pub reason: Option<MobileReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CoreStatus {
    pub enrolled: bool,
    pub protection_enabled: bool,
    pub policy_revision: u64,
    pub recovery_codes_remaining: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("state storage is unavailable")]
    Storage,
    #[error("the state key is invalid or does not match the stored state")]
    InvalidKey,
    #[error("the policy document is invalid")]
    InvalidPolicy,
    #[error("the classifier output is invalid")]
    InvalidClassifierOutput,
    #[error("the request is invalid")]
    InvalidRequest,
    #[error("administrator authorization is required")]
    Unauthorized,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum AuthError {
    #[error("no administrator password is set")]
    NotEnrolled,
    #[error("an administrator password is already set")]
    AlreadyEnrolled,
    #[error("the password must be at least 10 characters")]
    WeakPassword,
    #[error("authentication failed")]
    Failed,
    #[error("too many failed attempts")]
    RateLimited { retry_after_ms: i64 },
    #[error("the administrator session expired")]
    SessionExpired,
    #[error("state storage is unavailable")]
    Storage,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum SettingsError {
    #[error("no administrator password is set")]
    NotEnrolled,
    #[error("the administrator must sign in once before settings can be exported")]
    KeyNotReady,
    #[error("the file is not a Karma settings file")]
    InvalidFile,
    #[error("the file was protected with another administrator password")]
    NeedsAdministrator,
    #[error("the file was changed or the password is wrong")]
    Rejected,
    #[error("the settings in the file are invalid")]
    InvalidPolicy,
    #[error("the administrator session expired")]
    SessionExpired,
    #[error("state storage is unavailable")]
    Storage,
}
