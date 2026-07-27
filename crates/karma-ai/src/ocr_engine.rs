use crate::{OcrMatchSummary, PreparedFrame, WordPack};

pub trait OcrEngine {
    type Error;

    fn classify(
        &mut self,
        frame: &PreparedFrame,
        word_pack: &WordPack,
    ) -> Result<OcrMatchSummary, Self::Error>;

    /// Returns the cumulative number of resource-limit events observed by this engine.
    fn resource_limit_events(&self) -> u64;
}
