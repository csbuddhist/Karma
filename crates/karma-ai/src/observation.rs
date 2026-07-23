use karma_domain::{ModelVersions, MonitorId, RiskCategory, RiskObservation};

use crate::OcrMatchSummary;

pub struct ImageInference {
    pub score_millis: u16,
    pub categories: Vec<RiskCategory>,
}

pub struct ObservationInput {
    pub monitor_id: MonitorId,
    pub captured_at_ms: i64,
    pub image: ImageInference,
    pub ocr: OcrMatchSummary,
    pub image_model_version: String,
    pub ocr_model_version: String,
    pub word_pack_version: String,
}

pub struct ObservationAssembler;

fn category_rank(value: &RiskCategory) -> u8 {
    match value {
        RiskCategory::Nudity => 0,
        RiskCategory::Suggestive => 1,
        RiskCategory::ExplicitTerm => 2,
        RiskCategory::AdultService => 3,
        RiskCategory::ExemptionContext => 4,
    }
}

impl ObservationAssembler {
    pub fn assemble(input: ObservationInput) -> RiskObservation {
        let mut ocr_categories = input
            .ocr
            .categories
            .iter()
            .filter_map(|value| match value.as_str() {
                "explicit_term" => Some(RiskCategory::ExplicitTerm),
                "adult_service" => Some(RiskCategory::AdultService),
                _ => None,
            })
            .collect::<Vec<_>>();

        if input.ocr.exemption_context {
            ocr_categories.push(RiskCategory::ExemptionContext);
        }
        ocr_categories.sort_by_key(category_rank);
        ocr_categories.dedup();

        RiskObservation {
            monitor_id: input.monitor_id,
            captured_at_ms: input.captured_at_ms,
            image_score_millis: input.image.score_millis,
            image_labels: input.image.categories,
            ocr_risk: input.ocr.risk,
            ocr_categories,
            source_identity: None,
            model_versions: ModelVersions {
                image: input.image_model_version,
                ocr: input.ocr_model_version,
                word_pack: input.word_pack_version,
            },
        }
    }
}
