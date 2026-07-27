use crate::{OcrMatchSummary, PreparedFrame, WordPack};

pub trait OcrEngine {
    type Error;

    fn classify(
        &mut self,
        frame: &PreparedFrame,
        word_pack: &WordPack,
    ) -> Result<OcrMatchSummary, Self::Error>;
}
