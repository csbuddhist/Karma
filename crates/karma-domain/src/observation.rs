use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Versioned<T> {
    pub schema_version: u16,
    pub payload: T,
}

impl<T> Versioned<T> {
    pub fn new(payload: T) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            payload,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrRisk {
    None,
    Keyword,
    HighRiskPhrase,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versioned_payload_serializes_schema_version() {
        let json = serde_json::to_value(Versioned::new(OcrRisk::Keyword)).unwrap();

        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["payload"], "keyword");
    }
}
