use std::{collections::HashSet, fmt};

use thiserror::Error;
use zeroize::Zeroizing;

use crate::OcrTextBatch;

const MAXIMUM_LINE_CHARACTERS: usize = 128;
const MAXIMUM_TOTAL_CHARACTERS: usize = 4_096;
const MINIMUM_CONFIDENCE: f32 = 0.5;

/// Errors from bounded CTC dictionary parsing and decoding.
///
/// The variants deliberately carry no runtime text, token indices, logits, or probabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CtcError {
    #[error("OCR CTC dictionary is invalid")]
    InvalidDictionary,
    #[error("OCR CTC decoder configuration is invalid")]
    InvalidConfiguration,
    #[error("OCR CTC logits shape is invalid")]
    InvalidShape,
    #[error("OCR CTC logits class count does not match the dictionary")]
    ClassCountMismatch,
    #[error("OCR CTC logits element count is invalid")]
    InvalidLogitCount,
    #[error("OCR CTC tensor arithmetic overflow")]
    ArithmeticOverflow,
    #[error("OCR CTC logits contain a non-finite value")]
    NonFiniteLogits,
}

/// A parsed CTC dictionary whose entries deliberately exclude the CTC blank.
pub struct CtcDictionary {
    entries: Vec<Zeroizing<String>>,
    blank_index: usize,
}

impl CtcDictionary {
    /// Parses newline-delimited dictionary entries and locates the separate CTC blank class.
    pub fn parse(entries: &str, blank_index: usize) -> Result<Self, CtcError> {
        let parsed: Vec<&str> = entries.lines().collect();
        if parsed.is_empty()
            || parsed.iter().any(|entry| entry.is_empty())
            || blank_index > parsed.len()
        {
            return Err(CtcError::InvalidDictionary);
        }

        let unique_entries: HashSet<&str> = parsed.iter().copied().collect();
        if unique_entries.len() != parsed.len() {
            return Err(CtcError::InvalidDictionary);
        }

        Ok(Self {
            entries: parsed
                .into_iter()
                .map(|entry| Zeroizing::new(entry.to_owned()))
                .collect(),
            blank_index,
        })
    }

    fn class_count(&self) -> Result<usize, CtcError> {
        self.entries
            .len()
            .checked_add(1)
            .ok_or(CtcError::ArithmeticOverflow)
    }

    fn entry_for_class(&self, class: usize) -> Option<&str> {
        if class == self.blank_index {
            return None;
        }
        let entry_index = if class < self.blank_index {
            class
        } else {
            class.checked_sub(1)?
        };
        self.entries.get(entry_index).map(|entry| entry.as_str())
    }
}

impl fmt::Debug for CtcDictionary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CtcDictionary")
            .field("entries", &self.entries.len())
            .finish()
    }
}

/// A bounded greedy CTC decoder.
pub struct CtcDecoder {
    dictionary: CtcDictionary,
    minimum_confidence: f32,
    maximum_line_characters: usize,
    maximum_total_characters: usize,
}

impl CtcDecoder {
    /// Builds a decoder with caps that cannot exceed the OCR runtime's fixed budgets.
    pub fn new(
        dictionary: CtcDictionary,
        minimum_confidence: f32,
        maximum_line_characters: usize,
        maximum_total_characters: usize,
    ) -> Result<Self, CtcError> {
        if !minimum_confidence.is_finite()
            || !(0.0..=1.0).contains(&minimum_confidence)
            || maximum_line_characters == 0
            || maximum_total_characters == 0
        {
            return Err(CtcError::InvalidConfiguration);
        }

        Ok(Self {
            dictionary,
            minimum_confidence: minimum_confidence.max(MINIMUM_CONFIDENCE),
            maximum_line_characters: maximum_line_characters.min(MAXIMUM_LINE_CHARACTERS),
            maximum_total_characters: maximum_total_characters.min(MAXIMUM_TOTAL_CHARACTERS),
        })
    }

    /// Decodes a single `[1, time, classes]` greedy CTC line.
    pub fn decode_line(&self, logits: &[f32], shape: &[usize]) -> Result<DecodedLine, CtcError> {
        let [batch, time, classes] = shape else {
            return Err(CtcError::InvalidShape);
        };
        if *batch != 1 {
            return Err(CtcError::InvalidShape);
        }
        self.validate_logits(logits, *batch, *time, *classes)?;
        self.decode_validated_line(logits, *time, *classes, self.maximum_line_characters)
    }

    /// Decodes `[batch, time, classes]` greedy CTC logits, omitting low-confidence lines.
    pub fn decode_batch(&self, logits: &[f32], shape: &[usize]) -> Result<OcrTextBatch, CtcError> {
        let [batch, time, classes] = shape else {
            return Err(CtcError::InvalidShape);
        };
        self.validate_logits(logits, *batch, *time, *classes)?;

        let line_stride = time
            .checked_mul(*classes)
            .ok_or(CtcError::ArithmeticOverflow)?;
        let mut remaining = self.maximum_total_characters;
        let mut lines = Vec::new();
        for batch_index in 0..*batch {
            if remaining == 0 {
                break;
            }
            let offset = batch_index
                .checked_mul(line_stride)
                .ok_or(CtcError::ArithmeticOverflow)?;
            let end = offset
                .checked_add(line_stride)
                .ok_or(CtcError::ArithmeticOverflow)?;
            let line_limit = self.maximum_line_characters.min(remaining);
            let decoded =
                self.decode_validated_line(&logits[offset..end], *time, *classes, line_limit)?;
            if decoded.character_count() > 0 && decoded.confidence() >= self.minimum_confidence {
                remaining = remaining
                    .checked_sub(decoded.character_count())
                    .ok_or(CtcError::ArithmeticOverflow)?;
                lines.push(decoded.into_text());
            }
        }

        OcrTextBatch::from_lines(lines, self.maximum_total_characters)
            .map_err(|_| CtcError::ArithmeticOverflow)
    }

    fn validate_logits(
        &self,
        logits: &[f32],
        batch: usize,
        time: usize,
        classes: usize,
    ) -> Result<(), CtcError> {
        if classes != self.dictionary.class_count()? {
            return Err(CtcError::ClassCountMismatch);
        }
        let expected = batch
            .checked_mul(time)
            .and_then(|elements| elements.checked_mul(classes))
            .ok_or(CtcError::ArithmeticOverflow)?;
        if logits.len() != expected {
            return Err(CtcError::InvalidLogitCount);
        }
        if logits.iter().any(|value| !value.is_finite()) {
            return Err(CtcError::NonFiniteLogits);
        }
        Ok(())
    }

    fn decode_validated_line(
        &self,
        logits: &[f32],
        time: usize,
        classes: usize,
        maximum_characters: usize,
    ) -> Result<DecodedLine, CtcError> {
        let mut text = Zeroizing::new(String::new());
        let mut characters = 0_usize;
        let mut confidence_sum = 0.0_f64;
        let mut emitted = 0_usize;
        let mut previous = None;

        for timestep in 0..time {
            let offset = timestep
                .checked_mul(classes)
                .ok_or(CtcError::ArithmeticOverflow)?;
            let end = offset
                .checked_add(classes)
                .ok_or(CtcError::ArithmeticOverflow)?;
            let (class, probability) = most_likely_class(&logits[offset..end])?;
            if class == self.dictionary.blank_index {
                previous = None;
                continue;
            }
            if previous == Some(class) {
                continue;
            }
            previous = Some(class);

            let entry = self
                .dictionary
                .entry_for_class(class)
                .ok_or(CtcError::ClassCountMismatch)?;
            let entry_characters = entry.chars().count();
            let next_count = characters
                .checked_add(entry_characters)
                .ok_or(CtcError::ArithmeticOverflow)?;
            if next_count > maximum_characters {
                break;
            }
            text.push_str(entry);
            characters = next_count;
            confidence_sum += f64::from(probability);
            emitted = emitted.checked_add(1).ok_or(CtcError::ArithmeticOverflow)?;
        }

        let confidence = if emitted == 0 {
            0.0
        } else {
            (confidence_sum / emitted as f64) as f32
        };
        Ok(DecodedLine {
            text,
            characters,
            confidence,
        })
    }
}

impl fmt::Debug for CtcDecoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CtcDecoder")
    }
}

/// A decoded line held in zeroizing memory.
pub struct DecodedLine {
    text: Zeroizing<String>,
    characters: usize,
    confidence: f32,
}

impl DecodedLine {
    pub fn character_count(&self) -> usize {
        self.characters
    }

    pub fn confidence(&self) -> f32 {
        self.confidence
    }

    fn into_text(mut self) -> String {
        std::mem::take(&mut *self.text)
    }
}

impl fmt::Debug for DecodedLine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DecodedLine")
            .field("characters", &self.characters)
            .finish()
    }
}

fn most_likely_class(logits: &[f32]) -> Result<(usize, f32), CtcError> {
    let (&first, rest) = logits.split_first().ok_or(CtcError::InvalidShape)?;
    let (class, maximum) = rest.iter().copied().enumerate().fold(
        (0_usize, first),
        |(best_class, best_value), (offset, value)| {
            if value > best_value {
                (offset + 1, value)
            } else {
                (best_class, best_value)
            }
        },
    );
    let denominator = logits
        .iter()
        .map(|value| (value - maximum).exp())
        .sum::<f32>();
    let probability = 1.0 / denominator;
    Ok((class, probability))
}
