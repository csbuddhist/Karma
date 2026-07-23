#![forbid(unsafe_code)]

mod frame;
mod frame_pipeline;
mod image_manifest;
mod mailbox;
mod manifest;
mod observation;
mod preparation;
mod scheduler;
mod word_pack;

pub use frame::{BgraFrame, FrameDimensions, FrameError, PreparedFrame};
pub use frame_pipeline::{FramePipeline, ScheduledFrame};
pub use image_manifest::{
    ColorOrder, ImageInputContract, ImageManifestError, ImageModelManifest, ModelLabel,
    TensorLayout, VIDDEXA_LABELS, VIDDEXA_REPOSITORY, VIDDEXA_REVISION,
};
pub use mailbox::LatestFrameMailbox;
pub use manifest::{AssetKind, AssetManifest, ManifestError};
pub use observation::{ImageInference, ObservationAssembler, ObservationInput};
pub use preparation::{FramePreparationConfig, FramePreparer};
pub use scheduler::{FrameMetadata, FrameScheduler, FrameWork};
pub use word_pack::{OcrMatchSummary, WordPack, WordPackError, WordRule, WordRuleKind};
