use std::{fmt, sync::Arc, time::Instant};

use karma_ai::{
    CtcDecoder, CtcDictionary, DetectionMap, DetectorTensor, DetectorTensorBuilder, OcrEngine,
    OcrMatchSummary, OcrResourceLimits, OcrTensorContract, OcrTensorError, PreparedFrame,
    RecognizerTensorBatch, RecognizerTensorBuilder, WordPack,
};
use karma_domain::OcrRisk;
use ndarray::{ArrayViewD, IxDyn};
use ort::{
    session::{Session, builder::GraphOptimizationLevel},
    tensor::TensorElementType,
    value::TensorRef,
};
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::{
    DbPostProcessor, InferenceError, InferenceErrorKind, OcrInferenceHealth, VerifiedOcrBundle,
};

pub struct OnnxOcrEngine {
    detector: Session,
    recognizer: Session,
    detector_contract: OcrTensorContract,
    recognizer_contract: OcrTensorContract,
    limits: OcrResourceLimits,
    recognition_confidence: f32,
    class_count: usize,
    dictionary: Arc<CtcDictionary>,
    postprocessor: DbPostProcessor,
    health: OcrInferenceHealth,
}

impl fmt::Debug for OnnxOcrEngine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OnnxOcrEngine")
            .field("detector_input", &self.detector_contract.input_name)
            .field("detector_output", &self.detector_contract.output_name)
            .field("recognizer_input", &self.recognizer_contract.input_name)
            .field("recognizer_output", &self.recognizer_contract.output_name)
            .field("health", &self.health)
            .finish()
    }
}

impl OnnxOcrEngine {
    pub(crate) fn from_bundle(bundle: &VerifiedOcrBundle) -> Result<Self, InferenceError> {
        let mut detector = create_session(bundle.detector_bytes())?;
        let mut recognizer = create_session(bundle.recognizer_bytes())?;
        let class_count = bundle
            .manifest()
            .dictionary
            .entries
            .len()
            .checked_add(1)
            .ok_or_else(|| InferenceError::new(InferenceErrorKind::ModelContractMismatch))?;
        validate_detector_session(&detector, &bundle.manifest().detector_contract)?;
        validate_recognizer_session(
            &recognizer,
            &bundle.manifest().recognizer_contract,
            class_count,
        )?;
        verify_references(&mut detector, &mut recognizer, bundle)?;
        let postprocessor = DbPostProcessor::new(
            bundle.manifest().thresholds.clone(),
            bundle.manifest().resource_limits.clone(),
        )
        .map_err(|_| InferenceError::new(InferenceErrorKind::OcrContractInvalid))?;
        Ok(Self {
            detector,
            recognizer,
            detector_contract: bundle.manifest().detector_contract.clone(),
            recognizer_contract: bundle.manifest().recognizer_contract.clone(),
            limits: bundle.manifest().resource_limits.clone(),
            recognition_confidence: bundle.manifest().thresholds.recognition_confidence,
            class_count,
            dictionary: Arc::clone(bundle.dictionary()),
            postprocessor,
            health: OcrInferenceHealth::default(),
        })
    }

    pub fn health(&self) -> OcrInferenceHealth {
        self.health
    }

    fn classify_inner(
        &mut self,
        frame: &PreparedFrame,
        word_pack: &WordPack,
    ) -> Result<ClassifyOutcome, InferenceError> {
        let (detector_tensor, transform) =
            DetectorTensorBuilder::build(frame, &self.detector_contract)
                .map_err(|_| InferenceError::new(InferenceErrorKind::InputPreparation))?;
        let map = run_detector(
            &mut self.detector,
            &self.detector_contract,
            &detector_tensor,
        )?;
        let boxes = self
            .postprocessor
            .extract(&map, transform, frame.dimensions())
            .map_err(|_| InferenceError::new(InferenceErrorKind::OutputInvalid))?;
        drop(map);
        drop(detector_tensor);

        let mut usable_boxes = Vec::with_capacity(boxes.len());
        let mut skipped_boxes = 0_usize;
        for quadrilateral in boxes {
            match RecognizerTensorBuilder::build_batch(
                frame,
                std::slice::from_ref(&quadrilateral),
                &self.recognizer_contract,
                &self.limits,
            ) {
                Ok(probe) => {
                    drop(probe);
                    usable_boxes.push(quadrilateral);
                }
                Err(OcrTensorError::InvalidCoordinate | OcrTensorError::InvalidGeometry) => {
                    skipped_boxes = skipped_boxes.saturating_add(1);
                }
                Err(_) => {
                    return Err(InferenceError::new(InferenceErrorKind::InputPreparation));
                }
            }
        }

        let mut summary = word_pack.classify(&[]);
        let mut characters = 0_usize;
        let mut processed_boxes = 0_usize;
        let mut resource_limit_reached = false;
        for chunk in usable_boxes.chunks(self.limits.maximum_batch_size) {
            let remaining = self
                .limits
                .maximum_total_characters
                .saturating_sub(characters);
            if remaining == 0 {
                resource_limit_reached = processed_boxes < usable_boxes.len();
                break;
            }
            let tensor = RecognizerTensorBuilder::build_batch(
                frame,
                chunk,
                &self.recognizer_contract,
                &self.limits,
            )
            .map_err(|_| InferenceError::new(InferenceErrorKind::InputPreparation))?;
            let output = run_recognizer(
                &mut self.recognizer,
                &self.recognizer_contract,
                &tensor,
                chunk.len(),
                self.class_count,
            )?;
            drop(tensor);
            let decoder = CtcDecoder::from_shared(
                Arc::clone(&self.dictionary),
                self.recognition_confidence,
                self.limits.maximum_line_characters,
                remaining,
            )
            .map_err(|_| InferenceError::new(InferenceErrorKind::OcrContractInvalid))?;
            let batch = decoder
                .decode_batch(&output.values, &output.shape)
                .map_err(|_| InferenceError::new(InferenceErrorKind::OutputInvalid))?;
            characters = characters
                .checked_add(batch.character_count())
                .ok_or_else(|| InferenceError::new(InferenceErrorKind::OutputInvalid))?;
            merge_summary(&mut summary, word_pack.classify_batch(&batch));
            drop(batch);
            drop(output);
            processed_boxes = processed_boxes.saturating_add(chunk.len());
        }
        if characters >= self.limits.maximum_total_characters
            && processed_boxes < usable_boxes.len()
        {
            resource_limit_reached = true;
        }
        Ok(ClassifyOutcome {
            summary,
            skipped_boxes,
            resource_limit_reached,
        })
    }
}

impl OcrEngine for OnnxOcrEngine {
    type Error = InferenceError;

    fn classify(
        &mut self,
        frame: &PreparedFrame,
        word_pack: &WordPack,
    ) -> Result<OcrMatchSummary, Self::Error> {
        let started = Instant::now();
        match self.classify_inner(frame, word_pack) {
            Ok(outcome) => {
                let elapsed = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
                self.health.record_success(
                    elapsed,
                    outcome.skipped_boxes,
                    outcome.resource_limit_reached,
                );
                Ok(outcome.summary)
            }
            Err(error) => {
                self.health.record_failure();
                Err(error)
            }
        }
    }
}

struct ClassifyOutcome {
    summary: OcrMatchSummary,
    skipped_boxes: usize,
    resource_limit_reached: bool,
}

struct SessionOutput {
    shape: Vec<usize>,
    values: Zeroizing<Vec<f32>>,
}

struct ReferenceTensor {
    shape: Vec<usize>,
    values: Zeroizing<Vec<f32>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceOutput {
    shape: Vec<usize>,
    values: Vec<f32>,
}

fn create_session(bytes: &[u8]) -> Result<Session, InferenceError> {
    Session::builder()
        .map_err(|_| InferenceError::new(InferenceErrorKind::RuntimeInitialization))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|_| InferenceError::new(InferenceErrorKind::RuntimeInitialization))?
        .with_intra_threads(1)
        .map_err(|_| InferenceError::new(InferenceErrorKind::RuntimeInitialization))?
        .commit_from_memory(bytes)
        .map_err(|_| InferenceError::new(InferenceErrorKind::RuntimeInitialization))
}

fn validate_detector_session(
    session: &Session,
    contract: &OcrTensorContract,
) -> Result<(), InferenceError> {
    validate_single_io(session, contract)?;
    let input_shape = session.inputs[0]
        .input_type
        .tensor_shape()
        .ok_or_else(contract_mismatch)?;
    let output_shape = session.outputs[0]
        .output_type
        .tensor_shape()
        .ok_or_else(contract_mismatch)?;
    if **input_shape != [1, 3, -1, -1] || **output_shape != [1, 1, -1, -1] {
        return Err(contract_mismatch());
    }
    Ok(())
}

fn validate_recognizer_session(
    session: &Session,
    contract: &OcrTensorContract,
    class_count: usize,
) -> Result<(), InferenceError> {
    validate_single_io(session, contract)?;
    let input_shape = session.inputs[0]
        .input_type
        .tensor_shape()
        .ok_or_else(contract_mismatch)?;
    let output_shape = session.outputs[0]
        .output_type
        .tensor_shape()
        .ok_or_else(contract_mismatch)?;
    let class_count = i64::try_from(class_count).map_err(|_| contract_mismatch())?;
    if **input_shape != [-1, 3, 48, -1]
        || output_shape.len() != 3
        || output_shape[0] != -1
        || output_shape[1] <= 0
        || output_shape[2] != class_count
    {
        return Err(contract_mismatch());
    }
    Ok(())
}

fn validate_single_io(
    session: &Session,
    contract: &OcrTensorContract,
) -> Result<(), InferenceError> {
    if session.inputs.len() != 1
        || session.outputs.len() != 1
        || session.inputs[0].name != contract.input_name
        || session.outputs[0].name != contract.output_name
        || session.inputs[0].input_type.tensor_type() != Some(TensorElementType::Float32)
        || session.outputs[0].output_type.tensor_type() != Some(TensorElementType::Float32)
    {
        return Err(contract_mismatch());
    }
    Ok(())
}

fn contract_mismatch() -> InferenceError {
    InferenceError::new(InferenceErrorKind::ModelContractMismatch)
}

fn run_detector(
    session: &mut Session,
    contract: &OcrTensorContract,
    tensor: &DetectorTensor,
) -> Result<DetectionMap, InferenceError> {
    let shape = tensor.shape();
    let output = run_session(
        session,
        &contract.input_name,
        &contract.output_name,
        &shape,
        tensor.as_slice(),
        InferenceErrorKind::InferenceFailed,
        InferenceErrorKind::OutputInvalid,
    )?;
    if output.shape != [1, 1, shape[2], shape[3]] {
        return Err(InferenceError::new(InferenceErrorKind::OutputInvalid));
    }
    DetectionMap::from_values(shape[3], shape[2], output.values.to_vec())
        .map_err(|_| InferenceError::new(InferenceErrorKind::OutputInvalid))
}

fn run_recognizer(
    session: &mut Session,
    contract: &OcrTensorContract,
    tensor: &RecognizerTensorBatch,
    batch: usize,
    class_count: usize,
) -> Result<SessionOutput, InferenceError> {
    let shape = tensor.shape();
    let output = run_session(
        session,
        &contract.input_name,
        &contract.output_name,
        &shape,
        tensor.as_slice(),
        InferenceErrorKind::InferenceFailed,
        InferenceErrorKind::OutputInvalid,
    )?;
    if output.shape.len() != 3
        || output.shape[0] != batch
        || output.shape[1] == 0
        || output.shape[2] != class_count
    {
        return Err(InferenceError::new(InferenceErrorKind::OutputInvalid));
    }
    Ok(output)
}

fn run_session(
    session: &mut Session,
    input_name: &str,
    output_name: &str,
    shape: &[usize],
    values: &[f32],
    run_error: InferenceErrorKind,
    output_error: InferenceErrorKind,
) -> Result<SessionOutput, InferenceError> {
    let view =
        ArrayViewD::from_shape(IxDyn(shape), values).map_err(|_| InferenceError::new(run_error))?;
    let input = TensorRef::from_array_view(view).map_err(|_| InferenceError::new(run_error))?;
    let outputs = session
        .run(ort::inputs![input_name => input])
        .map_err(|_| InferenceError::new(run_error))?;
    let output = outputs
        .get(output_name)
        .ok_or_else(|| InferenceError::new(output_error))?;
    let (runtime_shape, runtime_values) = output
        .try_extract_tensor::<f32>()
        .map_err(|_| InferenceError::new(output_error))?;
    let shape = runtime_shape
        .iter()
        .map(|dimension| usize::try_from(*dimension))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| InferenceError::new(output_error))?;
    if runtime_values.iter().any(|value| !value.is_finite()) {
        return Err(InferenceError::new(output_error));
    }
    Ok(SessionOutput {
        shape,
        values: Zeroizing::new(runtime_values.to_vec()),
    })
}

fn verify_references(
    detector: &mut Session,
    recognizer: &mut Session,
    bundle: &VerifiedOcrBundle,
) -> Result<(), InferenceError> {
    let detector_reference = parse_reference_tensor(bundle.reference_detector_input_bytes())?;
    let [
        detector_batch,
        detector_channels,
        detector_height,
        detector_width,
    ] = detector_reference.shape.as_slice()
    else {
        return Err(reference_invalid());
    };
    if *detector_batch != 1
        || *detector_channels != 3
        || !(32..=640).contains(detector_height)
        || !(32..=640).contains(detector_width)
        || detector_height % 32 != 0
        || detector_width % 32 != 0
    {
        return Err(reference_invalid());
    }
    let detector_actual = run_session(
        detector,
        &bundle.manifest().detector_contract.input_name,
        &bundle.manifest().detector_contract.output_name,
        &detector_reference.shape,
        &detector_reference.values,
        InferenceErrorKind::OcrReferenceInvalid,
        InferenceErrorKind::OcrReferenceInvalid,
    )?;
    if detector_actual.shape != [1, 1, *detector_height, *detector_width] {
        return Err(reference_invalid());
    }
    let probability_map = DetectionMap::from_values(
        *detector_width,
        *detector_height,
        detector_actual.values.to_vec(),
    )
    .map_err(|_| reference_invalid())?;
    drop(probability_map);
    compare_reference(detector_actual, bundle.reference_detector_output_bytes())?;
    drop(detector_reference);

    let recognizer_reference = parse_reference_tensor(bundle.reference_recognizer_input_bytes())?;
    let [
        recognizer_batch,
        recognizer_channels,
        recognizer_height,
        recognizer_width,
    ] = recognizer_reference.shape.as_slice()
    else {
        return Err(reference_invalid());
    };
    if *recognizer_batch != 1
        || *recognizer_channels != 3
        || *recognizer_height != 48
        || !(1..=320).contains(recognizer_width)
    {
        return Err(reference_invalid());
    }
    let recognizer_actual = run_session(
        recognizer,
        &bundle.manifest().recognizer_contract.input_name,
        &bundle.manifest().recognizer_contract.output_name,
        &recognizer_reference.shape,
        &recognizer_reference.values,
        InferenceErrorKind::OcrReferenceInvalid,
        InferenceErrorKind::OcrReferenceInvalid,
    )?;
    compare_reference(
        recognizer_actual,
        bundle.reference_recognizer_output_bytes(),
    )
}

/// Parses `KOR1`, rank, dimensions, then little-endian `f32` tensor values.
fn parse_reference_tensor(bytes: &[u8]) -> Result<ReferenceTensor, InferenceError> {
    if bytes.len() < 8 || &bytes[..4] != b"KOR1" {
        return Err(reference_invalid());
    }
    let rank =
        u32::from_le_bytes(bytes[4..8].try_into().map_err(|_| reference_invalid())?) as usize;
    if rank == 0 || rank > 8 {
        return Err(reference_invalid());
    }
    let dimensions_bytes = rank.checked_mul(4).ok_or_else(reference_invalid)?;
    let header_bytes = 8_usize
        .checked_add(dimensions_bytes)
        .ok_or_else(reference_invalid)?;
    if bytes.len() < header_bytes {
        return Err(reference_invalid());
    }
    let shape = bytes[8..header_bytes]
        .chunks_exact(4)
        .map(|chunk| {
            usize::try_from(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .map_err(|_| reference_invalid())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if shape.iter().any(|dimension| *dimension == 0) {
        return Err(reference_invalid());
    }
    let elements = shape.iter().try_fold(1_usize, |total, dimension| {
        total.checked_mul(*dimension).ok_or_else(reference_invalid)
    })?;
    let payload_bytes = elements.checked_mul(4).ok_or_else(reference_invalid)?;
    if header_bytes.checked_add(payload_bytes) != Some(bytes.len()) {
        return Err(reference_invalid());
    }
    let values = bytes[header_bytes..]
        .chunks_exact(size_of::<f32>())
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect::<Vec<_>>();
    if values.iter().any(|value| !value.is_finite()) {
        return Err(reference_invalid());
    }
    Ok(ReferenceTensor {
        shape,
        values: Zeroizing::new(values),
    })
}

fn compare_reference(actual: SessionOutput, expected_bytes: &[u8]) -> Result<(), InferenceError> {
    let expected: ReferenceOutput =
        serde_json::from_slice(expected_bytes).map_err(|_| reference_invalid())?;
    let expected_values = Zeroizing::new(expected.values);
    if actual.shape != expected.shape
        || actual.values.len() != expected_values.len()
        || actual
            .values
            .iter()
            .zip(expected_values.iter())
            .any(|(actual, expected)| !expected.is_finite() || (*actual - *expected).abs() > 1.0e-4)
    {
        return Err(reference_invalid());
    }
    Ok(())
}

fn reference_invalid() -> InferenceError {
    InferenceError::new(InferenceErrorKind::OcrReferenceInvalid)
}

fn merge_summary(summary: &mut OcrMatchSummary, next: OcrMatchSummary) {
    if risk_rank(next.risk) > risk_rank(summary.risk) {
        summary.risk = next.risk;
    }
    summary.categories.extend(next.categories);
    summary.categories.sort();
    summary.categories.dedup();
    summary.exemption_context |= next.exemption_context;
}

fn risk_rank(risk: OcrRisk) -> u8 {
    match risk {
        OcrRisk::None => 0,
        OcrRisk::Keyword => 1,
        OcrRisk::HighRiskPhrase => 2,
    }
}
