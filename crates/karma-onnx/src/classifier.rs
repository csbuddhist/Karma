use std::{fmt, time::Instant};

use karma_ai::{
    ClassifierOutput, ImageClassifier, ImageInference, ImageInputContract, ImageTensorBuilder,
    PreparedFrame, ViddexaRiskMapper,
};
use ndarray::{ArrayViewD, IxDyn};
use ort::{
    session::{Session, builder::GraphOptimizationLevel},
    tensor::TensorElementType,
    value::TensorRef,
};

use crate::{InferenceError, InferenceErrorKind, InferenceHealth, VerifiedImageModel};

pub struct OnnxImageClassifier {
    session: Session,
    input: ImageInputContract,
    output_name: String,
    labels: Vec<String>,
    health: InferenceHealth,
}

impl fmt::Debug for OnnxImageClassifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OnnxImageClassifier")
            .field("input_name", &self.input.name)
            .field("output_name", &self.output_name)
            .field("health", &self.health)
            .finish()
    }
}

impl OnnxImageClassifier {
    pub(crate) fn from_model(model: &VerifiedImageModel) -> Result<Self, InferenceError> {
        let session = Session::builder()
            .map_err(|_| InferenceError::new(InferenceErrorKind::RuntimeInitialization))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|_| InferenceError::new(InferenceErrorKind::RuntimeInitialization))?
            .with_intra_threads(1)
            .map_err(|_| InferenceError::new(InferenceErrorKind::RuntimeInitialization))?
            .commit_from_memory(model.model_bytes())
            .map_err(|_| InferenceError::new(InferenceErrorKind::RuntimeInitialization))?;
        validate_session(&session, model)?;
        let mut indexed_labels = model.manifest().labels.clone();
        indexed_labels.sort_by_key(|label| label.index);
        Ok(Self {
            session,
            input: model.manifest().input.clone(),
            output_name: model.manifest().output_name.clone(),
            labels: indexed_labels.into_iter().map(|label| label.name).collect(),
            health: InferenceHealth::default(),
        })
    }

    pub fn health(&self) -> InferenceHealth {
        self.health
    }

    pub fn verify_reference_logits(
        &mut self,
        frame: &PreparedFrame,
        expected: &[f32],
    ) -> Result<(), InferenceError> {
        if expected.len() != self.labels.len() || expected.iter().any(|value| !value.is_finite()) {
            return Err(InferenceError::new(InferenceErrorKind::OutputInvalid));
        }
        let actual = self.run_logits(frame)?;
        if actual
            .iter()
            .zip(expected)
            .any(|(actual, expected)| (*actual - *expected).abs() > 1e-4)
        {
            return Err(InferenceError::new(InferenceErrorKind::OutputInvalid));
        }
        Ok(())
    }

    fn run_logits(&mut self, frame: &PreparedFrame) -> Result<Vec<f32>, InferenceError> {
        let tensor = ImageTensorBuilder::build(frame, &self.input)
            .map_err(|_| InferenceError::new(InferenceErrorKind::InputPreparation))?;
        let view = ArrayViewD::from_shape(IxDyn(&tensor.shape()), tensor.as_slice())
            .map_err(|_| InferenceError::new(InferenceErrorKind::InputPreparation))?;
        let input = TensorRef::from_array_view(view)
            .map_err(|_| InferenceError::new(InferenceErrorKind::InputPreparation))?;
        let outputs = self
            .session
            .run(ort::inputs![self.input.name.as_str() => input])
            .map_err(|_| InferenceError::new(InferenceErrorKind::InferenceFailed))?;
        let output = outputs
            .get(&self.output_name)
            .ok_or_else(|| InferenceError::new(InferenceErrorKind::OutputInvalid))?;
        let (shape, logits) = output
            .try_extract_tensor::<f32>()
            .map_err(|_| InferenceError::new(InferenceErrorKind::OutputInvalid))?;
        if **shape != [1, 5] || logits.len() != self.labels.len() {
            return Err(InferenceError::new(InferenceErrorKind::OutputInvalid));
        }
        if logits.iter().any(|value| !value.is_finite()) {
            return Err(InferenceError::new(InferenceErrorKind::OutputInvalid));
        }
        Ok(logits.to_vec())
    }

    fn classify_inner(&mut self, frame: &PreparedFrame) -> Result<ImageInference, InferenceError> {
        let logits = self.run_logits(frame)?;
        let probabilities = softmax(&logits)?;
        let classified = ClassifierOutput::new(self.labels.clone(), probabilities)
            .map_err(|_| InferenceError::new(InferenceErrorKind::OutputInvalid))?;
        ViddexaRiskMapper::map(&classified)
            .map_err(|_| InferenceError::new(InferenceErrorKind::OutputInvalid))
    }
}

impl ImageClassifier for OnnxImageClassifier {
    type Error = InferenceError;

    fn classify(&mut self, frame: &PreparedFrame) -> Result<ImageInference, Self::Error> {
        let started = Instant::now();
        match self.classify_inner(frame) {
            Ok(inference) => {
                let micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
                self.health.record_success(micros);
                Ok(inference)
            }
            Err(error) => {
                self.health.record_failure();
                Err(error)
            }
        }
    }
}

fn validate_session(session: &Session, model: &VerifiedImageModel) -> Result<(), InferenceError> {
    if session.inputs.len() != 1 || session.outputs.len() != 1 {
        return Err(InferenceError::new(
            InferenceErrorKind::ModelContractMismatch,
        ));
    }
    let input = &session.inputs[0];
    let output = &session.outputs[0];
    let expected_input_shape = model
        .manifest()
        .input
        .shape
        .map(|dimension| dimension as i64);
    if input.name != model.manifest().input.name
        || input.input_type.tensor_type() != Some(TensorElementType::Float32)
        || input
            .input_type
            .tensor_shape()
            .is_none_or(|shape| **shape != expected_input_shape)
        || output.name != model.manifest().output_name
        || output.output_type.tensor_type() != Some(TensorElementType::Float32)
        || output
            .output_type
            .tensor_shape()
            .is_none_or(|shape| **shape != [1, 5])
    {
        return Err(InferenceError::new(
            InferenceErrorKind::ModelContractMismatch,
        ));
    }
    Ok(())
}

fn softmax(logits: &[f32]) -> Result<Vec<f32>, InferenceError> {
    if logits.len() != 5 || logits.iter().any(|value| !value.is_finite()) {
        return Err(InferenceError::new(InferenceErrorKind::OutputInvalid));
    }
    let maximum = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exponentials = logits
        .iter()
        .map(|value| (*value - maximum).exp())
        .collect::<Vec<_>>();
    let total: f32 = exponentials.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return Err(InferenceError::new(InferenceErrorKind::OutputInvalid));
    }
    Ok(exponentials
        .into_iter()
        .map(|value| value / total)
        .collect())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use karma_ai::{
        AssetKind, AssetManifest, BgraFrame, ColorOrder, FrameDimensions, FramePreparationConfig,
        FramePreparer, ImageClassifier, ImageInputContract, ImageModelManifest, ModelLabel,
        TensorLayout, VIDDEXA_LABELS, VIDDEXA_REPOSITORY, VIDDEXA_REVISION,
    };
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use crate::{InferenceErrorKind, VerifiedImageModel};

    fn fixture_bytes() -> Vec<u8> {
        fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/five_class_logits.onnx"),
        )
        .unwrap()
    }

    fn verified_model(input_name: &str) -> (TempDir, VerifiedImageModel) {
        let directory = TempDir::new().unwrap();
        let model = fixture_bytes();
        fs::write(directory.path().join("model.onnx"), &model).unwrap();
        let manifest = ImageModelManifest {
            asset: AssetManifest {
                kind: AssetKind::ImageClassifier,
                version: "fixture-1".into(),
                license: "Apache-2.0".into(),
                sha256: format!("{:x}", Sha256::digest(&model)),
            },
            source_repository: VIDDEXA_REPOSITORY.into(),
            source_revision: VIDDEXA_REVISION.into(),
            file_name: "model.onnx".into(),
            file_bytes: model.len() as u64,
            opset: 18,
            minimum_runtime_version: "1.22".into(),
            input: ImageInputContract {
                name: input_name.into(),
                shape: [1, 3, 224, 224],
                layout: TensorLayout::Nchw,
                color_order: ColorOrder::Rgb,
                scale: 1.0 / 255.0,
                mean: [0.485, 0.456, 0.406],
                std: [0.47853944, 0.4732864, 0.47434163],
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
        };
        let manifest_path = directory.path().join("manifest.json");
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let verified = VerifiedImageModel::load(manifest_path).unwrap();
        (directory, verified)
    }

    fn prepared_frame() -> karma_ai::PreparedFrame {
        let dimensions = FrameDimensions::new(1, 1).unwrap();
        let input = BgraFrame::new(
            karma_domain::MonitorId("display-1".into()),
            10,
            dimensions,
            4,
            vec![0, 0, 0, 255],
        )
        .unwrap();
        FramePreparer::new(FramePreparationConfig::new(1).unwrap())
            .prepare(input)
            .unwrap()
    }

    #[test]
    fn runs_fixture_and_maps_named_softmax_output() {
        let (directory, model) = verified_model("pixel_values");
        fs::write(directory.path().join("model.onnx"), b"replaced").unwrap();
        let mut classifier = model.create_classifier().unwrap();

        let inference = classifier.classify(&prepared_frame()).unwrap();

        assert_eq!(inference.score_millis, 200);
        assert!(inference.categories.is_empty());
        assert_eq!(classifier.health().inferences(), 1);
        assert_eq!(classifier.health().failures(), 0);
    }

    #[test]
    fn rejects_graph_contract_mismatch_before_inference() {
        let (_directory, model) = verified_model("wrong_input");

        assert_eq!(
            model.create_classifier().unwrap_err().kind(),
            InferenceErrorKind::ModelContractMismatch
        );
    }

    #[test]
    fn verifies_reference_logits_without_exposing_runtime_output() {
        let (_directory, model) = verified_model("pixel_values");
        let mut classifier = model.create_classifier().unwrap();

        assert!(
            classifier
                .verify_reference_logits(&prepared_frame(), &[0.0, 1.0, 2.0, 3.0, 4.0])
                .is_ok()
        );
        assert_eq!(
            classifier
                .verify_reference_logits(&prepared_frame(), &[0.0, 1.0, 2.0, 3.0, 4.1])
                .unwrap_err()
                .kind(),
            InferenceErrorKind::OutputInvalid
        );
    }
}
