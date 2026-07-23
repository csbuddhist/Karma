use std::collections::{HashMap, VecDeque};

use karma_domain::{Action, MonitorId, OcrRisk, ReasonCode, RiskCategory, RiskObservation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiskOutcome {
    pub action: Action,
    pub reason: ReasonCode,
}

#[derive(Debug, Default)]
pub struct RiskState {
    histories: HashMap<MonitorId, VecDeque<RiskObservation>>,
}

impl RiskState {
    pub fn observe(&mut self, observation: RiskObservation) -> RiskOutcome {
        let now = observation.captured_at_ms;
        let history = self
            .histories
            .entry(observation.monitor_id.clone())
            .or_default();

        history.retain(|item| now - item.captured_at_ms <= 10_000);
        history.push_back(observation);

        let current = history.back().expect("inserted observation exists");
        if current.image_score_millis >= 950 {
            return RiskOutcome {
                action: Action::CloseGracefully,
                reason: ReasonCode::ImageImmediate,
            };
        }

        let repeated_images = history
            .iter()
            .filter(|item| now - item.captured_at_ms <= 5_000 && item.image_score_millis >= 820)
            .count();
        if repeated_images >= 3 {
            return RiskOutcome {
                action: Action::CloseGracefully,
                reason: ReasonCode::ImageRepeated,
            };
        }

        let combined_image_and_ocr = history
            .iter()
            .filter(|item| {
                now - item.captured_at_ms <= 5_000
                    && item.image_score_millis >= 650
                    && item.ocr_risk == OcrRisk::HighRiskPhrase
                    && !item
                        .ocr_categories
                        .contains(&RiskCategory::ExemptionContext)
            })
            .count();
        if combined_image_and_ocr >= 2 {
            return RiskOutcome {
                action: Action::CloseGracefully,
                reason: ReasonCode::OcrImageCombined,
            };
        }

        if current.ocr_risk != OcrRisk::None {
            return RiskOutcome {
                action: Action::Warn,
                reason: ReasonCode::OcrOnlyWarning,
            };
        }

        RiskOutcome {
            action: Action::Allow,
            reason: ReasonCode::DefaultAllow,
        }
    }
}

#[cfg(test)]
mod tests {
    use karma_domain::{
        Action, ModelVersions, MonitorId, OcrRisk, ReasonCode, RiskCategory, RiskObservation,
    };

    use super::*;

    fn observation(monitor: &str, at: i64, score: u16, ocr: OcrRisk) -> RiskObservation {
        RiskObservation {
            monitor_id: MonitorId(monitor.into()),
            captured_at_ms: at,
            image_score_millis: score,
            image_labels: vec![RiskCategory::Suggestive],
            ocr_risk: ocr,
            ocr_categories: vec![],
            source_identity: None,
            model_versions: ModelVersions {
                image: "image-v1".into(),
                ocr: "ocr-v1".into(),
                word_pack: "words-v1".into(),
            },
        }
    }

    #[test]
    fn immediate_image_threshold_closes() {
        let result = RiskState::default().observe(observation("a", 1000, 950, OcrRisk::None));

        assert_eq!(
            result,
            RiskOutcome {
                action: Action::CloseGracefully,
                reason: ReasonCode::ImageImmediate,
            }
        );
    }

    #[test]
    fn three_image_hits_inside_five_seconds_close() {
        let mut state = RiskState::default();
        state.observe(observation("a", 1000, 820, OcrRisk::None));
        state.observe(observation("a", 3000, 830, OcrRisk::None));

        assert_eq!(
            state
                .observe(observation("a", 5000, 840, OcrRisk::None))
                .reason,
            ReasonCode::ImageRepeated
        );
    }

    #[test]
    fn ocr_only_warns_but_two_combined_hits_close() {
        let mut state = RiskState::default();

        assert_eq!(
            state
                .observe(observation("a", 1000, 200, OcrRisk::HighRiskPhrase))
                .action,
            Action::Warn
        );

        state.observe(observation("b", 2000, 650, OcrRisk::HighRiskPhrase));
        assert_eq!(
            state
                .observe(observation("b", 3000, 660, OcrRisk::HighRiskPhrase))
                .reason,
            ReasonCode::OcrImageCombined
        );
    }

    #[test]
    fn monitors_do_not_share_history() {
        let mut state = RiskState::default();
        state.observe(observation("a", 1000, 900, OcrRisk::None));
        state.observe(observation("a", 2000, 900, OcrRisk::None));

        assert_eq!(
            state
                .observe(observation("b", 3000, 900, OcrRisk::None))
                .action,
            Action::Allow
        );
    }

    #[test]
    fn exemption_context_suppresses_combined_ocr_rule() {
        let mut state = RiskState::default();
        let mut first = observation("a", 1000, 650, OcrRisk::HighRiskPhrase);
        first.ocr_categories.push(RiskCategory::ExemptionContext);
        let mut second = observation("a", 2000, 660, OcrRisk::HighRiskPhrase);
        second.ocr_categories.push(RiskCategory::ExemptionContext);

        state.observe(first);

        assert_eq!(state.observe(second).action, Action::Warn);
    }
}
