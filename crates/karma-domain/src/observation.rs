use serde::{Deserialize, Serialize};

use crate::{MonitorId, SourceIdentity};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskCategory {
    Nudity,
    Suggestive,
    ExplicitTerm,
    AdultService,
    ExemptionContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelVersions {
    pub image: String,
    pub ocr: String,
    pub word_pack: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskObservation {
    pub monitor_id: MonitorId,
    pub captured_at_ms: i64,
    pub image_score_millis: u16,
    pub image_labels: Vec<RiskCategory>,
    pub ocr_risk: OcrRisk,
    pub ocr_categories: Vec<RiskCategory>,
    pub source_identity: Option<SourceIdentity>,
    pub model_versions: ModelVersions,
}

#[cfg(test)]
impl RiskObservation {
    fn test_value(image_score_millis: u16, ocr_risk: OcrRisk) -> Self {
        Self {
            monitor_id: MonitorId("display-1".into()),
            captured_at_ms: 1,
            image_score_millis,
            image_labels: vec![RiskCategory::Suggestive],
            ocr_risk,
            ocr_categories: vec![RiskCategory::ExplicitTerm],
            source_identity: None,
            model_versions: ModelVersions {
                image: "image-v1".into(),
                ocr: "ocr-v1".into(),
                word_pack: "words-v1".into(),
            },
        }
    }
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

    #[test]
    fn observation_serializes_categories_without_raw_text() {
        let value = RiskObservation::test_value(640, OcrRisk::HighRiskPhrase);
        let json = serde_json::to_string(&value).unwrap();

        assert!(json.contains("explicit_term"));
        assert!(!json.contains("raw_text"));
        assert!(!json.contains("recognized_text"));
    }
}
