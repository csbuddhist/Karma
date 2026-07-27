use std::{
    fmt,
    fs::File,
    io::{Read, Take},
    path::Path,
    sync::Arc,
};

use karma_ai::{CtcDictionary, OcrBundleManifest, OcrModelProfile, OcrReferenceArtifact};
use sha2::{Digest, Sha256};

use crate::{InferenceError, InferenceErrorKind, MAX_MANIFEST_BYTES};

/// Bound for the required bundled license text. It is deliberately much smaller than model data.
pub const MAX_OCR_LICENSE_BYTES: usize = 64 * 1024;
const MAX_OCR_REFERENCE_BYTES: usize = 256 * 1024 * 1024;

/// Immutable OCR bundle bytes verified before any inference session is created.
pub struct VerifiedOcrBundle {
    manifest: OcrBundleManifest,
    #[allow(dead_code)] // Consumed by the Task 5 OCR session constructor.
    detector_bytes: Arc<[u8]>,
    #[allow(dead_code)] // Consumed by the Task 5 OCR session constructor.
    recognizer_bytes: Arc<[u8]>,
    dictionary_bytes: Arc<[u8]>,
    #[allow(dead_code)] // Consumed by the Task 5 OCR session constructor.
    dictionary: Arc<CtcDictionary>,
    reference_bytes: [Arc<[u8]>; 4],
    _license_bytes: Arc<[u8]>,
}

impl fmt::Debug for VerifiedOcrBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerifiedOcrBundle")
            .field("version", &self.manifest.asset.version)
            .field("profile", &self.manifest.profile)
            .field("dictionary_bytes", &self.dictionary_bytes.len())
            .finish_non_exhaustive()
    }
}

impl VerifiedOcrBundle {
    /// Verifies and retains every bundle asset through a single file handle per asset.
    pub fn load(manifest_path: impl AsRef<Path>) -> Result<Self, InferenceError> {
        let manifest_path = manifest_path.as_ref();
        let manifest_bytes = read_bounded_file(
            manifest_path,
            MAX_MANIFEST_BYTES,
            InferenceErrorKind::OcrContractInvalid,
        )?;
        let manifest: OcrBundleManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|_| InferenceError::new(InferenceErrorKind::OcrContractInvalid))?;
        manifest
            .validate()
            .map_err(|_| InferenceError::new(InferenceErrorKind::OcrContractInvalid))?;
        let directory = manifest_path
            .parent()
            .ok_or_else(|| InferenceError::new(InferenceErrorKind::OcrContractInvalid))?;

        let license_bytes = read_bounded_file(
            &directory.join("LICENSE"),
            MAX_OCR_LICENSE_BYTES,
            InferenceErrorKind::OcrLicenseInvalid,
        )?;
        if license_bytes.is_empty() {
            return Err(InferenceError::new(InferenceErrorKind::OcrLicenseInvalid));
        }
        let detector_bytes = read_expected_file(
            &directory.join(&manifest.detector.file_name),
            manifest.detector.file_bytes,
            &manifest.detector.asset.sha256,
            karma_ai::MAX_OCR_MODEL_BYTES as usize,
            InferenceErrorKind::OcrAssetMissing,
            InferenceErrorKind::OcrAssetHashMismatch,
        )?;
        let recognizer_bytes = read_expected_file(
            &directory.join(&manifest.recognizer.file_name),
            manifest.recognizer.file_bytes,
            &manifest.recognizer.asset.sha256,
            karma_ai::MAX_OCR_MODEL_BYTES as usize,
            InferenceErrorKind::OcrAssetMissing,
            InferenceErrorKind::OcrAssetHashMismatch,
        )?;
        let dictionary_bytes = read_expected_file(
            &directory.join(&manifest.dictionary.file_name),
            manifest.dictionary.file_bytes,
            &manifest.dictionary.asset.sha256,
            karma_ai::MAX_OCR_DICTIONARY_BYTES as usize,
            InferenceErrorKind::OcrAssetMissing,
            InferenceErrorKind::OcrAssetHashMismatch,
        )?;
        let dictionary = parse_dictionary(&dictionary_bytes, &manifest)?;
        let reference_bytes = [
            read_reference_file(directory, &manifest.reference_artifacts.detector_input)?,
            read_reference_file(directory, &manifest.reference_artifacts.detector_output)?,
            read_reference_file(directory, &manifest.reference_artifacts.recognizer_input)?,
            read_reference_file(directory, &manifest.reference_artifacts.recognizer_output)?,
        ];

        Ok(Self {
            manifest,
            detector_bytes,
            recognizer_bytes,
            dictionary_bytes,
            dictionary,
            reference_bytes,
            _license_bytes: license_bytes,
        })
    }

    pub fn manifest(&self) -> &OcrBundleManifest {
        &self.manifest
    }

    pub fn profile(&self) -> OcrModelProfile {
        self.manifest.profile
    }

    #[allow(dead_code)] // Consumed by the Task 5 OCR session constructor.
    pub(crate) fn detector_bytes(&self) -> &[u8] {
        &self.detector_bytes
    }

    #[allow(dead_code)] // Consumed by the Task 5 OCR session constructor.
    pub(crate) fn recognizer_bytes(&self) -> &[u8] {
        &self.recognizer_bytes
    }

    #[allow(dead_code)] // Consumed by the Task 5 OCR session constructor.
    pub(crate) fn dictionary(&self) -> &Arc<CtcDictionary> {
        &self.dictionary
    }

    #[allow(dead_code)] // Consumed by the Task 5 reference verifier.
    pub(crate) fn reference_detector_input_bytes(&self) -> &[u8] {
        &self.reference_bytes[0]
    }

    #[allow(dead_code)] // Consumed by the Task 5 reference verifier.
    pub(crate) fn reference_detector_output_bytes(&self) -> &[u8] {
        &self.reference_bytes[1]
    }

    #[allow(dead_code)] // Consumed by the Task 5 reference verifier.
    pub(crate) fn reference_recognizer_input_bytes(&self) -> &[u8] {
        &self.reference_bytes[2]
    }

    #[allow(dead_code)] // Consumed by the Task 5 reference verifier.
    pub(crate) fn reference_recognizer_output_bytes(&self) -> &[u8] {
        &self.reference_bytes[3]
    }
}

fn read_reference_file(
    directory: &Path,
    artifact: &OcrReferenceArtifact,
) -> Result<Arc<[u8]>, InferenceError> {
    read_expected_file(
        &directory.join(&artifact.path),
        artifact.file_bytes,
        &artifact.sha256,
        MAX_OCR_REFERENCE_BYTES,
        InferenceErrorKind::OcrReferenceInvalid,
        InferenceErrorKind::OcrReferenceInvalid,
    )
}

fn parse_dictionary(
    bytes: &[u8],
    manifest: &OcrBundleManifest,
) -> Result<Arc<CtcDictionary>, InferenceError> {
    let contents = std::str::from_utf8(bytes)
        .map_err(|_| InferenceError::new(InferenceErrorKind::OcrDictionaryInvalid))?;
    if contents
        .lines()
        .ne(manifest.dictionary.entries.iter().map(String::as_str))
    {
        return Err(InferenceError::new(
            InferenceErrorKind::OcrDictionaryInvalid,
        ));
    }
    CtcDictionary::parse(contents, manifest.dictionary.blank_index)
        .map(Arc::new)
        .map_err(|_| InferenceError::new(InferenceErrorKind::OcrDictionaryInvalid))
}

fn read_bounded_file(
    path: &Path,
    maximum_bytes: usize,
    error_kind: InferenceErrorKind,
) -> Result<Arc<[u8]>, InferenceError> {
    let mut file = File::open(path).map_err(|_| InferenceError::new(error_kind))?;
    let metadata = file
        .metadata()
        .map_err(|_| InferenceError::new(error_kind))?;
    if metadata.len() > maximum_bytes as u64 {
        return Err(InferenceError::new(error_kind));
    }
    read_file_bytes(&mut file, maximum_bytes, error_kind)
}

fn read_expected_file(
    path: &Path,
    expected_bytes: u64,
    expected_sha256: &str,
    maximum_bytes: usize,
    missing_kind: InferenceErrorKind,
    invalid_kind: InferenceErrorKind,
) -> Result<Arc<[u8]>, InferenceError> {
    let expected_bytes =
        usize::try_from(expected_bytes).map_err(|_| InferenceError::new(invalid_kind))?;
    if expected_bytes > maximum_bytes {
        return Err(InferenceError::new(invalid_kind));
    }
    let mut file = File::open(path).map_err(|_| InferenceError::new(missing_kind))?;
    let metadata = file
        .metadata()
        .map_err(|_| InferenceError::new(invalid_kind))?;
    if metadata.len() != expected_bytes as u64 {
        return Err(InferenceError::new(invalid_kind));
    }
    let bytes = read_file_bytes(&mut file, expected_bytes, invalid_kind)?;
    if bytes.len() != expected_bytes || format!("{:x}", Sha256::digest(&bytes)) != expected_sha256 {
        return Err(InferenceError::new(invalid_kind));
    }
    Ok(bytes)
}

fn read_file_bytes(
    file: &mut File,
    maximum_bytes: usize,
    error_kind: InferenceErrorKind,
) -> Result<Arc<[u8]>, InferenceError> {
    let read_limit = maximum_bytes
        .checked_add(1)
        .ok_or_else(|| InferenceError::new(error_kind))?;
    let mut bytes = Vec::with_capacity(maximum_bytes);
    let mut limited: Take<&mut File> = file.by_ref().take(read_limit as u64);
    limited
        .read_to_end(&mut bytes)
        .map_err(|_| InferenceError::new(error_kind))?;
    if bytes.len() > maximum_bytes {
        return Err(InferenceError::new(error_kind));
    }
    Ok(bytes.into())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use karma_ai::{
        AssetKind, AssetManifest, ColorOrder, OCR_LIGHTWEIGHT_DETECTOR_MODEL,
        OCR_LIGHTWEIGHT_RECOGNIZER_MODEL, OCR_MANIFEST_FORMAT_VERSION,
        OCR_REFERENCE_DETECTOR_INPUT_PATH, OCR_REFERENCE_DETECTOR_OUTPUT_PATH,
        OCR_REFERENCE_RECOGNIZER_INPUT_PATH, OCR_REFERENCE_RECOGNIZER_OUTPUT_PATH,
        OCR_SOURCE_REPOSITORY, OCR_UPSTREAM_MODEL_HOST, OcrBundleManifest, OcrDictionaryManifest,
        OcrExportToolchain, OcrLanguage, OcrModelAsset, OcrModelProfile, OcrReferenceArtifact,
        OcrReferenceArtifacts, OcrResourceLimits, OcrTensorContract, OcrTensorElementType,
        OcrThresholds, OcrUpstreamAsset, TensorLayout,
    };
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;
    use crate::{InferenceErrorKind, MAX_MANIFEST_BYTES};

    const LICENSE: &[u8] = b"Apache License\nVersion 2.0\n";

    struct BundleFiles {
        detector: Vec<u8>,
        recognizer: Vec<u8>,
        dictionary: Vec<u8>,
        detector_input: Vec<u8>,
        detector_output: Vec<u8>,
        recognizer_input: Vec<u8>,
        recognizer_output: Vec<u8>,
    }

    impl BundleFiles {
        fn valid() -> Self {
            Self {
                detector: b"detector".to_vec(),
                recognizer: b"recognizer".to_vec(),
                dictionary: b"a\nb\n".to_vec(),
                detector_input: b"detector input".to_vec(),
                detector_output: b"detector output".to_vec(),
                recognizer_input: b"recognizer input".to_vec(),
                recognizer_output: b"recognizer output".to_vec(),
            }
        }
    }

    fn asset(kind: AssetKind, version: &str, bytes: &[u8]) -> AssetManifest {
        AssetManifest {
            kind,
            version: version.into(),
            license: "Apache-2.0".into(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }

    fn reference(path: &str, bytes: &[u8]) -> OcrReferenceArtifact {
        OcrReferenceArtifact {
            path: path.into(),
            file_bytes: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }

    fn manifest(files: &BundleFiles) -> OcrBundleManifest {
        OcrBundleManifest {
            format_version: OCR_MANIFEST_FORMAT_VERSION,
            asset: asset(AssetKind::OcrBundle, "pp-ocrv5-mobile-1", b"bundle"),
            profile: OcrModelProfile::Lightweight,
            source_repository: OCR_SOURCE_REPOSITORY.into(),
            source_revision: "0123456789abcdef0123456789abcdef01234567".into(),
            detector: OcrModelAsset {
                asset: asset(AssetKind::OcrDetector, "detector", &files.detector),
                model_name: OCR_LIGHTWEIGHT_DETECTOR_MODEL.into(),
                upstream: OcrUpstreamAsset {
                    download_url: format!(
                        "https://{OCR_UPSTREAM_MODEL_HOST}/ocr/PP-OCRv5_mobile_det_infer.tar"
                    ),
                    file_bytes: 1,
                    sha256: "a".repeat(64),
                },
                file_name: "detector.onnx".into(),
                file_bytes: files.detector.len() as u64,
            },
            recognizer: OcrModelAsset {
                asset: asset(AssetKind::OcrRecognizer, "recognizer", &files.recognizer),
                model_name: OCR_LIGHTWEIGHT_RECOGNIZER_MODEL.into(),
                upstream: OcrUpstreamAsset {
                    download_url: format!(
                        "https://{OCR_UPSTREAM_MODEL_HOST}/ocr/PP-OCRv5_mobile_rec_infer.tar"
                    ),
                    file_bytes: 1,
                    sha256: "b".repeat(64),
                },
                file_name: "recognizer.onnx".into(),
                file_bytes: files.recognizer.len() as u64,
            },
            dictionary: OcrDictionaryManifest {
                asset: asset(AssetKind::OcrDictionary, "dictionary", &files.dictionary),
                file_name: "dictionary.txt".into(),
                file_bytes: files.dictionary.len() as u64,
                entries: vec!["a".into(), "b".into()],
                blank_index: 0,
                languages: vec![
                    OcrLanguage::English,
                    OcrLanguage::ChineseSimplified,
                    OcrLanguage::ChineseTraditional,
                ],
            },
            detector_contract: detector_contract(),
            recognizer_contract: recognizer_contract(),
            thresholds: OcrThresholds::default(),
            resource_limits: ocr_resource_limits(),
            reference_artifacts: OcrReferenceArtifacts {
                detector_input: reference(OCR_REFERENCE_DETECTOR_INPUT_PATH, &files.detector_input),
                detector_output: reference(
                    OCR_REFERENCE_DETECTOR_OUTPUT_PATH,
                    &files.detector_output,
                ),
                recognizer_input: reference(
                    OCR_REFERENCE_RECOGNIZER_INPUT_PATH,
                    &files.recognizer_input,
                ),
                recognizer_output: reference(
                    OCR_REFERENCE_RECOGNIZER_OUTPUT_PATH,
                    &files.recognizer_output,
                ),
            },
            export_toolchain: OcrExportToolchain {
                paddlepaddle_version: "3.0.0".into(),
                paddle2onnx_version: "1.2.11".into(),
                onnx_version: "1.17.0".into(),
                onnx_runtime_version: "1.22.0".into(),
            },
            opset: 18,
            minimum_runtime_version: "1.22".into(),
        }
    }

    fn detector_contract() -> OcrTensorContract {
        OcrTensorContract {
            input_name: "x".into(),
            output_name: "sigmoid_0.tmp_0".into(),
            layout: TensorLayout::Nchw,
            color_order: ColorOrder::Rgb,
            element_type: OcrTensorElementType::F32,
            channels: 3,
            minimum_height: 32,
            maximum_height: 640,
            minimum_width: 32,
            maximum_width: 640,
            dimension_multiple: 32,
            scale: 1.0 / 255.0,
            mean: [0.485, 0.456, 0.406],
            std: [0.229, 0.224, 0.225],
        }
    }

    fn recognizer_contract() -> OcrTensorContract {
        OcrTensorContract {
            input_name: "x".into(),
            output_name: "softmax_0.tmp_0".into(),
            layout: TensorLayout::Nchw,
            color_order: ColorOrder::Rgb,
            element_type: OcrTensorElementType::F32,
            channels: 3,
            minimum_height: 48,
            maximum_height: 48,
            minimum_width: 1,
            maximum_width: 320,
            dimension_multiple: 1,
            scale: 1.0 / 255.0,
            mean: [0.5, 0.5, 0.5],
            std: [0.5, 0.5, 0.5],
        }
    }

    fn ocr_resource_limits() -> OcrResourceLimits {
        OcrResourceLimits {
            maximum_text_boxes: 64,
            minimum_box_side_pixels: 6,
            minimum_box_area_pixels: 48,
            recognizer_height: 48,
            maximum_recognizer_width: 320,
            maximum_batch_size: 8,
            maximum_line_characters: 128,
            maximum_total_characters: 4_096,
        }
    }

    fn write_bundle(
        directory: &TempDir,
        manifest: &OcrBundleManifest,
        files: &BundleFiles,
    ) -> std::path::PathBuf {
        fs::write(directory.path().join("detector.onnx"), &files.detector).unwrap();
        fs::write(directory.path().join("recognizer.onnx"), &files.recognizer).unwrap();
        fs::write(directory.path().join("dictionary.txt"), &files.dictionary).unwrap();
        fs::write(directory.path().join("LICENSE"), LICENSE).unwrap();
        fs::create_dir(directory.path().join("reference")).unwrap();
        fs::write(
            directory.path().join(OCR_REFERENCE_DETECTOR_INPUT_PATH),
            &files.detector_input,
        )
        .unwrap();
        fs::write(
            directory.path().join(OCR_REFERENCE_DETECTOR_OUTPUT_PATH),
            &files.detector_output,
        )
        .unwrap();
        fs::write(
            directory.path().join(OCR_REFERENCE_RECOGNIZER_INPUT_PATH),
            &files.recognizer_input,
        )
        .unwrap();
        fs::write(
            directory.path().join(OCR_REFERENCE_RECOGNIZER_OUTPUT_PATH),
            &files.recognizer_output,
        )
        .unwrap();
        let path = directory.path().join("manifest.json");
        fs::write(&path, serde_json::to_vec(manifest).unwrap()).unwrap();
        path
    }

    #[test]
    fn verifies_and_retains_a_complete_bundle() {
        let directory = TempDir::new().unwrap();
        let files = BundleFiles::valid();
        let path = write_bundle(&directory, &manifest(&files), &files);

        let bundle = VerifiedOcrBundle::load(&path).unwrap();

        assert_eq!(bundle.profile(), OcrModelProfile::Lightweight);
        assert_eq!(bundle.manifest().detector.file_name, "detector.onnx");
    }

    #[test]
    fn missing_declared_asset_has_a_stable_redacted_error() {
        let directory = TempDir::new().unwrap();
        let files = BundleFiles::valid();
        let path = write_bundle(&directory, &manifest(&files), &files);
        fs::remove_file(directory.path().join(OCR_REFERENCE_RECOGNIZER_OUTPUT_PATH)).unwrap();

        let error = VerifiedOcrBundle::load(&path).unwrap_err();

        assert_eq!(error.kind(), InferenceErrorKind::OcrReferenceInvalid);
        assert!(
            !error
                .to_string()
                .contains(directory.path().to_str().unwrap())
        );
    }

    #[test]
    fn every_model_and_dictionary_must_be_present() {
        for file_name in ["detector.onnx", "recognizer.onnx", "dictionary.txt"] {
            let directory = TempDir::new().unwrap();
            let files = BundleFiles::valid();
            let path = write_bundle(&directory, &manifest(&files), &files);
            fs::remove_file(directory.path().join(file_name)).unwrap();

            assert_eq!(
                VerifiedOcrBundle::load(&path).unwrap_err().kind(),
                InferenceErrorKind::OcrAssetMissing
            );
        }
    }

    #[test]
    fn every_reference_artifact_must_be_present() {
        for file_name in [
            OCR_REFERENCE_DETECTOR_INPUT_PATH,
            OCR_REFERENCE_DETECTOR_OUTPUT_PATH,
            OCR_REFERENCE_RECOGNIZER_INPUT_PATH,
            OCR_REFERENCE_RECOGNIZER_OUTPUT_PATH,
        ] {
            let directory = TempDir::new().unwrap();
            let files = BundleFiles::valid();
            let path = write_bundle(&directory, &manifest(&files), &files);
            fs::remove_file(directory.path().join(file_name)).unwrap();

            assert_eq!(
                VerifiedOcrBundle::load(&path).unwrap_err().kind(),
                InferenceErrorKind::OcrReferenceInvalid
            );
        }
    }

    #[test]
    fn declared_assets_reject_wrong_lengths_and_hashes() {
        let directory = TempDir::new().unwrap();
        let files = BundleFiles::valid();
        let mut value = manifest(&files);
        value.reference_artifacts.detector_output.file_bytes += 1;
        let path = write_bundle(&directory, &value, &files);
        assert_eq!(
            VerifiedOcrBundle::load(&path).unwrap_err().kind(),
            InferenceErrorKind::OcrReferenceInvalid
        );

        let directory = TempDir::new().unwrap();
        let mut files = BundleFiles::valid();
        files.dictionary = b"changed\n".to_vec();
        let path = write_bundle(&directory, &manifest(&BundleFiles::valid()), &files);
        assert_eq!(
            VerifiedOcrBundle::load(&path).unwrap_err().kind(),
            InferenceErrorKind::OcrAssetHashMismatch
        );
    }

    #[test]
    fn every_model_dictionary_and_reference_rejects_changed_bytes() {
        for (file_name, expected_kind) in [
            ("detector.onnx", InferenceErrorKind::OcrAssetHashMismatch),
            ("recognizer.onnx", InferenceErrorKind::OcrAssetHashMismatch),
            ("dictionary.txt", InferenceErrorKind::OcrAssetHashMismatch),
            (
                OCR_REFERENCE_DETECTOR_INPUT_PATH,
                InferenceErrorKind::OcrReferenceInvalid,
            ),
            (
                OCR_REFERENCE_DETECTOR_OUTPUT_PATH,
                InferenceErrorKind::OcrReferenceInvalid,
            ),
            (
                OCR_REFERENCE_RECOGNIZER_INPUT_PATH,
                InferenceErrorKind::OcrReferenceInvalid,
            ),
            (
                OCR_REFERENCE_RECOGNIZER_OUTPUT_PATH,
                InferenceErrorKind::OcrReferenceInvalid,
            ),
        ] {
            let directory = TempDir::new().unwrap();
            let files = BundleFiles::valid();
            let path = write_bundle(&directory, &manifest(&files), &files);
            fs::write(directory.path().join(file_name), b"changed").unwrap();

            assert_eq!(
                VerifiedOcrBundle::load(&path).unwrap_err().kind(),
                expected_kind
            );
        }
    }

    #[test]
    fn dictionary_contents_must_match_the_verified_manifest_entries() {
        let directory = TempDir::new().unwrap();
        let mut files = BundleFiles::valid();
        files.dictionary = b"a\nc\n".to_vec();
        let path = write_bundle(&directory, &manifest(&files), &files);

        assert_eq!(
            VerifiedOcrBundle::load(&path).unwrap_err().kind(),
            InferenceErrorKind::OcrDictionaryInvalid
        );
    }

    #[test]
    fn model_and_dictionary_limits_are_enforced_before_allocation() {
        let directory = TempDir::new().unwrap();
        let files = BundleFiles::valid();
        let mut value = manifest(&files);
        value.detector.file_bytes = karma_ai::MAX_OCR_MODEL_BYTES + 1;
        let path = write_bundle(&directory, &value, &files);
        assert_eq!(
            VerifiedOcrBundle::load(&path).unwrap_err().kind(),
            InferenceErrorKind::OcrContractInvalid
        );

        let directory = TempDir::new().unwrap();
        let mut value = manifest(&files);
        value.recognizer.file_bytes = karma_ai::MAX_OCR_MODEL_BYTES + 1;
        let path = write_bundle(&directory, &value, &files);
        assert_eq!(
            VerifiedOcrBundle::load(&path).unwrap_err().kind(),
            InferenceErrorKind::OcrContractInvalid
        );

        let directory = TempDir::new().unwrap();
        let mut value = manifest(&files);
        value.dictionary.file_bytes = karma_ai::MAX_OCR_DICTIONARY_BYTES + 1;
        let path = write_bundle(&directory, &value, &files);
        assert_eq!(
            VerifiedOcrBundle::load(&path).unwrap_err().kind(),
            InferenceErrorKind::OcrContractInvalid
        );
    }

    #[test]
    fn rejects_oversized_manifest_and_unsafe_manifest_names() {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("manifest.json");
        fs::write(&path, vec![b' '; MAX_MANIFEST_BYTES + 1]).unwrap();
        assert_eq!(
            VerifiedOcrBundle::load(&path).unwrap_err().kind(),
            InferenceErrorKind::OcrContractInvalid
        );

        let directory = TempDir::new().unwrap();
        let files = BundleFiles::valid();
        let mut value = manifest(&files);
        value.detector.file_name = "../detector.onnx".into();
        let path = write_bundle(&directory, &value, &files);
        assert_eq!(
            VerifiedOcrBundle::load(&path).unwrap_err().kind(),
            InferenceErrorKind::OcrContractInvalid
        );
    }

    #[test]
    fn missing_or_oversized_license_is_rejected_without_paths() {
        let directory = TempDir::new().unwrap();
        let files = BundleFiles::valid();
        let path = write_bundle(&directory, &manifest(&files), &files);
        fs::remove_file(directory.path().join("LICENSE")).unwrap();
        let error = VerifiedOcrBundle::load(&path).unwrap_err();
        assert_eq!(error.kind(), InferenceErrorKind::OcrLicenseInvalid);
        assert!(
            !error
                .to_string()
                .contains(directory.path().to_str().unwrap())
        );

        let directory = TempDir::new().unwrap();
        let path = write_bundle(&directory, &manifest(&files), &files);
        fs::write(
            directory.path().join("LICENSE"),
            vec![0; MAX_OCR_LICENSE_BYTES + 1],
        )
        .unwrap();
        assert_eq!(
            VerifiedOcrBundle::load(&path).unwrap_err().kind(),
            InferenceErrorKind::OcrLicenseInvalid
        );
    }

    #[test]
    fn empty_license_is_rejected() {
        let directory = TempDir::new().unwrap();
        let files = BundleFiles::valid();
        let path = write_bundle(&directory, &manifest(&files), &files);
        fs::write(directory.path().join("LICENSE"), []).unwrap();

        assert_eq!(
            VerifiedOcrBundle::load(&path).unwrap_err().kind(),
            InferenceErrorKind::OcrLicenseInvalid
        );
    }

    #[test]
    fn loaded_bundle_is_independent_of_later_path_replacement() {
        let directory = TempDir::new().unwrap();
        let files = BundleFiles::valid();
        let path = write_bundle(&directory, &manifest(&files), &files);
        let bundle = VerifiedOcrBundle::load(&path).unwrap();
        fs::write(directory.path().join("detector.onnx"), b"replacement").unwrap();
        fs::write(
            directory.path().join(OCR_REFERENCE_DETECTOR_INPUT_PATH),
            b"replacement",
        )
        .unwrap();

        assert_eq!(bundle.detector_bytes(), files.detector);
        assert_eq!(
            bundle.reference_detector_input_bytes(),
            files.detector_input
        );
    }
}
