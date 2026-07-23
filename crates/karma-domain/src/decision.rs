use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Allow,
    Warn,
    CloseGracefully,
    Terminate,
    BlockNetwork,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    DefaultAllow,
    TimeWindowBlocked,
    ApplicationBlocked,
    ImageImmediate,
    ImageRepeated,
    OcrImageCombined,
    OcrOnlyWarning,
    SourceUncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub action: Action,
    pub reason: ReasonCode,
    pub policy_id: String,
    pub expires_at_ms: Option<i64>,
}
