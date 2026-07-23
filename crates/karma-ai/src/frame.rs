use std::fmt;

use karma_domain::MonitorId;
use thiserror::Error;
use zeroize::Zeroizing;

const BYTES_PER_PIXEL: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameDimensions {
    width: u32,
    height: u32,
}

impl FrameDimensions {
    pub fn new(width: u32, height: u32) -> Result<Self, FrameError> {
        if width == 0 || height == 0 {
            return Err(FrameError::InvalidDimensions);
        }
        Ok(Self { width, height })
    }

    pub fn width(self) -> u32 {
        self.width
    }

    pub fn height(self) -> u32 {
        self.height
    }

    pub fn tight_stride(self) -> Result<usize, FrameError> {
        usize::try_from(self.width)
            .map_err(|_| FrameError::ArithmeticOverflow)?
            .checked_mul(BYTES_PER_PIXEL)
            .ok_or(FrameError::ArithmeticOverflow)
    }

    pub fn tight_byte_len(self) -> Result<usize, FrameError> {
        self.tight_stride()?
            .checked_mul(usize::try_from(self.height).map_err(|_| FrameError::ArithmeticOverflow)?)
            .ok_or(FrameError::ArithmeticOverflow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FrameError {
    #[error("frame dimensions must be non-zero")]
    InvalidDimensions,
    #[error("frame arithmetic overflow")]
    ArithmeticOverflow,
    #[error("frame stride {actual} is smaller than {minimum}")]
    StrideTooSmall { minimum: usize, actual: usize },
    #[error("frame buffer length {actual} does not equal {expected}")]
    BufferLengthMismatch { expected: usize, actual: usize },
    #[error("maximum frame edge must be non-zero")]
    InvalidMaximumEdge,
}

pub struct BgraFrame {
    monitor_id: MonitorId,
    captured_at_ms: i64,
    dimensions: FrameDimensions,
    stride: usize,
    pixels: Zeroizing<Vec<u8>>,
}

impl BgraFrame {
    pub fn new(
        monitor_id: MonitorId,
        captured_at_ms: i64,
        dimensions: FrameDimensions,
        stride: usize,
        pixels: Vec<u8>,
    ) -> Result<Self, FrameError> {
        let minimum = dimensions.tight_stride()?;
        if stride < minimum {
            return Err(FrameError::StrideTooSmall {
                minimum,
                actual: stride,
            });
        }
        let expected = stride
            .checked_mul(
                usize::try_from(dimensions.height()).map_err(|_| FrameError::ArithmeticOverflow)?,
            )
            .ok_or(FrameError::ArithmeticOverflow)?;
        if pixels.len() != expected {
            return Err(FrameError::BufferLengthMismatch {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            monitor_id,
            captured_at_ms,
            dimensions,
            stride,
            pixels: Zeroizing::new(pixels),
        })
    }

    pub fn monitor_id(&self) -> &MonitorId {
        &self.monitor_id
    }

    pub fn captured_at_ms(&self) -> i64 {
        self.captured_at_ms
    }

    pub fn dimensions(&self) -> FrameDimensions {
        self.dimensions
    }

    pub fn stride(&self) -> usize {
        self.stride
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

impl fmt::Debug for BgraFrame {
    fn fmt(&self, value: &mut fmt::Formatter<'_>) -> fmt::Result {
        value
            .debug_struct("BgraFrame")
            .field("monitor_id", &self.monitor_id)
            .field("captured_at_ms", &self.captured_at_ms)
            .field("dimensions", &self.dimensions)
            .field("stride", &self.stride)
            .field("pixel_bytes", &self.pixels.len())
            .finish()
    }
}

pub struct PreparedFrame {
    monitor_id: MonitorId,
    captured_at_ms: i64,
    dimensions: FrameDimensions,
    pixels: Zeroizing<Vec<u8>>,
    fingerprint: u64,
}

impl PreparedFrame {
    pub fn monitor_id(&self) -> &MonitorId {
        &self.monitor_id
    }

    pub fn captured_at_ms(&self) -> i64 {
        self.captured_at_ms
    }

    pub fn dimensions(&self) -> FrameDimensions {
        self.dimensions
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}

impl fmt::Debug for PreparedFrame {
    fn fmt(&self, value: &mut fmt::Formatter<'_>) -> fmt::Result {
        value
            .debug_struct("PreparedFrame")
            .field("monitor_id", &self.monitor_id)
            .field("captured_at_ms", &self.captured_at_ms)
            .field("dimensions", &self.dimensions)
            .field("pixel_bytes", &self.pixels.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use karma_domain::MonitorId;

    #[test]
    fn dimensions_reject_zero_and_compute_tight_layout() {
        assert_eq!(
            FrameDimensions::new(0, 10),
            Err(FrameError::InvalidDimensions)
        );
        let value = FrameDimensions::new(3, 2).unwrap();
        assert_eq!(value.tight_stride().unwrap(), 12);
        assert_eq!(value.tight_byte_len().unwrap(), 24);
    }

    #[test]
    fn bgra_frame_validates_stride_and_buffer_length() {
        let dimensions = FrameDimensions::new(2, 2).unwrap();
        assert!(matches!(
            BgraFrame::new(MonitorId("m".into()), 1, dimensions, 7, vec![0; 14]),
            Err(FrameError::StrideTooSmall { .. })
        ));
        assert!(matches!(
            BgraFrame::new(MonitorId("m".into()), 1, dimensions, 8, vec![0; 15]),
            Err(FrameError::BufferLengthMismatch { .. })
        ));
    }

    #[test]
    fn debug_redacts_owned_pixels() {
        let frame = BgraFrame::new(
            MonitorId("m".into()),
            1,
            FrameDimensions::new(1, 1).unwrap(),
            4,
            vec![11, 22, 33, 255],
        )
        .unwrap();
        let debug = format!("{frame:?}");
        assert!(debug.contains("pixel_bytes: 4"));
        assert!(!debug.contains("11"));
        assert!(!debug.contains("22"));
    }
}
