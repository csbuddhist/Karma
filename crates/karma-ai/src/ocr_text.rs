use std::fmt;

use thiserror::Error;
use zeroize::Zeroizing;

/// OCR text is deliberately not serializable.
///
/// ```compile_fail
/// use karma_ai::OcrTextBatch;
///
/// fn decoded_batch() -> OcrTextBatch { loop {} }
/// let batch = decoded_batch();
/// let _ = serde_json::to_string(&batch);
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct OcrTextBatch {
    lines: Vec<Zeroizing<String>>,
    characters: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OcrTextError {
    #[error("OCR text exceeds its character limit")]
    CharacterLimitExceeded,
}

impl OcrTextBatch {
    pub(crate) fn from_zeroizing_lines(
        lines: Vec<Zeroizing<String>>,
        maximum_characters: usize,
    ) -> Result<Self, OcrTextError> {
        let characters = lines.iter().try_fold(0_usize, |total, line| {
            total
                .checked_add(line.chars().count())
                .filter(|count| *count <= maximum_characters)
                .ok_or(OcrTextError::CharacterLimitExceeded)
        })?;

        Ok(Self { lines, characters })
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn character_count(&self) -> usize {
        self.characters
    }

    #[allow(dead_code)] // Consumed by the portable OCR decoder added in a later runtime task.
    pub(crate) fn line_refs(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(|line| line.as_str())
    }
}

impl fmt::Debug for OcrTextBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OcrTextBatch")
            .field("lines", &self.line_count())
            .field("characters", &self.character_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_private_constructor_keeps_zeroizing_line_ownership() {
        let batch = OcrTextBatch::from_zeroizing_lines(
            vec![Zeroizing::new("sensitive fixture".to_owned())],
            64,
        )
        .unwrap();

        assert_eq!(batch.line_refs().collect::<Vec<_>>(), ["sensitive fixture"]);
    }
}
