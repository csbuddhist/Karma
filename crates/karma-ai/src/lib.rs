#![forbid(unsafe_code)]

mod frame;
mod frame_pipeline;
mod image_classifier;
mod image_manifest;
mod image_tensor;
mod mailbox;
mod manifest;
mod observation;
mod ocr_engine;
mod ocr_geometry;
mod ocr_manifest;
mod ocr_tensor;
mod ocr_text;
mod preparation;
mod scheduler;
mod word_pack;

pub use frame::{BgraFrame, FrameDimensions, FrameError, PreparedFrame};
pub use frame_pipeline::{FramePipeline, ScheduledFrame};
pub use image_classifier::{
    ClassifierOutput, ClassifierOutputError, ImageClassifier, ViddexaRiskMapper,
};
pub use image_manifest::{
    ColorOrder, ImageInputContract, ImageManifestError, ImageModelManifest, ModelLabel,
    TensorLayout, VIDDEXA_LABELS, VIDDEXA_MEAN, VIDDEXA_REPOSITORY, VIDDEXA_REVISION,
    VIDDEXA_SCALE, VIDDEXA_STD,
};
pub use image_tensor::{ImageTensor, ImageTensorBuilder, ImageTensorError};
pub use mailbox::LatestFrameMailbox;
pub use manifest::{AssetKind, AssetManifest, ManifestError};
pub use observation::{ImageInference, ObservationAssembler, ObservationInput};
pub use ocr_engine::OcrEngine;
pub use ocr_geometry::{TextQuadrilateral, sort_and_limit_boxes};
pub use ocr_manifest::{
    MAX_OCR_DICTIONARY_BYTES, MAX_OCR_EXPORT_TOOL_VERSION_LENGTH, MAX_OCR_MODEL_BYTES,
    OCR_ACCURATE_DETECTOR_MODEL, OCR_ACCURATE_RECOGNIZER_MODEL, OCR_LIGHTWEIGHT_DETECTOR_MODEL,
    OCR_LIGHTWEIGHT_RECOGNIZER_MODEL, OCR_MANIFEST_FORMAT_VERSION,
    OCR_REFERENCE_DETECTOR_INPUT_PATH, OCR_REFERENCE_DETECTOR_OUTPUT_PATH,
    OCR_REFERENCE_RECOGNIZER_INPUT_PATH, OCR_REFERENCE_RECOGNIZER_OUTPUT_PATH,
    OCR_SOURCE_REPOSITORY, OCR_UPSTREAM_MODEL_HOST, OcrBundleManifest, OcrDictionaryManifest,
    OcrExportToolchain, OcrLanguage, OcrManifestError, OcrModelAsset, OcrModelProfile,
    OcrReferenceArtifact, OcrReferenceArtifacts, OcrResourceLimits, OcrTensorContract,
    OcrTensorElementType, OcrThresholds, OcrUpstreamAsset,
};
pub use ocr_tensor::{
    DetectionTransform, DetectorTensor, DetectorTensorBuilder, OcrTensorError,
    RecognizerTensorBatch, RecognizerTensorBuilder,
};
pub use ocr_text::{OcrTextBatch, OcrTextError};
pub use preparation::{FramePreparationConfig, FramePreparer};
pub use scheduler::{FrameMetadata, FrameScheduler, FrameWork};
pub use word_pack::{OcrMatchSummary, WordPack, WordPackError, WordRule, WordRuleKind};

#[cfg(test)]
mod ocr_contract_tests {
    use super::*;

    fn valid_manifest() -> OcrBundleManifest {
        OcrBundleManifest {
            format_version: OCR_MANIFEST_FORMAT_VERSION,
            asset: AssetManifest {
                kind: AssetKind::OcrBundle,
                version: "pp-ocrv5-mobile-1".into(),
                license: "Apache-2.0".into(),
                sha256: "a".repeat(64),
            },
            profile: OcrModelProfile::Lightweight,
            source_repository: OCR_SOURCE_REPOSITORY.into(),
            source_revision: "0123456789abcdef0123456789abcdef01234567".into(),
            detector: OcrModelAsset {
                asset: AssetManifest {
                    kind: AssetKind::OcrDetector,
                    version: "pp-ocrv5-mobile-det".into(),
                    license: "Apache-2.0".into(),
                    sha256: "b".repeat(64),
                },
                model_name: OCR_LIGHTWEIGHT_DETECTOR_MODEL.into(),
                upstream: OcrUpstreamAsset {
                    download_url: format!(
                        "https://{OCR_UPSTREAM_MODEL_HOST}/ocr/PP-OCRv5_mobile_det_infer.tar"
                    ),
                    file_bytes: 1_024,
                    sha256: "2".repeat(64),
                },
                file_name: "detector.onnx".into(),
                file_bytes: 1_024,
            },
            recognizer: OcrModelAsset {
                asset: AssetManifest {
                    kind: AssetKind::OcrRecognizer,
                    version: "pp-ocrv5-mobile-rec".into(),
                    license: "Apache-2.0".into(),
                    sha256: "c".repeat(64),
                },
                model_name: OCR_LIGHTWEIGHT_RECOGNIZER_MODEL.into(),
                upstream: OcrUpstreamAsset {
                    download_url: format!(
                        "https://{OCR_UPSTREAM_MODEL_HOST}/ocr/PP-OCRv5_mobile_rec_infer.tar"
                    ),
                    file_bytes: 1_024,
                    sha256: "3".repeat(64),
                },
                file_name: "recognizer.onnx".into(),
                file_bytes: 1_024,
            },
            dictionary: OcrDictionaryManifest {
                asset: AssetManifest {
                    kind: AssetKind::OcrDictionary,
                    version: "pp-ocrv5-mobile-dict".into(),
                    license: "Apache-2.0".into(),
                    sha256: "d".repeat(64),
                },
                file_name: "dictionary.txt".into(),
                file_bytes: 64,
                entries: vec!["a".into(), "b".into()],
                blank_index: 0,
                languages: vec![
                    OcrLanguage::English,
                    OcrLanguage::ChineseSimplified,
                    OcrLanguage::ChineseTraditional,
                ],
            },
            detector_contract: OcrTensorContract {
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
            },
            recognizer_contract: OcrTensorContract {
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
            },
            thresholds: OcrThresholds {
                probability: 0.3,
                text_box: 0.6,
                expansion: 1.5,
                recognition_confidence: 0.5,
            },
            resource_limits: OcrResourceLimits {
                maximum_text_boxes: 64,
                minimum_box_side_pixels: 6,
                minimum_box_area_pixels: 48,
                recognizer_height: 48,
                maximum_recognizer_width: 320,
                maximum_batch_size: 8,
                maximum_line_characters: 128,
                maximum_total_characters: 4_096,
            },
            reference_artifacts: OcrReferenceArtifacts {
                detector_input: OcrReferenceArtifact {
                    path: OCR_REFERENCE_DETECTOR_INPUT_PATH.into(),
                    file_bytes: 16,
                    sha256: "e".repeat(64),
                },
                detector_output: OcrReferenceArtifact {
                    path: OCR_REFERENCE_DETECTOR_OUTPUT_PATH.into(),
                    file_bytes: 16,
                    sha256: "f".repeat(64),
                },
                recognizer_input: OcrReferenceArtifact {
                    path: OCR_REFERENCE_RECOGNIZER_INPUT_PATH.into(),
                    file_bytes: 16,
                    sha256: "0".repeat(64),
                },
                recognizer_output: OcrReferenceArtifact {
                    path: OCR_REFERENCE_RECOGNIZER_OUTPUT_PATH.into(),
                    file_bytes: 16,
                    sha256: "1".repeat(64),
                },
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

    #[test]
    fn ocr_manifest_accepts_the_portable_contract() {
        assert_eq!(valid_manifest().validate(), Ok(()));
    }

    #[test]
    fn ocr_contract_exports_provenance_constants() {
        assert_eq!(MAX_OCR_EXPORT_TOOL_VERSION_LENGTH, 128);
        assert_eq!(OCR_MANIFEST_FORMAT_VERSION, 1);
        assert_eq!(
            OCR_SOURCE_REPOSITORY,
            "https://github.com/PaddlePaddle/PaddleOCR"
        );
        assert_eq!(
            OCR_UPSTREAM_MODEL_HOST,
            "paddle-model-ecology.bj.bcebos.com"
        );
    }

    #[test]
    fn ocr_manifest_rejects_untrusted_asset_and_source_metadata() {
        let mut manifest = valid_manifest();
        manifest.asset.license = "MIT".into();
        assert_eq!(manifest.validate(), Err(OcrManifestError::InvalidLicense));

        let mut manifest = valid_manifest();
        manifest.source_revision = "main".into();
        assert_eq!(manifest.validate(), Err(OcrManifestError::InvalidSource));

        let mut manifest = valid_manifest();
        manifest.detector.asset.sha256 = "A".repeat(64);
        assert_eq!(manifest.validate(), Err(OcrManifestError::InvalidAsset));

        let mut manifest = valid_manifest();
        manifest.recognizer.file_bytes = 0;
        assert_eq!(manifest.validate(), Err(OcrManifestError::InvalidAsset));
    }

    #[test]
    fn ocr_manifest_rejects_wrong_file_names_and_paths() {
        let mut manifest = valid_manifest();
        manifest.detector.file_name = "detector-v2.onnx".into();
        assert_eq!(manifest.validate(), Err(OcrManifestError::InvalidAsset));

        let mut manifest = valid_manifest();
        manifest.recognizer.file_name = "../recognizer.onnx".into();
        assert_eq!(manifest.validate(), Err(OcrManifestError::InvalidAsset));

        let mut manifest = valid_manifest();
        manifest.dictionary.file_name = "nested/dictionary.txt".into();
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidDictionary)
        );
    }

    #[test]
    fn ocr_manifest_rejects_invalid_runtime_and_tensor_contracts() {
        let mut manifest = valid_manifest();
        manifest.opset = 17;
        assert_eq!(manifest.validate(), Err(OcrManifestError::InvalidOpset));

        let mut manifest = valid_manifest();
        manifest.minimum_runtime_version = "1.21".into();
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidRuntimeVersion)
        );

        let mut manifest = valid_manifest();
        manifest.detector_contract.maximum_width = 639;
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidDetectorContract)
        );

        let mut manifest = valid_manifest();
        manifest.recognizer_contract.minimum_height = 47;
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidRecognizerContract)
        );

        let mut manifest = valid_manifest();
        manifest.recognizer_contract.std[0] = f32::NAN;
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidRecognizerContract)
        );
    }

    #[test]
    fn ocr_manifest_rejects_invalid_dictionary_thresholds_and_limits() {
        let mut manifest = valid_manifest();
        manifest.dictionary.entries.push("a".into());
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidDictionary)
        );

        let mut manifest = valid_manifest();
        manifest.dictionary.languages = vec![OcrLanguage::English];
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidDictionary)
        );

        let mut manifest = valid_manifest();
        manifest.thresholds.probability = 1.1;
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidThresholds)
        );

        let mut manifest = valid_manifest();
        manifest.thresholds.expansion = f32::INFINITY;
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidThresholds)
        );

        let mut manifest = valid_manifest();
        manifest.resource_limits.maximum_batch_size = 9;
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidResourceLimits)
        );
    }

    #[test]
    fn ocr_manifest_binds_each_profile_to_its_official_model_pair() {
        let mut manifest = valid_manifest();
        manifest.detector.model_name = OCR_ACCURATE_DETECTOR_MODEL.into();
        manifest.detector.upstream.download_url = format!(
            "https://{OCR_UPSTREAM_MODEL_HOST}/ocr/{OCR_ACCURATE_DETECTOR_MODEL}_infer.tar"
        );
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::ModelProfileMismatch)
        );

        let mut manifest = valid_manifest();
        manifest.profile = OcrModelProfile::Accurate;
        manifest.detector.model_name = OCR_ACCURATE_DETECTOR_MODEL.into();
        manifest.recognizer.model_name = OCR_ACCURATE_RECOGNIZER_MODEL.into();
        manifest.detector.upstream.download_url = format!(
            "https://{OCR_UPSTREAM_MODEL_HOST}/ocr/{OCR_ACCURATE_DETECTOR_MODEL}_infer.tar"
        );
        manifest.recognizer.upstream.download_url = format!(
            "https://{OCR_UPSTREAM_MODEL_HOST}/ocr/{OCR_ACCURATE_RECOGNIZER_MODEL}_infer.tar"
        );
        assert_eq!(manifest.validate(), Ok(()));
    }

    #[test]
    fn ocr_manifest_rejects_invalid_reference_artifact_metadata() {
        let mut manifest = valid_manifest();
        manifest.reference_artifacts.detector_input.path = "reference/../detector-input.bin".into();
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidReferenceArtifacts)
        );

        let mut manifest = valid_manifest();
        manifest.reference_artifacts.detector_output.path = "reference/detector-output.bin".into();
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidReferenceArtifacts)
        );

        let mut manifest = valid_manifest();
        manifest.reference_artifacts.recognizer_input.file_bytes = 0;
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidReferenceArtifacts)
        );

        let mut manifest = valid_manifest();
        manifest.reference_artifacts.recognizer_output.sha256 = "A".repeat(64);
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidReferenceArtifacts)
        );
    }

    #[test]
    fn ocr_manifest_rejects_wrong_format_version_or_source_repository() {
        let mut manifest = valid_manifest();
        manifest.format_version = 2;
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidFormatVersion)
        );

        let mut manifest = valid_manifest();
        manifest.source_repository = "https://example.invalid/PaddleOCR".into();
        assert_eq!(manifest.validate(), Err(OcrManifestError::InvalidSource));
    }

    #[test]
    fn ocr_manifest_rejects_invalid_upstream_model_metadata() {
        let mut manifest = valid_manifest();
        manifest.detector.upstream.download_url =
            format!("http://{OCR_UPSTREAM_MODEL_HOST}/ocr/PP-OCRv5_mobile_det_infer.tar");
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidUpstreamAsset)
        );

        let mut manifest = valid_manifest();
        manifest.recognizer.upstream.download_url =
            "https://example.invalid/PP-OCRv5_mobile_rec_infer.tar".into();
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidUpstreamAsset)
        );

        let mut manifest = valid_manifest();
        manifest.detector.upstream.download_url =
            format!("https://user@{OCR_UPSTREAM_MODEL_HOST}/ocr/PP-OCRv5_mobile_det_infer.tar");
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidUpstreamAsset)
        );

        let mut manifest = valid_manifest();
        manifest.detector.upstream.download_url = format!(
            "https://{OCR_UPSTREAM_MODEL_HOST}/ocr/PP-OCRv5_mobile_det_infer.tar?token=secret"
        );
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidUpstreamAsset)
        );

        let mut manifest = valid_manifest();
        manifest.recognizer.upstream.download_url =
            format!("https://{OCR_UPSTREAM_MODEL_HOST}/ocr/PP-OCRv5_mobile_rec_infer.tar#fragment");
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidUpstreamAsset)
        );

        let mut manifest = valid_manifest();
        manifest.detector.upstream.file_bytes = 0;
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidUpstreamAsset)
        );

        let mut manifest = valid_manifest();
        manifest.recognizer.upstream.sha256 = "A".repeat(64);
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidUpstreamAsset)
        );
    }

    #[test]
    fn ocr_manifest_rejects_upstream_archives_that_do_not_match_the_declared_model() {
        let mut manifest = valid_manifest();
        manifest.detector.upstream.download_url = format!(
            "https://{OCR_UPSTREAM_MODEL_HOST}/ocr/{OCR_LIGHTWEIGHT_RECOGNIZER_MODEL}_infer.tar"
        );
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidUpstreamAsset)
        );

        let mut manifest = valid_manifest();
        manifest.recognizer.upstream.download_url = format!(
            "https://{OCR_UPSTREAM_MODEL_HOST}/ocr/{OCR_LIGHTWEIGHT_DETECTOR_MODEL}_infer.tar"
        );
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidUpstreamAsset)
        );

        let mut manifest = valid_manifest();
        manifest.detector.upstream.download_url =
            format!("https://{OCR_UPSTREAM_MODEL_HOST}/ocr/unrelated_infer.tar");
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidUpstreamAsset)
        );
    }

    #[test]
    fn ocr_manifest_rejects_missing_or_invalid_export_tool_versions() {
        let mut manifest = valid_manifest();
        manifest.export_toolchain.paddlepaddle_version.clear();
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidExportToolchain)
        );

        let mut manifest = valid_manifest();
        manifest.export_toolchain.paddle2onnx_version = "1.2.11 beta".into();
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidExportToolchain)
        );

        let mut manifest = valid_manifest();
        manifest.export_toolchain.onnx_version = "1".repeat(129);
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidExportToolchain)
        );

        let mut manifest = valid_manifest();
        manifest.export_toolchain.onnx_runtime_version = "1/22/0".into();
        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidExportToolchain)
        );
    }

    #[test]
    fn ocr_thresholds_default_to_the_documented_values() {
        assert_eq!(
            OcrThresholds::default(),
            OcrThresholds {
                probability: 0.3,
                text_box: 0.6,
                expansion: 1.5,
                recognition_confidence: 0.5,
            }
        );
    }

    #[test]
    fn ocr_manifest_accepts_a_ctc_blank_after_dictionary_entries() {
        let mut manifest = valid_manifest();
        manifest.dictionary.blank_index = manifest.dictionary.entries.len();

        assert_eq!(manifest.validate(), Ok(()));
    }

    #[test]
    fn ocr_manifest_rejects_a_ctc_blank_past_dictionary_entries() {
        let mut manifest = valid_manifest();
        manifest.dictionary.blank_index = manifest.dictionary.entries.len() + 1;

        assert_eq!(
            manifest.validate(),
            Err(OcrManifestError::InvalidDictionary)
        );
    }

    #[test]
    fn ocr_manifest_rejects_unknown_json_fields() {
        let value = serde_json::to_value(valid_manifest()).unwrap();
        let mut object = value.as_object().unwrap().clone();
        object.insert("unexpected".into(), serde_json::Value::Null);

        assert!(
            serde_json::from_value::<OcrBundleManifest>(serde_json::Value::Object(object)).is_err()
        );
    }

    #[test]
    fn ocr_text_debug_and_json_do_not_expose_content() {
        let batch = OcrTextBatch::from_lines(vec!["sensitive fixture phrase".into()], 64).unwrap();
        assert_eq!(
            format!("{batch:?}"),
            "OcrTextBatch { lines: 1, characters: 24 }"
        );
        assert_eq!(batch.line_count(), 1);
        assert_eq!(batch.character_count(), 24);
    }

    #[test]
    fn ocr_text_enforces_the_character_limit() {
        assert_eq!(
            OcrTextBatch::from_lines(vec!["four".into(), "five!".into()], 8),
            Err(OcrTextError::CharacterLimitExceeded)
        );
    }
}
