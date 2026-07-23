use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{AssetKind, AssetManifest};

pub const VIDDEXA_REPOSITORY: &str = "https://huggingface.co/viddexa/nsfw-detection-2-nano";
pub const VIDDEXA_REVISION: &str = "913bc502e69fa3edfe2cfce72c98cad4ddc6149b";
pub const VIDDEXA_LABELS: [&str; 5] = ["normal", "hentai", "porn", "sexy", "drawing"];
pub const VIDDEXA_SCALE: f32 = 1.0 / 255.0;
pub const VIDDEXA_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
pub const VIDDEXA_STD: [f32; 3] = [0.47853944, 0.4732864, 0.47434163];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TensorLayout {
    Nchw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorOrder {
    Rgb,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageInputContract {
    pub name: String,
    pub shape: [usize; 4],
    pub layout: TensorLayout,
    pub color_order: ColorOrder,
    pub scale: f32,
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelLabel {
    pub index: usize,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageModelManifest {
    pub asset: AssetManifest,
    pub source_repository: String,
    pub source_revision: String,
    pub file_name: String,
    pub file_bytes: u64,
    pub opset: u16,
    pub minimum_runtime_version: String,
    pub input: ImageInputContract,
    pub output_name: String,
    pub labels: Vec<ModelLabel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImageManifestError {
    #[error("image model asset metadata is invalid")]
    InvalidAsset,
    #[error("image model source is invalid")]
    InvalidSource,
    #[error("image model file metadata is invalid")]
    InvalidFile,
    #[error("image model opset is invalid")]
    InvalidOpset,
    #[error("image model runtime version is invalid")]
    InvalidRuntimeVersion,
    #[error("image model input shape is invalid")]
    InvalidInputShape,
    #[error("image model input or output name is invalid")]
    InvalidTensorName,
    #[error("image model normalization is invalid")]
    InvalidNormalization,
    #[error("image model labels are invalid")]
    InvalidLabels,
}

impl ImageModelManifest {
    pub fn validate(&self) -> Result<(), ImageManifestError> {
        self.asset
            .validate()
            .map_err(|_| ImageManifestError::InvalidAsset)?;
        if self.asset.kind != AssetKind::ImageClassifier {
            return Err(ImageManifestError::InvalidAsset);
        }
        if self.source_repository != VIDDEXA_REPOSITORY || self.source_revision != VIDDEXA_REVISION
        {
            return Err(ImageManifestError::InvalidSource);
        }
        if self.file_bytes == 0 || !is_safe_file_name(&self.file_name) {
            return Err(ImageManifestError::InvalidFile);
        }
        if self.opset != 18 {
            return Err(ImageManifestError::InvalidOpset);
        }
        if self.minimum_runtime_version != "1.22" {
            return Err(ImageManifestError::InvalidRuntimeVersion);
        }
        if self.input.shape != [1, 3, 224, 224]
            || self.input.layout != TensorLayout::Nchw
            || self.input.color_order != ColorOrder::Rgb
        {
            return Err(ImageManifestError::InvalidInputShape);
        }
        if self.input.name.trim().is_empty() || self.output_name.trim().is_empty() {
            return Err(ImageManifestError::InvalidTensorName);
        }
        if self.input.scale != VIDDEXA_SCALE
            || self.input.mean != VIDDEXA_MEAN
            || self.input.std != VIDDEXA_STD
        {
            return Err(ImageManifestError::InvalidNormalization);
        }
        self.validate_labels()
    }

    fn validate_labels(&self) -> Result<(), ImageManifestError> {
        if self.labels.len() != VIDDEXA_LABELS.len() {
            return Err(ImageManifestError::InvalidLabels);
        }
        let mut seen = [false; VIDDEXA_LABELS.len()];
        for label in &self.labels {
            if label.index >= seen.len()
                || seen[label.index]
                || !VIDDEXA_LABELS.contains(&label.name.as_str())
            {
                return Err(ImageManifestError::InvalidLabels);
            }
            seen[label.index] = true;
        }
        for required in VIDDEXA_LABELS {
            if self.labels.iter().all(|label| label.name != required) {
                return Err(ImageManifestError::InvalidLabels);
            }
        }
        Ok(())
    }
}

fn is_safe_file_name(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetKind, AssetManifest};

    fn valid_manifest() -> ImageModelManifest {
        ImageModelManifest {
            asset: AssetManifest {
                kind: AssetKind::ImageClassifier,
                version: "viddexa-nano-1".into(),
                license: "Apache-2.0".into(),
                sha256: "a".repeat(64),
            },
            source_repository: VIDDEXA_REPOSITORY.into(),
            source_revision: VIDDEXA_REVISION.into(),
            file_name: "model.onnx".into(),
            file_bytes: 1_024,
            opset: 18,
            minimum_runtime_version: "1.22".into(),
            input: ImageInputContract {
                name: "pixel_values".into(),
                shape: [1, 3, 224, 224],
                layout: TensorLayout::Nchw,
                color_order: ColorOrder::Rgb,
                scale: VIDDEXA_SCALE,
                mean: VIDDEXA_MEAN,
                std: VIDDEXA_STD,
            },
            output_name: "logits".into(),
            labels: VIDDEXA_LABELS
                .iter()
                .enumerate()
                .map(|(index, name)| ModelLabel {
                    index,
                    name: (*name).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn valid_manifest_passes() {
        assert_eq!(valid_manifest().validate(), Ok(()));
    }

    #[test]
    fn rejects_duplicate_or_missing_required_labels() {
        let mut value = valid_manifest();
        value.labels[4].name = "normal".into();

        assert_eq!(value.validate(), Err(ImageManifestError::InvalidLabels));
    }

    #[test]
    fn rejects_non_static_nchw_shape() {
        let mut value = valid_manifest();
        value.input.shape = [2, 3, 224, 224];

        assert_eq!(value.validate(), Err(ImageManifestError::InvalidInputShape));
    }

    #[test]
    fn rejects_invalid_normalization() {
        let mut value = valid_manifest();
        value.input.std[1] = 0.473;

        assert_eq!(
            value.validate(),
            Err(ImageManifestError::InvalidNormalization)
        );

        let mut value = valid_manifest();
        value.input.scale = 1.0;
        assert_eq!(
            value.validate(),
            Err(ImageManifestError::InvalidNormalization)
        );
    }

    #[test]
    fn rejects_unpinned_source_or_unsafe_file() {
        let mut value = valid_manifest();
        value.source_revision = "main".into();
        assert_eq!(value.validate(), Err(ImageManifestError::InvalidSource));

        let mut value = valid_manifest();
        value.file_name = "../model.onnx".into();
        assert_eq!(value.validate(), Err(ImageManifestError::InvalidFile));
    }

    #[test]
    fn rejects_zero_file_length_and_wrong_opset() {
        let mut value = valid_manifest();
        value.file_bytes = 0;
        assert_eq!(value.validate(), Err(ImageManifestError::InvalidFile));

        let mut value = valid_manifest();
        value.opset = 17;
        assert_eq!(value.validate(), Err(ImageManifestError::InvalidOpset));
    }
}
