#![forbid(unsafe_code)]

mod classifier;
mod db_postprocess;
mod health;
mod model;
mod ocr;
mod ocr_health;
mod ocr_model;

pub use classifier::OnnxImageClassifier;
pub use db_postprocess::{DbPostProcessError, DbPostProcessor};
pub use health::InferenceHealth;
pub use model::{
    InferenceError, InferenceErrorKind, MAX_MANIFEST_BYTES, MAX_MODEL_BYTES, VerifiedImageModel,
};
pub use ocr::OnnxOcrEngine;
pub use ocr_health::OcrInferenceHealth;
pub use ocr_model::{MAX_OCR_LICENSE_BYTES, VerifiedOcrBundle};
