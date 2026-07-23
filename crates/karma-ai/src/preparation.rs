use crate::{BgraFrame, FrameDimensions, FrameError, PreparedFrame};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramePreparationConfig {
    maximum_edge: u32,
}

impl Default for FramePreparationConfig {
    fn default() -> Self {
        Self { maximum_edge: 640 }
    }
}

impl FramePreparationConfig {
    pub fn new(maximum_edge: u32) -> Result<Self, FrameError> {
        if maximum_edge == 0 {
            return Err(FrameError::InvalidMaximumEdge);
        }
        Ok(Self { maximum_edge })
    }

    pub fn target(self, source: FrameDimensions) -> Result<FrameDimensions, FrameError> {
        let width = source.width();
        let height = source.height();
        if width.max(height) <= self.maximum_edge {
            return Ok(source);
        }

        let maximum = u64::from(self.maximum_edge);
        if width >= height {
            let scaled_height =
                (u64::from(height) * maximum + u64::from(width) / 2) / u64::from(width);
            FrameDimensions::new(
                self.maximum_edge,
                u32::try_from(scaled_height.max(1)).map_err(|_| FrameError::ArithmeticOverflow)?,
            )
        } else {
            let scaled_width =
                (u64::from(width) * maximum + u64::from(height) / 2) / u64::from(height);
            FrameDimensions::new(
                u32::try_from(scaled_width.max(1)).map_err(|_| FrameError::ArithmeticOverflow)?,
                self.maximum_edge,
            )
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AxisSample {
    low: usize,
    high: usize,
    high_weight: u64,
    denominator: u64,
}

fn axis_sample(source: u32, target: u32, index: u32) -> AxisSample {
    if source == 1 || target == 1 {
        return AxisSample {
            low: 0,
            high: 0,
            high_weight: 0,
            denominator: 1,
        };
    }

    let denominator = u64::from(target - 1);
    let numerator = u64::from(index) * u64::from(source - 1);
    let low = numerator / denominator;
    AxisSample {
        low: low as usize,
        high: (low + 1).min(u64::from(source - 1)) as usize,
        high_weight: numerator % denominator,
        denominator,
    }
}

fn lerp(first: u8, second: u8, weight: u64, denominator: u64) -> u8 {
    let inverse = denominator - weight;
    ((u64::from(first) * inverse + u64::from(second) * weight + denominator / 2) / denominator)
        as u8
}

fn offset(x: usize, y: usize, stride: usize, channel: usize) -> Result<usize, FrameError> {
    y.checked_mul(stride)
        .and_then(|row| {
            x.checked_mul(4)
                .and_then(|pixel| row.checked_add(pixel))
                .and_then(|pixel| pixel.checked_add(channel))
        })
        .ok_or(FrameError::ArithmeticOverflow)
}

fn sample_channel(
    pixels: &[u8],
    stride: usize,
    x: AxisSample,
    y: AxisSample,
    channel: usize,
) -> Result<u8, FrameError> {
    let top_left = pixels[offset(x.low, y.low, stride, channel)?];
    let top_right = pixels[offset(x.high, y.low, stride, channel)?];
    let bottom_left = pixels[offset(x.low, y.high, stride, channel)?];
    let bottom_right = pixels[offset(x.high, y.high, stride, channel)?];
    let top = lerp(top_left, top_right, x.high_weight, x.denominator);
    let bottom = lerp(bottom_left, bottom_right, x.high_weight, x.denominator);
    Ok(lerp(top, bottom, y.high_weight, y.denominator))
}

fn scale_bgra(input: &BgraFrame, target: FrameDimensions) -> Result<Vec<u8>, FrameError> {
    let source = input.dimensions();
    let target_stride = target.tight_stride()?;
    let mut output = vec![0; target.tight_byte_len()?];

    for target_y in 0..target.height() {
        let y = axis_sample(source.height(), target.height(), target_y);
        for target_x in 0..target.width() {
            let x = axis_sample(source.width(), target.width(), target_x);
            for channel in 0..4 {
                let destination =
                    offset(target_x as usize, target_y as usize, target_stride, channel)?;
                output[destination] =
                    sample_channel(input.pixels(), input.stride(), x, y, channel)?;
            }
        }
    }
    Ok(output)
}

fn difference_hash(pixels: &[u8], dimensions: FrameDimensions) -> Result<u64, FrameError> {
    let stride = dimensions.tight_stride()?;
    let mut hash = 0u64;

    for row in 0..8u32 {
        let y = axis_sample(dimensions.height(), 8, row);
        let mut lumas = [0u16; 9];
        for column in 0..9u32 {
            let x = axis_sample(dimensions.width(), 9, column);
            let blue = u16::from(sample_channel(pixels, stride, x, y, 0)?);
            let green = u16::from(sample_channel(pixels, stride, x, y, 1)?);
            let red = u16::from(sample_channel(pixels, stride, x, y, 2)?);
            lumas[column as usize] = (77 * red + 150 * green + 29 * blue) >> 8;
        }
        for column in 0..8usize {
            if lumas[column] > lumas[column + 1] {
                hash |= 1 << (row * 8 + column as u32);
            }
        }
    }
    Ok(hash)
}

#[derive(Debug, Default)]
pub struct FramePreparer {
    config: FramePreparationConfig,
}

impl FramePreparer {
    pub fn new(config: FramePreparationConfig) -> Self {
        Self { config }
    }

    pub fn prepare(&self, input: BgraFrame) -> Result<PreparedFrame, FrameError> {
        let target = self.config.target(input.dimensions())?;
        let pixels = scale_bgra(&input, target)?;
        let fingerprint = difference_hash(&pixels, target)?;
        Ok(PreparedFrame::new(
            input.monitor_id().clone(),
            input.captured_at_ms(),
            target,
            pixels,
            fingerprint,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BgraFrame, FrameDimensions};
    use karma_domain::MonitorId;

    fn horizontal_gradient(reversed: bool) -> BgraFrame {
        let dimensions = FrameDimensions::new(18, 8).unwrap();
        let mut pixels = Vec::with_capacity(dimensions.tight_byte_len().unwrap());
        for _ in 0..dimensions.height() {
            for x in 0..dimensions.width() {
                let value = if reversed {
                    255 - (x * 15) as u8
                } else {
                    (x * 15) as u8
                };
                pixels.extend_from_slice(&[value, value, value, 255]);
            }
        }
        BgraFrame::new(
            MonitorId("m".into()),
            1,
            dimensions,
            dimensions.tight_stride().unwrap(),
            pixels,
        )
        .unwrap()
    }

    #[test]
    fn target_dimensions_preserve_aspect_and_do_not_enlarge() {
        let config = FramePreparationConfig::default();
        assert_eq!(
            config
                .target(FrameDimensions::new(1920, 1080).unwrap())
                .unwrap(),
            FrameDimensions::new(640, 360).unwrap()
        );
        assert_eq!(
            config
                .target(FrameDimensions::new(1080, 1920).unwrap())
                .unwrap(),
            FrameDimensions::new(360, 640).unwrap()
        );
        assert_eq!(
            config
                .target(FrameDimensions::new(320, 200).unwrap())
                .unwrap(),
            FrameDimensions::new(320, 200).unwrap()
        );
    }

    #[test]
    fn one_pixel_and_padded_rows_are_prepared_tightly() {
        let input = BgraFrame::new(
            MonitorId("m".into()),
            7,
            FrameDimensions::new(1, 1).unwrap(),
            8,
            vec![10, 20, 30, 255, 99, 99, 99, 99],
        )
        .unwrap();
        let output = FramePreparer::default().prepare(input).unwrap();
        assert_eq!(output.dimensions(), FrameDimensions::new(1, 1).unwrap());
        assert_eq!(output.pixels(), &[10, 20, 30, 255]);
    }

    #[test]
    fn bilinear_scaling_interpolates_middle_pixel() {
        let dimensions = FrameDimensions::new(4, 1).unwrap();
        let input = BgraFrame::new(
            MonitorId("m".into()),
            1,
            dimensions,
            dimensions.tight_stride().unwrap(),
            vec![
                0, 0, 0, 255, 40, 40, 40, 255, 80, 80, 80, 255, 120, 120, 120, 255,
            ],
        )
        .unwrap();
        let output = FramePreparer::new(FramePreparationConfig::new(3).unwrap())
            .prepare(input)
            .unwrap();
        assert_eq!(
            output.pixels(),
            &[0, 0, 0, 255, 60, 60, 60, 255, 120, 120, 120, 255]
        );
    }

    #[test]
    fn fingerprint_is_stable_and_changes_with_structure() {
        let first = horizontal_gradient(false);
        let same = horizontal_gradient(false);
        let reversed = horizontal_gradient(true);
        let preparer = FramePreparer::default();
        assert_eq!(
            preparer.prepare(first).unwrap().fingerprint(),
            preparer.prepare(same).unwrap().fingerprint()
        );
        assert_ne!(
            preparer
                .prepare(horizontal_gradient(false))
                .unwrap()
                .fingerprint(),
            preparer.prepare(reversed).unwrap().fingerprint()
        );
    }
}
