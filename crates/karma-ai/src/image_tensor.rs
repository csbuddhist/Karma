use std::fmt;

use thiserror::Error;
use zeroize::Zeroizing;

use crate::{ColorOrder, ImageInputContract, PreparedFrame, TensorLayout};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImageTensorError {
    #[error("image tensor contract is invalid")]
    InvalidContract,
    #[error("image tensor size overflow")]
    ArithmeticOverflow,
    #[error("prepared frame buffer is invalid")]
    InvalidFrameBuffer,
}

pub struct ImageTensor {
    shape: [usize; 4],
    values: Zeroizing<Vec<f32>>,
}

impl ImageTensor {
    pub fn shape(&self) -> [usize; 4] {
        self.shape
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.values
    }
}

impl fmt::Debug for ImageTensor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageTensor")
            .field("shape", &self.shape)
            .field("elements", &self.values.len())
            .finish()
    }
}

pub struct ImageTensorBuilder;

impl ImageTensorBuilder {
    pub fn build(
        frame: &PreparedFrame,
        contract: &ImageInputContract,
    ) -> Result<ImageTensor, ImageTensorError> {
        validate_contract(contract)?;
        let target_height = contract.shape[2];
        let target_width = contract.shape[3];
        let plane_len = target_width
            .checked_mul(target_height)
            .ok_or(ImageTensorError::ArithmeticOverflow)?;
        let element_count = plane_len
            .checked_mul(3)
            .ok_or(ImageTensorError::ArithmeticOverflow)?;
        let source = frame.dimensions();
        let source_stride = source
            .tight_stride()
            .map_err(|_| ImageTensorError::ArithmeticOverflow)?;
        let expected = source
            .tight_byte_len()
            .map_err(|_| ImageTensorError::ArithmeticOverflow)?;
        if frame.pixels().len() != expected {
            return Err(ImageTensorError::InvalidFrameBuffer);
        }

        let mut values = Zeroizing::new(vec![0.0; element_count]);
        for target_y in 0..target_height {
            let y = axis_sample(source.height() as usize, target_height, target_y);
            for target_x in 0..target_width {
                let x = axis_sample(source.width() as usize, target_width, target_x);
                let destination = target_y
                    .checked_mul(target_width)
                    .and_then(|row| row.checked_add(target_x))
                    .ok_or(ImageTensorError::ArithmeticOverflow)?;
                for channel in 0..3 {
                    let bgra_channel = 2 - channel;
                    let pixel = sample_channel(frame.pixels(), source_stride, x, y, bgra_channel)?;
                    values[channel * plane_len + destination] =
                        (pixel * contract.scale - contract.mean[channel]) / contract.std[channel];
                }
            }
        }

        Ok(ImageTensor {
            shape: contract.shape,
            values,
        })
    }
}

fn validate_contract(contract: &ImageInputContract) -> Result<(), ImageTensorError> {
    if contract.shape[0] != 1
        || contract.shape[1] != 3
        || contract.shape[2] == 0
        || contract.shape[3] == 0
        || contract.layout != TensorLayout::Nchw
        || contract.color_order != ColorOrder::Rgb
        || !contract.scale.is_finite()
        || contract.scale <= 0.0
        || contract.mean.iter().any(|value| !value.is_finite())
        || contract
            .std
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(ImageTensorError::InvalidContract);
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct AxisSample {
    low: usize,
    high: usize,
    high_weight: f32,
}

fn axis_sample(source: usize, target: usize, index: usize) -> AxisSample {
    if source == 1 || target == 1 {
        return AxisSample {
            low: 0,
            high: 0,
            high_weight: 0.0,
        };
    }
    let position = index as f32 * (source - 1) as f32 / (target - 1) as f32;
    let low = position.floor() as usize;
    AxisSample {
        low,
        high: (low + 1).min(source - 1),
        high_weight: position - low as f32,
    }
}

fn sample_channel(
    pixels: &[u8],
    stride: usize,
    x: AxisSample,
    y: AxisSample,
    channel: usize,
) -> Result<f32, ImageTensorError> {
    let sample = |sample_x: usize, sample_y: usize| {
        sample_y
            .checked_mul(stride)
            .and_then(|row| {
                sample_x
                    .checked_mul(4)
                    .and_then(|pixel| row.checked_add(pixel))
            })
            .and_then(|pixel| pixel.checked_add(channel))
            .and_then(|offset| pixels.get(offset).copied())
            .map(f32::from)
            .ok_or(ImageTensorError::InvalidFrameBuffer)
    };
    let top_left = sample(x.low, y.low)?;
    let top_right = sample(x.high, y.low)?;
    let bottom_left = sample(x.low, y.high)?;
    let bottom_right = sample(x.high, y.high)?;
    let top = top_left + (top_right - top_left) * x.high_weight;
    let bottom = bottom_left + (bottom_right - bottom_left) * x.high_weight;
    Ok(top + (bottom - top) * y.high_weight)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BgraFrame, ColorOrder, FrameDimensions, FramePreparationConfig, FramePreparer,
        ImageInputContract, TensorLayout,
    };
    use karma_domain::MonitorId;

    fn contract(width: usize, height: usize) -> ImageInputContract {
        ImageInputContract {
            name: "pixel_values".into(),
            shape: [1, 3, height, width],
            layout: TensorLayout::Nchw,
            color_order: ColorOrder::Rgb,
            scale: 1.0 / 255.0,
            mean: [0.0; 3],
            std: [1.0; 3],
        }
    }

    fn prepared(width: u32, height: u32, pixels: Vec<u8>) -> crate::PreparedFrame {
        let dimensions = FrameDimensions::new(width, height).unwrap();
        let input = BgraFrame::new(
            MonitorId("display-1".into()),
            10,
            dimensions,
            dimensions.tight_stride().unwrap(),
            pixels,
        )
        .unwrap();
        FramePreparer::new(FramePreparationConfig::new(width.max(height)).unwrap())
            .prepare(input)
            .unwrap()
    }

    #[test]
    fn converts_bgra_to_normalized_nchw_rgb() {
        let frame = prepared(2, 1, vec![0, 0, 255, 255, 255, 0, 0, 255]);

        let tensor = ImageTensorBuilder::build(&frame, &contract(2, 1)).unwrap();

        assert_eq!(tensor.shape(), [1, 3, 1, 2]);
        assert_eq!(tensor.as_slice(), &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn bilinear_resize_interpolates_all_channels() {
        let frame = prepared(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255]);

        let tensor = ImageTensorBuilder::build(&frame, &contract(3, 1)).unwrap();

        assert_eq!(
            tensor.as_slice(),
            &[0.0, 0.5, 1.0, 0.0, 0.5, 1.0, 0.0, 0.5, 1.0]
        );
    }

    #[test]
    fn tensor_debug_redacts_values() {
        let frame = prepared(1, 1, vec![11, 22, 33, 255]);
        let tensor = ImageTensorBuilder::build(&frame, &contract(1, 1)).unwrap();

        let debug = format!("{tensor:?}");
        assert!(debug.contains("elements: 3"));
        assert!(!debug.contains("0.129"));
        assert!(!debug.contains("0.086"));
    }
}
