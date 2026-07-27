use std::{
    collections::HashSet,
    path::{Component, Path},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{AssetKind, AssetManifest, ColorOrder, ManifestError, TensorLayout};

pub const MAX_OCR_MODEL_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_OCR_DICTIONARY_BYTES: u64 = 4 * 1024 * 1024;
const APACHE_2_LICENSE: &str = "Apache-2.0";
const OCR_OPSET: u16 = 18;
const OCR_RUNTIME_VERSION: &str = "1.22";
const SUPPORTED_LANGUAGES: [OcrLanguage; 3] = [
    OcrLanguage::English,
    OcrLanguage::ChineseSimplified,
    OcrLanguage::ChineseTraditional,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrModelProfile {
    Lightweight,
    Accurate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrLanguage {
    English,
    ChineseSimplified,
    ChineseTraditional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrTensorElementType {
    F32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OcrModelAsset {
    pub asset: AssetManifest,
    pub file_name: String,
    pub file_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OcrDictionaryManifest {
    pub asset: AssetManifest,
    pub file_name: String,
    pub file_bytes: u64,
    pub entries: Vec<String>,
    pub blank_index: usize,
    pub languages: Vec<OcrLanguage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OcrTensorContract {
    pub input_name: String,
    pub output_name: String,
    pub layout: TensorLayout,
    pub color_order: ColorOrder,
    pub element_type: OcrTensorElementType,
    pub channels: usize,
    pub minimum_height: usize,
    pub maximum_height: usize,
    pub minimum_width: usize,
    pub maximum_width: usize,
    pub dimension_multiple: usize,
    pub scale: f32,
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OcrThresholds {
    pub probability: f32,
    pub text_box: f32,
    pub expansion: f32,
    pub recognition_confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OcrResourceLimits {
    pub maximum_text_boxes: usize,
    pub minimum_box_side_pixels: usize,
    pub minimum_box_area_pixels: usize,
    pub recognizer_height: usize,
    pub maximum_recognizer_width: usize,
    pub maximum_batch_size: usize,
    pub maximum_line_characters: usize,
    pub maximum_total_characters: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OcrBundleManifest {
    pub asset: AssetManifest,
    pub profile: OcrModelProfile,
    pub source_repository: String,
    pub source_revision: String,
    pub detector: OcrModelAsset,
    pub recognizer: OcrModelAsset,
    pub dictionary: OcrDictionaryManifest,
    pub detector_contract: OcrTensorContract,
    pub recognizer_contract: OcrTensorContract,
    pub thresholds: OcrThresholds,
    pub resource_limits: OcrResourceLimits,
    pub opset: u16,
    pub minimum_runtime_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OcrManifestError {
    #[error("OCR bundle asset metadata is invalid")]
    InvalidAsset,
    #[error("OCR bundle license is invalid")]
    InvalidLicense,
    #[error("OCR bundle source is invalid")]
    InvalidSource,
    #[error("OCR bundle opset is invalid")]
    InvalidOpset,
    #[error("OCR bundle runtime version is invalid")]
    InvalidRuntimeVersion,
    #[error("OCR detector tensor contract is invalid")]
    InvalidDetectorContract,
    #[error("OCR recognizer tensor contract is invalid")]
    InvalidRecognizerContract,
    #[error("OCR dictionary is invalid")]
    InvalidDictionary,
    #[error("OCR thresholds are invalid")]
    InvalidThresholds,
    #[error("OCR resource limits are invalid")]
    InvalidResourceLimits,
}

impl OcrBundleManifest {
    pub fn validate(&self) -> Result<(), OcrManifestError> {
        validate_asset(&self.asset, AssetKind::OcrBundle, None, None)?;
        if self.source_repository.trim().is_empty()
            || !self.source_repository.starts_with("https://")
            || !is_pinned_revision(&self.source_revision)
        {
            return Err(OcrManifestError::InvalidSource);
        }
        validate_asset(
            &self.detector.asset,
            AssetKind::OcrDetector,
            Some((&self.detector.file_name, self.detector.file_bytes)),
            Some(MAX_OCR_MODEL_BYTES),
        )?;
        if self.detector.file_name != "detector.onnx" {
            return Err(OcrManifestError::InvalidAsset);
        }
        validate_asset(
            &self.recognizer.asset,
            AssetKind::OcrRecognizer,
            Some((&self.recognizer.file_name, self.recognizer.file_bytes)),
            Some(MAX_OCR_MODEL_BYTES),
        )?;
        if self.recognizer.file_name != "recognizer.onnx" {
            return Err(OcrManifestError::InvalidAsset);
        }
        self.validate_dictionary()?;
        if self.opset != OCR_OPSET {
            return Err(OcrManifestError::InvalidOpset);
        }
        if self.minimum_runtime_version != OCR_RUNTIME_VERSION {
            return Err(OcrManifestError::InvalidRuntimeVersion);
        }
        if !is_detector_contract_valid(&self.detector_contract) {
            return Err(OcrManifestError::InvalidDetectorContract);
        }
        if !is_recognizer_contract_valid(&self.recognizer_contract) {
            return Err(OcrManifestError::InvalidRecognizerContract);
        }
        if !are_thresholds_valid(&self.thresholds) {
            return Err(OcrManifestError::InvalidThresholds);
        }
        if !has_exact_resource_limits(&self.resource_limits) {
            return Err(OcrManifestError::InvalidResourceLimits);
        }
        Ok(())
    }

    fn validate_dictionary(&self) -> Result<(), OcrManifestError> {
        validate_asset(
            &self.dictionary.asset,
            AssetKind::OcrDictionary,
            Some((&self.dictionary.file_name, self.dictionary.file_bytes)),
            Some(MAX_OCR_DICTIONARY_BYTES),
        )
        .map_err(|_| OcrManifestError::InvalidDictionary)?;
        if self.dictionary.file_name != "dictionary.txt"
            || self.dictionary.entries.is_empty()
            || self.dictionary.entries.iter().any(|entry| entry.is_empty())
            || self.dictionary.blank_index > self.dictionary.entries.len()
            || !has_supported_languages(&self.dictionary.languages)
        {
            return Err(OcrManifestError::InvalidDictionary);
        }
        let unique_entries: HashSet<&str> =
            self.dictionary.entries.iter().map(String::as_str).collect();
        if unique_entries.len() != self.dictionary.entries.len() {
            return Err(OcrManifestError::InvalidDictionary);
        }
        Ok(())
    }
}

fn validate_asset(
    asset: &AssetManifest,
    expected_kind: AssetKind,
    file: Option<(&str, u64)>,
    maximum_bytes: Option<u64>,
) -> Result<(), OcrManifestError> {
    asset
        .validate()
        .map_err(|_: ManifestError| OcrManifestError::InvalidAsset)?;
    if asset.kind != expected_kind || asset.license != APACHE_2_LICENSE {
        return Err(if asset.license != APACHE_2_LICENSE {
            OcrManifestError::InvalidLicense
        } else {
            OcrManifestError::InvalidAsset
        });
    }
    if let Some((file_name, file_bytes)) = file {
        if file_bytes == 0
            || !is_safe_file_name(file_name)
            || maximum_bytes.is_some_and(|limit| file_bytes > limit)
        {
            return Err(OcrManifestError::InvalidAsset);
        }
    }
    Ok(())
}

fn is_pinned_revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn is_safe_file_name(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn is_detector_contract_valid(contract: &OcrTensorContract) -> bool {
    has_valid_tensor_names(contract)
        && contract.layout == TensorLayout::Nchw
        && contract.color_order == ColorOrder::Rgb
        && contract.element_type == OcrTensorElementType::F32
        && contract.channels == 3
        && contract.dimension_multiple == 32
        && contract.minimum_height >= 32
        && contract.maximum_height <= 640
        && contract.minimum_height <= contract.maximum_height
        && contract.minimum_width >= 32
        && contract.maximum_width <= 640
        && contract.minimum_width <= contract.maximum_width
        && contract.minimum_height % 32 == 0
        && contract.maximum_height % 32 == 0
        && contract.minimum_width % 32 == 0
        && contract.maximum_width % 32 == 0
        && has_finite_normalization(contract)
}

fn is_recognizer_contract_valid(contract: &OcrTensorContract) -> bool {
    has_valid_tensor_names(contract)
        && contract.layout == TensorLayout::Nchw
        && contract.color_order == ColorOrder::Rgb
        && contract.element_type == OcrTensorElementType::F32
        && contract.channels == 3
        && contract.minimum_height == 48
        && contract.maximum_height == 48
        && contract.minimum_width == 1
        && contract.maximum_width == 320
        && contract.dimension_multiple == 1
        && has_finite_normalization(contract)
}

fn has_valid_tensor_names(contract: &OcrTensorContract) -> bool {
    !contract.input_name.trim().is_empty() && !contract.output_name.trim().is_empty()
}

fn has_finite_normalization(contract: &OcrTensorContract) -> bool {
    contract.scale.is_finite()
        && contract.mean.iter().all(|value| value.is_finite())
        && contract
            .std
            .iter()
            .all(|value| value.is_finite() && *value != 0.0)
}

fn has_supported_languages(languages: &[OcrLanguage]) -> bool {
    languages.len() == SUPPORTED_LANGUAGES.len()
        && SUPPORTED_LANGUAGES.iter().all(|language| {
            languages
                .iter()
                .filter(|candidate| *candidate == language)
                .count()
                == 1
        })
}

fn are_thresholds_valid(thresholds: &OcrThresholds) -> bool {
    thresholds.probability.is_finite()
        && (0.0..=1.0).contains(&thresholds.probability)
        && thresholds.text_box.is_finite()
        && (0.0..=1.0).contains(&thresholds.text_box)
        && thresholds.expansion.is_finite()
        && (1.0..=3.0).contains(&thresholds.expansion)
        && thresholds.recognition_confidence.is_finite()
        && (0.0..=1.0).contains(&thresholds.recognition_confidence)
}

fn has_exact_resource_limits(limits: &OcrResourceLimits) -> bool {
    limits.maximum_text_boxes == 64
        && limits.minimum_box_side_pixels == 6
        && limits.minimum_box_area_pixels == 48
        && limits.recognizer_height == 48
        && limits.maximum_recognizer_width == 320
        && limits.maximum_batch_size == 8
        && limits.maximum_line_characters == 128
        && limits.maximum_total_characters == 4_096
}
