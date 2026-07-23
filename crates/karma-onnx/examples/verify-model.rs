use std::{env, fs, path::PathBuf, process::ExitCode};

use karma_ai::{BgraFrame, FrameDimensions, FramePreparer, PreparedFrame};
use karma_domain::MonitorId;
use karma_onnx::{InferenceErrorKind, MAX_MANIFEST_BYTES, VerifiedImageModel};

fn main() -> ExitCode {
    let Some(manifest_path) = env::args_os().nth(1) else {
        eprintln!("status=unavailable component=image_inference error=manifest_invalid");
        return ExitCode::FAILURE;
    };
    match verify(PathBuf::from(manifest_path)) {
        Ok((version, file_bytes)) => {
            println!("status=verified version={} bytes={}", version, file_bytes);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!(
                "status=unavailable component=image_inference error={}",
                error
            );
            ExitCode::FAILURE
        }
    }
}

fn verify(manifest_path: PathBuf) -> Result<(String, u64), InferenceErrorKind> {
    let reference = reference_logits(&manifest_path)?;
    let model = VerifiedImageModel::load(&manifest_path).map_err(|error| error.kind())?;
    let mut classifier = model.create_classifier().map_err(|error| error.kind())?;
    let frame = reference_frame().expect("fixed reference frame must be valid");
    classifier
        .verify_reference_logits(&frame, &reference)
        .map_err(|error| error.kind())?;
    Ok((
        model.manifest().asset.version.clone(),
        model.manifest().file_bytes,
    ))
}

fn reference_logits(manifest_path: &std::path::Path) -> Result<Vec<f32>, InferenceErrorKind> {
    let path = manifest_path
        .parent()
        .ok_or(InferenceErrorKind::OutputInvalid)?
        .join("reference-output.json");
    let bytes = fs::read(path).map_err(|_| InferenceErrorKind::OutputInvalid)?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(InferenceErrorKind::OutputInvalid);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| InferenceErrorKind::OutputInvalid)?;
    let rows = value
        .get("logits")
        .and_then(serde_json::Value::as_array)
        .filter(|rows| rows.len() == 1)
        .ok_or(InferenceErrorKind::OutputInvalid)?;
    rows[0]
        .as_array()
        .filter(|values| values.len() == 5)
        .ok_or(InferenceErrorKind::OutputInvalid)?
        .iter()
        .map(|value| {
            value
                .as_f64()
                .map(|value| value as f32)
                .filter(|value| value.is_finite())
                .ok_or(InferenceErrorKind::OutputInvalid)
        })
        .collect()
}

fn reference_frame() -> Option<PreparedFrame> {
    let dimensions = FrameDimensions::new(224, 224).ok()?;
    let mut pixels = Vec::with_capacity(dimensions.tight_byte_len().ok()?);
    for index in 0..(224usize * 224) {
        let value = (index % 256) as u8;
        pixels.extend_from_slice(&[value, value, value, 255]);
    }
    let frame = BgraFrame::new(
        MonitorId("model-verification".into()),
        0,
        dimensions,
        dimensions.tight_stride().ok()?,
        pixels,
    )
    .ok()?;
    FramePreparer::default().prepare(frame).ok()
}
