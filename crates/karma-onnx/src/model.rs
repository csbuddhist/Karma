use std::{
    fmt,
    fs::{self, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

use karma_ai::ImageModelManifest;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const HASH_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceErrorKind {
    ManifestInvalid,
    ModelMissing,
    ModelHashMismatch,
    RuntimeInitialization,
    ModelContractMismatch,
    InputPreparation,
    InferenceFailed,
    OutputInvalid,
}

impl fmt::Display for InferenceErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ManifestInvalid => "manifest_invalid",
            Self::ModelMissing => "model_missing",
            Self::ModelHashMismatch => "model_hash_mismatch",
            Self::RuntimeInitialization => "runtime_initialization",
            Self::ModelContractMismatch => "model_contract_mismatch",
            Self::InputPreparation => "input_preparation",
            Self::InferenceFailed => "inference_failed",
            Self::OutputInvalid => "output_invalid",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{kind}")]
pub struct InferenceError {
    kind: InferenceErrorKind,
}

impl InferenceError {
    pub(crate) fn new(kind: InferenceErrorKind) -> Self {
        Self { kind }
    }

    pub fn kind(&self) -> InferenceErrorKind {
        self.kind
    }
}

pub struct VerifiedImageModel {
    manifest: ImageModelManifest,
    model_path: PathBuf,
}

impl fmt::Debug for VerifiedImageModel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerifiedImageModel")
            .field("version", &self.manifest.asset.version)
            .field("file_bytes", &self.manifest.file_bytes)
            .finish()
    }
}

impl VerifiedImageModel {
    pub fn load(manifest_path: impl AsRef<Path>) -> Result<Self, InferenceError> {
        let manifest_path = manifest_path.as_ref();
        let metadata = fs::metadata(manifest_path)
            .map_err(|_| InferenceError::new(InferenceErrorKind::ManifestInvalid))?;
        if metadata.len() > MAX_MANIFEST_BYTES as u64 {
            return Err(InferenceError::new(InferenceErrorKind::ManifestInvalid));
        }
        let bytes = fs::read(manifest_path)
            .map_err(|_| InferenceError::new(InferenceErrorKind::ManifestInvalid))?;
        let manifest: ImageModelManifest = serde_json::from_slice(&bytes)
            .map_err(|_| InferenceError::new(InferenceErrorKind::ManifestInvalid))?;
        manifest
            .validate()
            .map_err(|_| InferenceError::new(InferenceErrorKind::ManifestInvalid))?;
        let directory = manifest_path
            .parent()
            .ok_or_else(|| InferenceError::new(InferenceErrorKind::ManifestInvalid))?;
        let model_path = directory.join(&manifest.file_name);
        let model_metadata = fs::metadata(&model_path)
            .map_err(|_| InferenceError::new(InferenceErrorKind::ModelMissing))?;
        if model_metadata.len() != manifest.file_bytes {
            return Err(InferenceError::new(InferenceErrorKind::ModelHashMismatch));
        }
        let actual_hash = hash_file(&model_path)?;
        if actual_hash != manifest.asset.sha256 {
            return Err(InferenceError::new(InferenceErrorKind::ModelHashMismatch));
        }
        Ok(Self {
            manifest,
            model_path,
        })
    }

    pub fn manifest(&self) -> &ImageModelManifest {
        &self.manifest
    }

    pub fn create_classifier(&self) -> Result<crate::OnnxImageClassifier, InferenceError> {
        crate::OnnxImageClassifier::from_model(self)
    }

    pub(crate) fn model_path(&self) -> &Path {
        &self.model_path
    }
}

fn hash_file(path: &Path) -> Result<String, InferenceError> {
    let file =
        File::open(path).map_err(|_| InferenceError::new(InferenceErrorKind::ModelMissing))?;
    let mut reader = BufReader::with_capacity(HASH_BUFFER_BYTES, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; HASH_BUFFER_BYTES];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| InferenceError::new(InferenceErrorKind::ModelHashMismatch))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use karma_ai::{
        AssetKind, AssetManifest, ColorOrder, ImageInputContract, ImageModelManifest, ModelLabel,
        TensorLayout, VIDDEXA_LABELS, VIDDEXA_REPOSITORY, VIDDEXA_REVISION,
    };
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;

    fn manifest(model: &[u8]) -> ImageModelManifest {
        ImageModelManifest {
            asset: AssetManifest {
                kind: AssetKind::ImageClassifier,
                version: "viddexa-nano-1".into(),
                license: "Apache-2.0".into(),
                sha256: format!("{:x}", Sha256::digest(model)),
            },
            source_repository: VIDDEXA_REPOSITORY.into(),
            source_revision: VIDDEXA_REVISION.into(),
            file_name: "model.onnx".into(),
            file_bytes: model.len() as u64,
            opset: 18,
            minimum_runtime_version: "1.22".into(),
            input: ImageInputContract {
                name: "pixel_values".into(),
                shape: [1, 3, 224, 224],
                layout: TensorLayout::Nchw,
                color_order: ColorOrder::Rgb,
                scale: 1.0 / 255.0,
                mean: [0.485, 0.456, 0.406],
                std: [0.229, 0.224, 0.225],
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

    fn write_manifest(directory: &TempDir, value: &ImageModelManifest) -> std::path::PathBuf {
        let path = directory.path().join("manifest.json");
        fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
        path
    }

    #[test]
    fn verifies_length_and_sha_before_exposing_model() {
        let directory = TempDir::new().unwrap();
        let bytes = b"model";
        fs::write(directory.path().join("model.onnx"), bytes).unwrap();
        let path = write_manifest(&directory, &manifest(bytes));

        let verified = VerifiedImageModel::load(&path).unwrap();

        assert_eq!(verified.manifest().asset.version, "viddexa-nano-1");
    }

    #[test]
    fn missing_model_has_stable_error_without_path() {
        let directory = TempDir::new().unwrap();
        let path = write_manifest(&directory, &manifest(b"model"));

        let error = VerifiedImageModel::load(&path).unwrap_err();

        assert_eq!(error.kind(), InferenceErrorKind::ModelMissing);
        assert!(
            !error
                .to_string()
                .contains(directory.path().to_str().unwrap())
        );
    }

    #[test]
    fn changed_length_or_hash_is_rejected() {
        let directory = TempDir::new().unwrap();
        fs::write(directory.path().join("model.onnx"), b"changed").unwrap();
        let path = write_manifest(&directory, &manifest(b"model"));

        assert_eq!(
            VerifiedImageModel::load(&path).unwrap_err().kind(),
            InferenceErrorKind::ModelHashMismatch
        );

        let mut wrong_hash = manifest(b"changed");
        wrong_hash.asset.sha256 = "b".repeat(64);
        let path = write_manifest(&directory, &wrong_hash);
        assert_eq!(
            VerifiedImageModel::load(&path).unwrap_err().kind(),
            InferenceErrorKind::ModelHashMismatch
        );
    }

    #[test]
    fn oversized_or_invalid_manifest_is_rejected() {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("manifest.json");
        fs::write(&path, vec![b' '; MAX_MANIFEST_BYTES + 1]).unwrap();

        assert_eq!(
            VerifiedImageModel::load(&path).unwrap_err().kind(),
            InferenceErrorKind::ManifestInvalid
        );
    }
}
