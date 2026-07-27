use std::{fs, path::PathBuf};

use karma_ai::{
    AssetKind, AssetManifest, BgraFrame, ColorOrder, FrameDimensions, FramePreparationConfig,
    FramePreparer, OCR_LIGHTWEIGHT_DETECTOR_MODEL, OCR_LIGHTWEIGHT_RECOGNIZER_MODEL,
    OCR_MANIFEST_FORMAT_VERSION, OCR_REFERENCE_DETECTOR_INPUT_PATH,
    OCR_REFERENCE_DETECTOR_OUTPUT_PATH, OCR_REFERENCE_RECOGNIZER_INPUT_PATH,
    OCR_REFERENCE_RECOGNIZER_OUTPUT_PATH, OCR_SOURCE_REPOSITORY, OCR_UPSTREAM_MODEL_HOST,
    OcrBundleManifest, OcrDictionaryManifest, OcrEngine, OcrExportToolchain, OcrLanguage,
    OcrModelAsset, OcrModelProfile, OcrReferenceArtifact, OcrReferenceArtifacts, OcrResourceLimits,
    OcrTensorContract, OcrTensorElementType, OcrThresholds, OcrUpstreamAsset, TensorLayout,
    WordPack, WordRule,
};
use karma_domain::{MonitorId, OcrRisk};
use karma_onnx::{InferenceErrorKind, VerifiedOcrBundle};
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const LICENSE: &[u8] = b"Apache License\nVersion 2.0\n";
const DICTIONARY: &[u8] = b"fixture\nsafe\n";

struct Bundle {
    directory: TempDir,
    manifest: OcrBundleManifest,
    detector_output: Vec<f32>,
    recognizer_output: Vec<f32>,
}

impl Bundle {
    fn fixture() -> Self {
        let directory = TempDir::new().unwrap();
        let detector = fixture_bytes("ocr_detector.onnx");
        let recognizer = fixture_bytes("ocr_recognizer.onnx");
        let detector_input = vec![0.0; 3 * 32 * 32];
        let detector_output = vec![0.0; 32 * 32];
        let recognizer_input = vec![0.0; 3 * 48 * 8];
        let recognizer_output = vec![0.0, 10.0, 0.0, 10.0, 0.0, 0.0];
        let detector_input_bytes = tensor_bytes(&[1, 3, 32, 32], &detector_input);
        let detector_output_bytes = output_json(&[1, 1, 32, 32], &detector_output);
        let recognizer_input_bytes = tensor_bytes(&[1, 3, 48, 8], &recognizer_input);
        let recognizer_output_bytes = output_json(&[1, 2, 3], &recognizer_output);
        let manifest = manifest(
            &detector,
            &recognizer,
            &detector_input_bytes,
            &detector_output_bytes,
            &recognizer_input_bytes,
            &recognizer_output_bytes,
        );
        write(directory.path().join("detector.onnx"), &detector);
        write(directory.path().join("recognizer.onnx"), &recognizer);
        write(directory.path().join("dictionary.txt"), DICTIONARY);
        write(directory.path().join("LICENSE"), LICENSE);
        write_reference(
            &directory,
            OCR_REFERENCE_DETECTOR_INPUT_PATH,
            &detector_input_bytes,
        );
        write_reference(
            &directory,
            OCR_REFERENCE_DETECTOR_OUTPUT_PATH,
            &detector_output_bytes,
        );
        write_reference(
            &directory,
            OCR_REFERENCE_RECOGNIZER_INPUT_PATH,
            &recognizer_input_bytes,
        );
        write_reference(
            &directory,
            OCR_REFERENCE_RECOGNIZER_OUTPUT_PATH,
            &recognizer_output_bytes,
        );
        Self {
            directory,
            manifest,
            detector_output,
            recognizer_output,
        }
    }

    fn load(&self) -> VerifiedOcrBundle {
        let path = self.directory.path().join("manifest.json");
        write(&path, &serde_json::to_vec(&self.manifest).unwrap());
        VerifiedOcrBundle::load(path).unwrap()
    }

    fn replace_detector_output(&mut self) {
        let bytes = output_json(&[1, 1, 32, 32], &self.detector_output);
        write_reference(&self.directory, OCR_REFERENCE_DETECTOR_OUTPUT_PATH, &bytes);
        self.manifest.reference_artifacts.detector_output =
            reference(OCR_REFERENCE_DETECTOR_OUTPUT_PATH, &bytes);
    }

    fn replace_detector_reference_with_out_of_range_probabilities(&mut self) {
        let input = tensor_bytes(&[1, 3, 32, 32], &vec![1.1; 3 * 32 * 32]);
        let output = output_json(&[1, 1, 32, 32], &vec![1.1; 32 * 32]);
        write_reference(&self.directory, OCR_REFERENCE_DETECTOR_INPUT_PATH, &input);
        write_reference(&self.directory, OCR_REFERENCE_DETECTOR_OUTPUT_PATH, &output);
        self.manifest.reference_artifacts.detector_input =
            reference(OCR_REFERENCE_DETECTOR_INPUT_PATH, &input);
        self.manifest.reference_artifacts.detector_output =
            reference(OCR_REFERENCE_DETECTOR_OUTPUT_PATH, &output);
    }

    fn replace_detector_reference_shape(&mut self, height: usize, width: usize) {
        let input = tensor_bytes(&[1, 3, height, width], &vec![0.0; 3 * height * width]);
        let output = output_json(&[1, 1, height, width], &vec![0.0; height * width]);
        write_reference(&self.directory, OCR_REFERENCE_DETECTOR_INPUT_PATH, &input);
        write_reference(&self.directory, OCR_REFERENCE_DETECTOR_OUTPUT_PATH, &output);
        self.manifest.reference_artifacts.detector_input =
            reference(OCR_REFERENCE_DETECTOR_INPUT_PATH, &input);
        self.manifest.reference_artifacts.detector_output =
            reference(OCR_REFERENCE_DETECTOR_OUTPUT_PATH, &output);
    }

    fn replace_recognizer_output(&mut self) {
        let bytes = output_json(&[1, 2, 3], &self.recognizer_output);
        write_reference(
            &self.directory,
            OCR_REFERENCE_RECOGNIZER_OUTPUT_PATH,
            &bytes,
        );
        self.manifest.reference_artifacts.recognizer_output =
            reference(OCR_REFERENCE_RECOGNIZER_OUTPUT_PATH, &bytes);
    }

    fn replace_dictionary(&mut self, entries: [String; 2]) {
        let bytes = format!("{}\n{}\n", entries[0], entries[1]).into_bytes();
        write(self.directory.path().join("dictionary.txt"), &bytes);
        self.manifest.dictionary.asset =
            asset(AssetKind::OcrDictionary, "fixture-dictionary-1", &bytes);
        self.manifest.dictionary.file_bytes = bytes.len() as u64;
        self.manifest.dictionary.entries = entries.into();
    }
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name),
    )
    .unwrap()
}

fn write(path: impl AsRef<std::path::Path>, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
}

fn write_reference(directory: &TempDir, relative: &str, bytes: &[u8]) {
    let path = directory.path().join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    write(path, bytes);
}

fn tensor_bytes(shape: &[usize], values: &[f32]) -> Vec<u8> {
    let mut bytes = b"KOR1".to_vec();
    bytes.extend_from_slice(&(shape.len() as u32).to_le_bytes());
    for dimension in shape {
        bytes.extend_from_slice(&u32::try_from(*dimension).unwrap().to_le_bytes());
    }
    bytes.extend(values.iter().flat_map(|value| value.to_le_bytes()));
    bytes
}

fn output_json(shape: &[usize], values: &[f32]) -> Vec<u8> {
    serde_json::to_vec(&json!({ "shape": shape, "values": values })).unwrap()
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

fn model_asset(
    kind: AssetKind,
    version: &str,
    model_name: &str,
    file_name: &str,
    bytes: &[u8],
) -> OcrModelAsset {
    OcrModelAsset {
        asset: asset(kind, version, bytes),
        model_name: model_name.into(),
        upstream: OcrUpstreamAsset {
            download_url: format!("https://{OCR_UPSTREAM_MODEL_HOST}/ocr/{version}.tar"),
            file_bytes: 1,
            sha256: "a".repeat(64),
        },
        file_name: file_name.into(),
        file_bytes: bytes.len() as u64,
    }
}

fn limits() -> OcrResourceLimits {
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

fn manifest(
    detector: &[u8],
    recognizer: &[u8],
    detector_input: &[u8],
    detector_output: &[u8],
    recognizer_input: &[u8],
    recognizer_output: &[u8],
) -> OcrBundleManifest {
    OcrBundleManifest {
        format_version: OCR_MANIFEST_FORMAT_VERSION,
        asset: asset(AssetKind::OcrBundle, "fixture-bundle-1", b"fixture-bundle"),
        profile: OcrModelProfile::Lightweight,
        source_repository: OCR_SOURCE_REPOSITORY.into(),
        source_revision: "0123456789abcdef0123456789abcdef01234567".into(),
        detector: model_asset(
            AssetKind::OcrDetector,
            "PP-OCRv5_mobile_det_infer",
            OCR_LIGHTWEIGHT_DETECTOR_MODEL,
            "detector.onnx",
            detector,
        ),
        recognizer: model_asset(
            AssetKind::OcrRecognizer,
            "PP-OCRv5_mobile_rec_infer",
            OCR_LIGHTWEIGHT_RECOGNIZER_MODEL,
            "recognizer.onnx",
            recognizer,
        ),
        dictionary: OcrDictionaryManifest {
            asset: asset(AssetKind::OcrDictionary, "fixture-dictionary-1", DICTIONARY),
            file_name: "dictionary.txt".into(),
            file_bytes: DICTIONARY.len() as u64,
            entries: vec!["fixture".into(), "safe".into()],
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
            mean: [0.0; 3],
            std: [1.0; 3],
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
            mean: [0.5; 3],
            std: [0.5; 3],
        },
        thresholds: OcrThresholds::default(),
        resource_limits: limits(),
        reference_artifacts: OcrReferenceArtifacts {
            detector_input: reference(OCR_REFERENCE_DETECTOR_INPUT_PATH, detector_input),
            detector_output: reference(OCR_REFERENCE_DETECTOR_OUTPUT_PATH, detector_output),
            recognizer_input: reference(OCR_REFERENCE_RECOGNIZER_INPUT_PATH, recognizer_input),
            recognizer_output: reference(OCR_REFERENCE_RECOGNIZER_OUTPUT_PATH, recognizer_output),
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

fn prepared_frame() -> karma_ai::PreparedFrame {
    let dimensions = FrameDimensions::new(64, 64).unwrap();
    let mut pixels = Vec::with_capacity(dimensions.tight_byte_len().unwrap());
    for y in 0..64 {
        for x in 0..64 {
            let value = if (8..56).contains(&x) && (20..36).contains(&y) {
                255
            } else {
                0
            };
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
    }
    let frame = BgraFrame::new(
        MonitorId("fixture-display".into()),
        1,
        dimensions,
        dimensions.tight_stride().unwrap(),
        pixels,
    )
    .unwrap();
    FramePreparer::new(FramePreparationConfig::new(64).unwrap())
        .prepare(frame)
        .unwrap()
}

fn prepared_region_grid(columns: u32, rows: u32) -> karma_ai::PreparedFrame {
    let dimensions = FrameDimensions::new(224, 160).unwrap();
    let mut pixels = Vec::with_capacity(dimensions.tight_byte_len().unwrap());
    for y in 0..dimensions.height() {
        for x in 0..dimensions.width() {
            let active = (0..rows).any(|row| {
                (0..columns).any(|column| {
                    let left = 4 + column * 26;
                    let top = 4 + row * 28;
                    (left..left + 10).contains(&x) && (top..top + 10).contains(&y)
                })
            });
            let value = if active { 255 } else { 0 };
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
    }
    let frame = BgraFrame::new(
        MonitorId("fixture-display".into()),
        1,
        dimensions,
        dimensions.tight_stride().unwrap(),
        pixels,
    )
    .unwrap();
    FramePreparer::new(FramePreparationConfig::new(224).unwrap())
        .prepare(frame)
        .unwrap()
}

#[test]
fn validates_dynamic_f32_session_contracts_and_reference_outputs() {
    let fixture = Bundle::fixture();
    let bundle = fixture.load();

    let engine = bundle.create_engine().unwrap();

    assert_eq!(engine.health().inferences(), 0);

    let mut non_square = Bundle::fixture();
    non_square.replace_detector_reference_shape(32, 64);
    assert!(non_square.load().create_engine().is_ok());
}

#[test]
fn rejects_names_class_counts_and_reference_mismatches_before_inference() {
    let mut wrong_name = Bundle::fixture();
    wrong_name.manifest.detector_contract.input_name = "wrong_input".into();
    assert_eq!(
        wrong_name.load().create_engine().unwrap_err().kind(),
        InferenceErrorKind::ModelContractMismatch
    );

    let mut wrong_classes = Bundle::fixture();
    wrong_classes
        .manifest
        .dictionary
        .entries
        .push("extra".into());
    let dictionary = b"fixture\nsafe\nextra\n";
    write(
        wrong_classes.directory.path().join("dictionary.txt"),
        dictionary,
    );
    wrong_classes.manifest.dictionary.asset =
        asset(AssetKind::OcrDictionary, "fixture-dictionary-1", dictionary);
    wrong_classes.manifest.dictionary.file_bytes = dictionary.len() as u64;
    assert_eq!(
        wrong_classes.load().create_engine().unwrap_err().kind(),
        InferenceErrorKind::ModelContractMismatch
    );

    let mut wrong_detector_reference = Bundle::fixture();
    wrong_detector_reference.detector_output[0] = 1.0;
    wrong_detector_reference.replace_detector_output();
    assert_eq!(
        wrong_detector_reference
            .load()
            .create_engine()
            .unwrap_err()
            .kind(),
        InferenceErrorKind::OcrReferenceInvalid
    );

    let mut wrong_recognizer_reference = Bundle::fixture();
    wrong_recognizer_reference.recognizer_output[0] = 1.0;
    wrong_recognizer_reference.replace_recognizer_output();
    assert_eq!(
        wrong_recognizer_reference
            .load()
            .create_engine()
            .unwrap_err()
            .kind(),
        InferenceErrorKind::OcrReferenceInvalid
    );

    let mut out_of_range_detector_reference = Bundle::fixture();
    out_of_range_detector_reference.replace_detector_reference_with_out_of_range_probabilities();
    assert_eq!(
        out_of_range_detector_reference
            .load()
            .create_engine()
            .unwrap_err()
            .kind(),
        InferenceErrorKind::OcrReferenceInvalid
    );
}

#[test]
fn classifies_end_to_end_and_exposes_only_a_summary_and_redacted_health() {
    let fixture = Bundle::fixture();
    let bundle = fixture.load();
    let mut engine = bundle.create_engine().unwrap();
    let pack = WordPack::compile(vec![WordRule::literal(
        "fixture_category",
        "fixture",
        OcrRisk::Keyword,
    )])
    .unwrap();

    let summary = engine.classify(&prepared_frame(), &pack).unwrap();

    assert_eq!(summary.risk, OcrRisk::Keyword);
    assert_eq!(summary.categories, vec!["fixture_category"]);
    assert!(!summary.exemption_context);
    assert_eq!(engine.health().inferences(), 1);
    assert_eq!(engine.health().failures(), 0);
    let debug = format!("{:?}", engine.health());
    assert!(!debug.contains("fixture"));
    assert!(!format!("{engine:?}").contains("fixture"));
}

#[test]
fn processes_batches_of_eight_and_stops_at_the_frame_character_budget() {
    let mut fixture = Bundle::fixture();
    fixture.replace_dictionary(["x".repeat(128), "safe".into()]);
    let bundle = fixture.load();
    let mut engine = bundle.create_engine().unwrap();
    let pack = WordPack::compile(vec![WordRule::literal(
        "bounded_category",
        "xxxx",
        OcrRisk::Keyword,
    )])
    .unwrap();

    let summary = engine.classify(&prepared_region_grid(8, 5), &pack).unwrap();

    assert_eq!(summary.risk, OcrRisk::Keyword);
    assert_eq!(engine.health().resource_limit_events(), 1);
    assert_eq!(engine.health().failures(), 0);
}
