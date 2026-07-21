use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MonitorId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceIdentity {
    pub pid: u32,
    pub started_at_ms: i64,
    pub executable_path: String,
    pub publisher: Option<String>,
    pub sha256: Option<String>,
}
