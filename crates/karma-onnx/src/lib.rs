#![forbid(unsafe_code)]

mod classifier;
mod health;
mod model;
mod ocr_model;

pub use classifier::OnnxImageClassifier;
pub use health::InferenceHealth;
pub use model::{
    InferenceError, InferenceErrorKind, MAX_MANIFEST_BYTES, MAX_MODEL_BYTES, VerifiedImageModel,
};
pub use ocr_model::{MAX_OCR_LICENSE_BYTES, VerifiedOcrBundle};
