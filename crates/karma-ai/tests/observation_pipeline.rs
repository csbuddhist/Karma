use karma_ai::{ImageInference, ObservationAssembler, ObservationInput, OcrMatchSummary};
use karma_domain::{MonitorId, OcrRisk, RiskCategory};

#[test]
fn assembly_maps_categories_without_text() {
    let value = ObservationAssembler::assemble(ObservationInput {
        monitor_id: MonitorId("display-1".into()),
        captured_at_ms: 42,
        image: ImageInference {
            score_millis: 700,
            categories: vec![RiskCategory::Suggestive],
        },
        ocr: OcrMatchSummary {
            risk: OcrRisk::HighRiskPhrase,
            categories: vec!["explicit_term".into()],
            exemption_context: false,
        },
        image_model_version: "i1".into(),
        ocr_model_version: "o1".into(),
        word_pack_version: "w1".into(),
    });
    assert_eq!(value.ocr_categories, vec![RiskCategory::ExplicitTerm]);
    let json = serde_json::to_string(&value).unwrap();
    assert!(!json.contains("recognized_text"));
    assert!(!json.contains("raw_text"));
}

#[test]
fn exemption_context_is_preserved_as_category() {
    let value = ObservationAssembler::assemble(ObservationInput {
        monitor_id: MonitorId("display-1".into()),
        captured_at_ms: 42,
        image: ImageInference {
            score_millis: 0,
            categories: vec![],
        },
        ocr: OcrMatchSummary {
            risk: OcrRisk::None,
            categories: vec!["medical".into()],
            exemption_context: true,
        },
        image_model_version: "i1".into(),
        ocr_model_version: "o1".into(),
        word_pack_version: "w1".into(),
    });
    assert!(
        value
            .ocr_categories
            .contains(&RiskCategory::ExemptionContext)
    );
}
