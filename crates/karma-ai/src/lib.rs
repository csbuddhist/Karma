#![forbid(unsafe_code)]

mod frame;
mod frame_pipeline;
mod image_classifier;
mod image_manifest;
mod image_tensor;
mod mailbox;
mod manifest;
mod observation;
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
    TensorLayout, VIDDEXA_LABELS, VIDDEXA_REPOSITORY, VIDDEXA_REVISION,
};
pub use image_tensor::{ImageTensor, ImageTensorBuilder, ImageTensorError};
pub use mailbox::LatestFrameMailbox;
pub use manifest::{AssetKind, AssetManifest, ManifestError};
pub use observation::{ImageInference, ObservationAssembler, ObservationInput};
pub use preparation::{FramePreparationConfig, FramePreparer};
pub use scheduler::{FrameMetadata, FrameScheduler, FrameWork};
pub use word_pack::{OcrMatchSummary, WordPack, WordPackError, WordRule, WordRuleKind};
