#![forbid(unsafe_code)]

mod frame;
mod manifest;
mod observation;
mod preparation;
mod scheduler;
mod word_pack;

pub use frame::{BgraFrame, FrameDimensions, FrameError, PreparedFrame};
pub use manifest::{AssetKind, AssetManifest, ManifestError};
pub use observation::{ImageInference, ObservationAssembler, ObservationInput};
pub use preparation::{FramePreparationConfig, FramePreparer};
pub use scheduler::{FrameMetadata, FrameScheduler, FrameWork};
pub use word_pack::{OcrMatchSummary, WordPack, WordPackError, WordRule, WordRuleKind};
