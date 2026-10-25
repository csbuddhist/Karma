use std::fmt;

use karma_domain::MonitorId;
use thiserror::Error;
use zeroize::Zeroizing;

use crate::preparation::difference_hash;

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
    #[error("frame region lies outside the frame")]
    RegionOutOfBounds,
}

/// A rectangle inside a prepared frame, in prepared-frame pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRegion {
    x: u32,
    y: u32,
    dimensions: FrameDimensions,
}

impl FrameRegion {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Result<Self, FrameError> {
        Ok(Self {
            x,
            y,
            dimensions: FrameDimensions::new(width, height)?,
        })
    }

    pub fn x(self) -> u32 {
        self.x
    }

    pub fn y(self) -> u32 {
        self.y
    }

    pub fn dimensions(self) -> FrameDimensions {
        self.dimensions
    }

    /// The smallest region of a `to`-sized frame that covers this region of a `from`-sized
    /// frame showing the same picture.
    pub fn scaled(self, from: FrameDimensions, to: FrameDimensions) -> Result<Self, FrameError> {
        let start = |offset: u32, source: u32, target: u32| {
            u64::from(offset) * u64::from(target) / u64::from(source)
        };
        let end = |offset: u32, length: u32, source: u32, target: u32| {
            (u64::from(offset) + u64::from(length))
                .checked_mul(u64::from(target))
                .map(|scaled| scaled.div_ceil(u64::from(source)).min(u64::from(target)))
        };
        let right = self
            .x
            .checked_add(self.dimensions.width())
            .ok_or(FrameError::ArithmeticOverflow)?;
        let bottom = self
            .y
            .checked_add(self.dimensions.height())
            .ok_or(FrameError::ArithmeticOverflow)?;
        if right > from.width() || bottom > from.height() {
            return Err(FrameError::RegionOutOfBounds);
        }
        let x = start(self.x, from.width(), to.width());
        let y = start(self.y, from.height(), to.height());
        let right = end(self.x, self.dimensions.width(), from.width(), to.width())
            .ok_or(FrameError::ArithmeticOverflow)?;
        let bottom = end(self.y, self.dimensions.height(), from.height(), to.height())
            .ok_or(FrameError::ArithmeticOverflow)?;
        let narrow = |value: u64| u32::try_from(value).map_err(|_| FrameError::ArithmeticOverflow);
        Self::new(
            narrow(x)?,
            narrow(y)?,
            narrow(right - x)?,
            narrow(bottom - y)?,
        )
    }
}

/// Reads parts of the capture a prepared frame was scaled down from, so that small content can
/// be classified from its own pixels rather than an enlarged copy of the prepared frame.
pub trait FrameDetailSource {
    /// Size of the capture behind the prepared frames, or `None` without one.
    fn capture_dimensions(&self) -> Option<FrameDimensions>;

    /// Reads `region` of the prepared `frame` from the capture.
    fn read(&mut self, frame: &PreparedFrame, region: FrameRegion) -> Option<PreparedFrame>;
}

/// A detail source for frames that have no capture behind them.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoFrameDetail;

impl FrameDetailSource for NoFrameDetail {
    fn capture_dimensions(&self) -> Option<FrameDimensions> {
        None
    }

    fn read(&mut self, _frame: &PreparedFrame, _region: FrameRegion) -> Option<PreparedFrame> {
        None
    }
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
        let pixels = Zeroizing::new(pixels);
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
            pixels,
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

    pub(crate) fn into_parts(mut self) -> (MonitorId, i64, FrameDimensions, usize, Vec<u8>) {
        let pixels = std::mem::take(&mut *self.pixels);
        (
            self.monitor_id,
            self.captured_at_ms,
            self.dimensions,
            self.stride,
            pixels,
        )
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
    pub(crate) fn new(
        monitor_id: MonitorId,
        captured_at_ms: i64,
        dimensions: FrameDimensions,
        pixels: Vec<u8>,
        fingerprint: u64,
    ) -> Self {
        Self {
            monitor_id,
            captured_at_ms,
            dimensions,
            pixels: Zeroizing::new(pixels),
            fingerprint,
        }
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

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// Copies `region` into a frame of its own, keeping the monitor and capture time.
    pub fn crop(&self, region: FrameRegion) -> Result<PreparedFrame, FrameError> {
        let dimensions = region.dimensions();
        let right = region
            .x()
            .checked_add(dimensions.width())
            .ok_or(FrameError::ArithmeticOverflow)?;
        let bottom = region
            .y()
            .checked_add(dimensions.height())
            .ok_or(FrameError::ArithmeticOverflow)?;
        if right > self.dimensions.width() || bottom > self.dimensions.height() {
            return Err(FrameError::RegionOutOfBounds);
        }
        let source_stride = self.dimensions.tight_stride()?;
        let row_bytes = dimensions.tight_stride()?;
        let left = region.x() as usize * BYTES_PER_PIXEL;
        // The exact capacity keeps the copy in one allocation, so no unzeroized
        // reallocation of pixel data is left behind.
        let mut pixels = Vec::with_capacity(dimensions.tight_byte_len()?);
        for row in region.y() as usize..bottom as usize {
            let start = row * source_stride + left;
            pixels.extend_from_slice(&self.pixels[start..start + row_bytes]);
        }
        let fingerprint = difference_hash(&pixels, dimensions)?;
        Ok(Self::new(
            self.monitor_id.clone(),
            self.captured_at_ms,
            dimensions,
            pixels,
            fingerprint,
        ))
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

    fn prepared(width: u32, height: u32) -> PreparedFrame {
        let dimensions = FrameDimensions::new(width, height).unwrap();
        let pixels = (0..width * height)
            .flat_map(|index| [index as u8, (index >> 8) as u8, 0, 255])
            .collect();
        crate::FramePreparer::new(crate::FramePreparationConfig::new(width.max(height)).unwrap())
            .prepare(
                BgraFrame::new(
                    MonitorId("m".into()),
                    9,
                    dimensions,
                    dimensions.tight_stride().unwrap(),
                    pixels,
                )
                .unwrap(),
            )
            .unwrap()
    }

    #[test]
    fn crop_copies_the_region_rows_and_keeps_frame_identity() {
        let frame = prepared(4, 3);

        let cropped = frame.crop(FrameRegion::new(1, 1, 2, 2).unwrap()).unwrap();

        assert_eq!(cropped.dimensions(), FrameDimensions::new(2, 2).unwrap());
        assert_eq!(cropped.monitor_id(), &MonitorId("m".into()));
        assert_eq!(cropped.captured_at_ms(), 9);
        assert_eq!(
            cropped.pixels(),
            &[5, 0, 0, 255, 6, 0, 0, 255, 9, 0, 0, 255, 10, 0, 0, 255]
        );
    }

    #[test]
    fn crop_rejects_regions_outside_the_frame() {
        let frame = prepared(4, 3);

        assert_eq!(
            frame
                .crop(FrameRegion::new(3, 0, 2, 1).unwrap())
                .unwrap_err(),
            FrameError::RegionOutOfBounds
        );
        assert_eq!(
            frame
                .crop(FrameRegion::new(0, 1, 1, 3).unwrap())
                .unwrap_err(),
            FrameError::RegionOutOfBounds
        );
        assert_eq!(
            frame
                .crop(FrameRegion::new(u32::MAX, 0, 1, 1).unwrap())
                .unwrap_err(),
            FrameError::ArithmeticOverflow
        );
    }

    #[test]
    fn scaled_region_covers_the_same_part_of_a_larger_capture() {
        let prepared = FrameDimensions::new(640, 360).unwrap();
        let capture = FrameDimensions::new(1920, 1080).unwrap();

        assert_eq!(
            FrameRegion::new(496, 272, 80, 48)
                .unwrap()
                .scaled(prepared, capture)
                .unwrap(),
            FrameRegion::new(1488, 816, 240, 144).unwrap()
        );
        // Uneven scales round outward and stay inside the capture.
        let capture = FrameDimensions::new(1366, 768).unwrap();
        let region = FrameRegion::new(633, 353, 7, 7)
            .unwrap()
            .scaled(prepared, capture)
            .unwrap();
        assert_eq!((region.x(), region.y()), (1351, 753));
        assert_eq!(region.dimensions(), FrameDimensions::new(15, 15).unwrap());
    }

    #[test]
    fn scaled_region_must_lie_inside_its_frame() {
        let prepared = FrameDimensions::new(640, 360).unwrap();
        let capture = FrameDimensions::new(1920, 1080).unwrap();

        assert_eq!(
            FrameRegion::new(600, 0, 41, 8)
                .unwrap()
                .scaled(prepared, capture)
                .unwrap_err(),
            FrameError::RegionOutOfBounds
        );
        assert_eq!(
            FrameRegion::new(u32::MAX, 0, 1, 1)
                .unwrap()
                .scaled(prepared, capture)
                .unwrap_err(),
            FrameError::ArithmeticOverflow
        );
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
