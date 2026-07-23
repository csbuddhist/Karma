use std::fmt;

use karma_domain::RiskCategory;
use thiserror::Error;

use crate::{ImageInference, PreparedFrame, VIDDEXA_LABELS};

#[derive(Clone, PartialEq)]
pub struct ClassifierOutput {
    labels: Vec<String>,
    probabilities: Vec<f32>,
}

impl fmt::Debug for ClassifierOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClassifierOutput")
            .field("labels", &self.labels)
            .field("probability_count", &self.probabilities.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ClassifierOutputError {
    #[error("classifier labels and probabilities have different lengths")]
    LengthMismatch,
    #[error("classifier labels are invalid")]
    InvalidLabels,
    #[error("classifier probability is invalid")]
    InvalidProbability,
    #[error("classifier probability sum is invalid")]
    InvalidProbabilitySum,
}

impl ClassifierOutput {
    pub fn new(
        labels: Vec<String>,
        probabilities: Vec<f32>,
    ) -> Result<Self, ClassifierOutputError> {
        if labels.len() != probabilities.len() {
            return Err(ClassifierOutputError::LengthMismatch);
        }
        if labels.len() != VIDDEXA_LABELS.len()
            || VIDDEXA_LABELS
                .iter()
                .any(|required| labels.iter().filter(|label| label == required).count() != 1)
        {
            return Err(ClassifierOutputError::InvalidLabels);
        }
        if probabilities
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        {
            return Err(ClassifierOutputError::InvalidProbability);
        }
        let sum: f32 = probabilities.iter().sum();
        if (sum - 1.0).abs() > 0.01 {
            return Err(ClassifierOutputError::InvalidProbabilitySum);
        }
        Ok(Self {
            labels,
            probabilities,
        })
    }

    pub fn probability(&self, label: &str) -> Result<f32, ClassifierOutputError> {
        self.labels
            .iter()
            .position(|candidate| candidate == label)
            .map(|index| self.probabilities[index])
            .ok_or(ClassifierOutputError::InvalidLabels)
    }
}

pub trait ImageClassifier {
    type Error;

    fn classify(&mut self, frame: &PreparedFrame) -> Result<ImageInference, Self::Error>;
}

pub struct ViddexaRiskMapper;

impl ViddexaRiskMapper {
    pub fn map(output: &ClassifierOutput) -> Result<ImageInference, ClassifierOutputError> {
        let explicit = output.probability("porn")? + output.probability("hentai")?;
        let suggestive = output.probability("sexy")?;
        let score = (explicit + 0.35 * suggestive).clamp(0.0, 1.0);
        let mut categories = Vec::with_capacity(2);
        if explicit >= 0.5 {
            categories.push(RiskCategory::Nudity);
        }
        if suggestive >= 0.5 {
            categories.push(RiskCategory::Suggestive);
        }
        Ok(ImageInference {
            score_millis: (score * 1_000.0).round() as u16,
            categories,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use karma_domain::RiskCategory;

    #[test]
    fn maps_probabilities_by_label_name_not_position() {
        let output = ClassifierOutput::new(
            vec![
                "sexy".into(),
                "drawing".into(),
                "porn".into(),
                "normal".into(),
                "hentai".into(),
            ],
            vec![0.20, 0.05, 0.45, 0.10, 0.20],
        )
        .unwrap();

        let inference = ViddexaRiskMapper::map(&output).unwrap();

        assert_eq!(inference.score_millis, 720);
        assert_eq!(inference.categories, vec![RiskCategory::Nudity]);
    }

    #[test]
    fn suggestive_threshold_adds_category() {
        let output = ClassifierOutput::new(
            vec![
                "normal".into(),
                "hentai".into(),
                "porn".into(),
                "sexy".into(),
                "drawing".into(),
            ],
            vec![0.25, 0.05, 0.05, 0.60, 0.05],
        )
        .unwrap();

        let inference = ViddexaRiskMapper::map(&output).unwrap();

        assert_eq!(inference.score_millis, 310);
        assert_eq!(inference.categories, vec![RiskCategory::Suggestive]);
    }

    #[test]
    fn rejects_invalid_probability_vectors() {
        let labels = vec![
            "normal".into(),
            "hentai".into(),
            "porn".into(),
            "sexy".into(),
            "drawing".into(),
        ];
        assert_eq!(
            ClassifierOutput::new(labels.clone(), vec![f32::NAN, 0.2, 0.2, 0.2, 0.2]),
            Err(ClassifierOutputError::InvalidProbability)
        );
        assert_eq!(
            ClassifierOutput::new(labels.clone(), vec![0.4, 0.4, 0.4, 0.0, 0.0]),
            Err(ClassifierOutputError::InvalidProbabilitySum)
        );
        assert_eq!(
            ClassifierOutput::new(labels, vec![0.2, 0.2, 0.2, 0.2]),
            Err(ClassifierOutputError::LengthMismatch)
        );
    }

    #[test]
    fn rejects_duplicate_or_unknown_labels() {
        assert_eq!(
            ClassifierOutput::new(
                vec![
                    "normal".into(),
                    "hentai".into(),
                    "porn".into(),
                    "sexy".into(),
                    "normal".into(),
                ],
                vec![0.2; 5],
            ),
            Err(ClassifierOutputError::InvalidLabels)
        );
    }
}
