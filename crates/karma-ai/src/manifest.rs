use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    ImageClassifier,
    OcrDetector,
    OcrRecognizer,
    WordPack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetManifest {
    pub kind: AssetKind,
    pub version: String,
    pub license: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManifestError {
    #[error("asset version is required")]
    MissingVersion,
    #[error("asset license is required")]
    MissingLicense,
    #[error("asset SHA-256 must contain 64 lowercase hexadecimal characters")]
    InvalidSha256,
}

impl AssetManifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.version.trim().is_empty() {
            return Err(ManifestError::MissingVersion);
        }
        if self.license.trim().is_empty() {
            return Err(ManifestError::MissingLicense);
        }
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
        {
            return Err(ManifestError::InvalidSha256);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> AssetManifest {
        AssetManifest {
            kind: AssetKind::ImageClassifier,
            version: "image-v1".into(),
            license: "Apache-2.0".into(),
            sha256: "a".repeat(64),
        }
    }

    #[test]
    fn valid_manifest_passes() {
        assert_eq!(valid().validate(), Ok(()));
    }

    #[test]
    fn rejects_missing_license() {
        let mut value = valid();
        value.license.clear();

        assert_eq!(value.validate(), Err(ManifestError::MissingLicense));
    }

    #[test]
    fn rejects_non_hex_sha256() {
        let mut value = valid();
        value.sha256 = "z".repeat(64);

        assert_eq!(value.validate(), Err(ManifestError::InvalidSha256));
    }
}
