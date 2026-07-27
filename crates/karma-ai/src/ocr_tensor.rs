use std::fmt;

use thiserror::Error;
use zeroize::Zeroizing;

use crate::{
    ColorOrder, OcrResourceLimits, OcrTensorContract, OcrTensorElementType, PreparedFrame,
    TensorLayout, TextQuadrilateral,
};

const DETECTOR_MAXIMUM_EDGE: usize = 640;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum OcrTensorError {
    #[error("OCR tensor contract is invalid")]
    InvalidContract,
    #[error("OCR tensor dimensions are invalid")]
    InvalidDimensions,
    #[error("OCR tensor arithmetic overflow")]
    ArithmeticOverflow,
    #[error("prepared frame buffer is invalid")]
    InvalidFrameBuffer,
    #[error("OCR coordinate is invalid")]
    InvalidCoordinate,
    #[error("OCR quadrilateral geometry is invalid")]
    InvalidGeometry,
    #[error("OCR recognizer batch exceeds the configured limit")]
    BatchLimitExceeded,
}

pub struct DetectorTensor {
    shape: [usize; 4],
    values: Zeroizing<Vec<f32>>,
}

impl DetectorTensor {
    pub fn shape(&self) -> [usize; 4] {
        self.shape
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.values
    }
}

impl fmt::Debug for DetectorTensor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DetectorTensor")
            .field("shape", &self.shape)
            .field("elements", &self.values.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectionTransform {
    scale_x: f32,
    scale_y: f32,
    content_width: usize,
    content_height: usize,
    frame_width: usize,
    frame_height: usize,
}

impl DetectionTransform {
    pub fn map_to_frame(self, x: f32, y: f32) -> Result<[f32; 2], OcrTensorError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(OcrTensorError::InvalidCoordinate);
        }
        let mapped_x = (x / self.scale_x).clamp(0.0, (self.frame_width - 1) as f32);
        let mapped_y = (y / self.scale_y).clamp(0.0, (self.frame_height - 1) as f32);
        Ok([mapped_x, mapped_y])
    }

    pub fn content_dimensions(self) -> [usize; 2] {
        [self.content_width, self.content_height]
    }
}

pub struct DetectorTensorBuilder;

impl DetectorTensorBuilder {
    pub fn build(
        frame: &PreparedFrame,
        contract: &OcrTensorContract,
    ) -> Result<(DetectorTensor, DetectionTransform), OcrTensorError> {
        validate_detector_contract(contract)?;
        let dimensions = frame.dimensions();
        let source_width =
            usize::try_from(dimensions.width()).map_err(|_| OcrTensorError::ArithmeticOverflow)?;
        let source_height =
            usize::try_from(dimensions.height()).map_err(|_| OcrTensorError::ArithmeticOverflow)?;
        let expected_frame_len = dimensions
            .tight_byte_len()
            .map_err(|_| OcrTensorError::ArithmeticOverflow)?;
        if frame.pixels().len() != expected_frame_len {
            return Err(OcrTensorError::InvalidFrameBuffer);
        }

        let longest = source_width.max(source_height);
        let resize_scale = if longest > DETECTOR_MAXIMUM_EDGE {
            DETECTOR_MAXIMUM_EDGE as f32 / longest as f32
        } else {
            1.0
        };
        let content_width = scaled_dimension(source_width, resize_scale)?;
        let content_height = scaled_dimension(source_height, resize_scale)?;
        let target_width = padded_dimension(content_width, contract.minimum_width)?;
        let target_height = padded_dimension(content_height, contract.minimum_height)?;
        if target_width > contract.maximum_width
            || target_height > contract.maximum_height
            || target_width > DETECTOR_MAXIMUM_EDGE
            || target_height > DETECTOR_MAXIMUM_EDGE
            || target_width % contract.dimension_multiple != 0
            || target_height % contract.dimension_multiple != 0
        {
            return Err(OcrTensorError::InvalidDimensions);
        }
        let element_count = checked_tensor_elements(target_width, target_height)?;
        let plane_len = target_width
            .checked_mul(target_height)
            .ok_or(OcrTensorError::ArithmeticOverflow)?;
        let stride = dimensions
            .tight_stride()
            .map_err(|_| OcrTensorError::ArithmeticOverflow)?;
        let mut values = Zeroizing::new(vec![0.0; element_count]);
        for y in 0..content_height {
            let sample_y = axis_sample(source_height, content_height, y);
            for x in 0..content_width {
                let sample_x = axis_sample(source_width, content_width, x);
                let destination = y
                    .checked_mul(target_width)
                    .and_then(|row| row.checked_add(x))
                    .ok_or(OcrTensorError::ArithmeticOverflow)?;
                for channel in 0..3 {
                    let bgra_channel = 2 - channel;
                    let pixel =
                        sample_channel(frame.pixels(), stride, sample_x, sample_y, bgra_channel)?;
                    values[channel * plane_len + destination] =
                        (pixel * contract.scale - contract.mean[channel]) / contract.std[channel];
                }
            }
        }
        Ok((
            DetectorTensor {
                shape: [1, 3, target_height, target_width],
                values,
            },
            DetectionTransform {
                scale_x: content_width as f32 / source_width as f32,
                scale_y: content_height as f32 / source_height as f32,
                content_width,
                content_height,
                frame_width: source_width,
                frame_height: source_height,
            },
        ))
    }
}

pub struct RecognizerTensorBatch {
    shape: [usize; 4],
    widths: Vec<usize>,
    values: Zeroizing<Vec<f32>>,
}

impl RecognizerTensorBatch {
    pub fn shape(&self) -> [usize; 4] {
        self.shape
    }

    pub fn widths(&self) -> &[usize] {
        &self.widths
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.values
    }
}

impl fmt::Debug for RecognizerTensorBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecognizerTensorBatch")
            .field("shape", &self.shape)
            .field("crops", &self.widths.len())
            .field("elements", &self.values.len())
            .finish()
    }
}

pub struct RecognizerTensorBuilder;

impl RecognizerTensorBuilder {
    pub fn build_batch(
        frame: &PreparedFrame,
        boxes: &[TextQuadrilateral],
        contract: &OcrTensorContract,
        limits: &OcrResourceLimits,
    ) -> Result<RecognizerTensorBatch, OcrTensorError> {
        validate_recognizer_contract(contract, limits)?;
        if boxes.len() > limits.maximum_batch_size {
            return Err(OcrTensorError::BatchLimitExceeded);
        }
        let dimensions = frame.dimensions();
        let expected_frame_len = dimensions
            .tight_byte_len()
            .map_err(|_| OcrTensorError::ArithmeticOverflow)?;
        if frame.pixels().len() != expected_frame_len {
            return Err(OcrTensorError::InvalidFrameBuffer);
        }
        let widths: Vec<usize> = boxes
            .iter()
            .copied()
            .map(|quadrilateral| crop_width(quadrilateral, limits))
            .collect::<Result<_, _>>()?;
        let maximum_width = widths.iter().copied().max().unwrap_or(0);
        let height = limits.recognizer_height;
        let values_len = checked_batch_tensor_elements(boxes.len(), height, maximum_width)?;
        let plane_len = height
            .checked_mul(maximum_width)
            .ok_or(OcrTensorError::ArithmeticOverflow)?;
        let stride = dimensions
            .tight_stride()
            .map_err(|_| OcrTensorError::ArithmeticOverflow)?;
        let source_width =
            usize::try_from(dimensions.width()).map_err(|_| OcrTensorError::ArithmeticOverflow)?;
        let source_height =
            usize::try_from(dimensions.height()).map_err(|_| OcrTensorError::ArithmeticOverflow)?;
        let mut values = Zeroizing::new(vec![0.0; values_len]);
        for (batch_index, (quadrilateral, width)) in boxes.iter().zip(&widths).enumerate() {
            for target_y in 0..height {
                for target_x in 0..*width {
                    let source = inverse_bilinear_point(
                        quadrilateral.points(),
                        target_x,
                        target_y,
                        *width,
                        height,
                    );
                    for channel in 0..3 {
                        let pixel = sample_bgra_bilinear(
                            frame.pixels(),
                            stride,
                            source_width,
                            source_height,
                            source,
                            2 - channel,
                        )?;
                        let destination = batch_index
                            .checked_mul(3)
                            .and_then(|batch| batch.checked_add(channel))
                            .and_then(|plane| plane.checked_mul(plane_len))
                            .and_then(|plane| {
                                target_y
                                    .checked_mul(maximum_width)
                                    .and_then(|row| plane.checked_add(row))
                            })
                            .and_then(|row| row.checked_add(target_x))
                            .ok_or(OcrTensorError::ArithmeticOverflow)?;
                        values[destination] = (pixel * contract.scale - contract.mean[channel])
                            / contract.std[channel];
                    }
                }
            }
        }
        Ok(RecognizerTensorBatch {
            shape: [boxes.len(), 3, height, maximum_width],
            widths,
            values,
        })
    }
}

fn validate_detector_contract(contract: &OcrTensorContract) -> Result<(), OcrTensorError> {
    if contract.layout != TensorLayout::Nchw
        || contract.color_order != ColorOrder::Rgb
        || contract.element_type != OcrTensorElementType::F32
        || contract.channels != 3
        || contract.dimension_multiple != 32
        || contract.minimum_height == 0
        || contract.minimum_width == 0
        || contract.minimum_height > contract.maximum_height
        || contract.minimum_width > contract.maximum_width
        || contract.maximum_height > DETECTOR_MAXIMUM_EDGE
        || contract.maximum_width > DETECTOR_MAXIMUM_EDGE
        || contract.minimum_height % contract.dimension_multiple != 0
        || contract.minimum_width % contract.dimension_multiple != 0
        || contract.maximum_height % contract.dimension_multiple != 0
        || contract.maximum_width % contract.dimension_multiple != 0
        || !contract.scale.is_finite()
        || contract.mean.iter().any(|value| !value.is_finite())
        || contract
            .std
            .iter()
            .any(|value| !value.is_finite() || *value == 0.0)
    {
        return Err(OcrTensorError::InvalidContract);
    }
    Ok(())
}

fn validate_recognizer_contract(
    contract: &OcrTensorContract,
    limits: &OcrResourceLimits,
) -> Result<(), OcrTensorError> {
    if contract.layout != TensorLayout::Nchw
        || contract.color_order != ColorOrder::Rgb
        || contract.element_type != OcrTensorElementType::F32
        || contract.channels != 3
        || contract.minimum_height != limits.recognizer_height
        || contract.maximum_height != limits.recognizer_height
        || contract.minimum_width == 0
        || contract.maximum_width != limits.maximum_recognizer_width
        || contract.dimension_multiple != 1
        || limits.recognizer_height != 48
        || limits.maximum_recognizer_width > 320
        || limits.maximum_batch_size == 0
        || limits.maximum_batch_size > 8
        || !contract.scale.is_finite()
        || contract.mean.iter().any(|value| !value.is_finite())
        || contract
            .std
            .iter()
            .any(|value| !value.is_finite() || *value == 0.0)
    {
        return Err(OcrTensorError::InvalidContract);
    }
    Ok(())
}

fn scaled_dimension(source: usize, scale: f32) -> Result<usize, OcrTensorError> {
    let scaled = (source as f32 * scale).round();
    if !scaled.is_finite() || scaled < 1.0 || scaled > DETECTOR_MAXIMUM_EDGE as f32 {
        return Err(OcrTensorError::InvalidDimensions);
    }
    Ok(scaled as usize)
}

fn padded_dimension(content: usize, minimum: usize) -> Result<usize, OcrTensorError> {
    content
        .max(minimum)
        .checked_add(31)
        .map(|value| value / 32 * 32)
        .ok_or(OcrTensorError::ArithmeticOverflow)
}

fn checked_tensor_elements(width: usize, height: usize) -> Result<usize, OcrTensorError> {
    width
        .checked_mul(height)
        .and_then(|plane| plane.checked_mul(3))
        .ok_or(OcrTensorError::ArithmeticOverflow)
}

fn checked_batch_tensor_elements(
    batch: usize,
    height: usize,
    width: usize,
) -> Result<usize, OcrTensorError> {
    batch
        .checked_mul(3)
        .and_then(|channels| channels.checked_mul(height))
        .and_then(|rows| rows.checked_mul(width))
        .ok_or(OcrTensorError::ArithmeticOverflow)
}

fn crop_width(
    quadrilateral: TextQuadrilateral,
    limits: &OcrResourceLimits,
) -> Result<usize, OcrTensorError> {
    let height = quadrilateral.height();
    let aspect = quadrilateral.width() / height;
    let scaled = (limits.recognizer_height as f32 * aspect).ceil();
    if !scaled.is_finite() || scaled < 1.0 {
        return Err(OcrTensorError::InvalidGeometry);
    }
    Ok((scaled as usize).min(limits.maximum_recognizer_width))
}

fn inverse_bilinear_point(
    points: [[f32; 2]; 4],
    target_x: usize,
    target_y: usize,
    width: usize,
    height: usize,
) -> [f32; 2] {
    let u = if width <= 1 {
        0.0
    } else {
        target_x as f32 / (width - 1) as f32
    };
    let v = if height <= 1 {
        0.0
    } else {
        target_y as f32 / (height - 1) as f32
    };
    let top = [
        points[0][0] + (points[1][0] - points[0][0]) * u,
        points[0][1] + (points[1][1] - points[0][1]) * u,
    ];
    let bottom = [
        points[3][0] + (points[2][0] - points[3][0]) * u,
        points[3][1] + (points[2][1] - points[3][1]) * u,
    ];
    [
        top[0] + (bottom[0] - top[0]) * v,
        top[1] + (bottom[1] - top[1]) * v,
    ]
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
) -> Result<f32, OcrTensorError> {
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
            .ok_or(OcrTensorError::InvalidFrameBuffer)
    };
    let top_left = sample(x.low, y.low)?;
    let top_right = sample(x.high, y.low)?;
    let bottom_left = sample(x.low, y.high)?;
    let bottom_right = sample(x.high, y.high)?;
    let top = top_left + (top_right - top_left) * x.high_weight;
    let bottom = bottom_left + (bottom_right - bottom_left) * x.high_weight;
    Ok(top + (bottom - top) * y.high_weight)
}

fn sample_bgra_bilinear(
    pixels: &[u8],
    stride: usize,
    source_width: usize,
    source_height: usize,
    point: [f32; 2],
    channel: usize,
) -> Result<f32, OcrTensorError> {
    if !point[0].is_finite() || !point[1].is_finite() {
        return Err(OcrTensorError::InvalidGeometry);
    }
    let x = point[0].clamp(0.0, (source_width - 1) as f32);
    let y = point[1].clamp(0.0, (source_height - 1) as f32);
    let x_low = x.floor() as usize;
    let y_low = y.floor() as usize;
    sample_channel(
        pixels,
        stride,
        AxisSample {
            low: x_low,
            high: (x_low + 1).min(source_width - 1),
            high_weight: x - x_low as f32,
        },
        AxisSample {
            low: y_low,
            high: (y_low + 1).min(source_height - 1),
            high_weight: y - y_low as f32,
        },
        channel,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BgraFrame, ColorOrder, FrameDimensions, FramePreparationConfig, FramePreparer,
        OcrResourceLimits, OcrTensorContract, OcrTensorElementType, TensorLayout,
        TextQuadrilateral,
    };
    use karma_domain::MonitorId;

    fn detector_contract() -> OcrTensorContract {
        OcrTensorContract {
            input_name: "x".into(),
            output_name: "sigmoid_0.tmp_0".into(),
            layout: TensorLayout::Nchw,
            color_order: ColorOrder::Rgb,
            element_type: OcrTensorElementType::F32,
            channels: 3,
            minimum_height: 32,
            maximum_height: 640,
            minimum_width: 32,
            maximum_width: 640,
            dimension_multiple: 32,
            scale: 1.0 / 255.0,
            mean: [0.0, 0.0, 0.0],
            std: [1.0, 1.0, 1.0],
        }
    }

    fn recognizer_contract() -> OcrTensorContract {
        OcrTensorContract {
            input_name: "x".into(),
            output_name: "softmax_0.tmp_0".into(),
            layout: TensorLayout::Nchw,
            color_order: ColorOrder::Rgb,
            element_type: OcrTensorElementType::F32,
            channels: 3,
            minimum_height: 48,
            maximum_height: 48,
            minimum_width: 1,
            maximum_width: 320,
            dimension_multiple: 1,
            scale: 1.0 / 255.0,
            mean: [0.0, 0.0, 0.0],
            std: [1.0, 1.0, 1.0],
        }
    }

    fn limits() -> OcrResourceLimits {
        OcrResourceLimits {
            maximum_text_boxes: 64,
            minimum_box_side_pixels: 6,
            minimum_box_area_pixels: 48,
            recognizer_height: 48,
            maximum_recognizer_width: 320,
            maximum_batch_size: 8,
            maximum_line_characters: 128,
            maximum_total_characters: 4_096,
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
    fn detector_converts_bgra_to_rgb_nchw_and_redacts_values() {
        let frame = prepared(2, 1, vec![0, 0, 255, 255, 255, 0, 0, 255]);

        let (tensor, _) = DetectorTensorBuilder::build(&frame, &detector_contract()).unwrap();

        assert_eq!(tensor.shape(), [1, 3, 32, 32]);
        assert_eq!(tensor.as_slice()[0], 1.0);
        assert_eq!(tensor.as_slice()[1], 0.0);
        assert_eq!(tensor.as_slice()[32 * 32], 0.0);
        assert_eq!(tensor.as_slice()[32 * 32 + 1], 0.0);
        assert_eq!(tensor.as_slice()[2 * 32 * 32], 0.0);
        assert_eq!(tensor.as_slice()[2 * 32 * 32 + 1], 1.0);
        let debug = format!("{tensor:?}");
        assert!(debug.contains("elements: 3072"));
        assert!(!debug.contains("1.0"));
    }

    #[test]
    fn detector_preserves_aspect_ratio_pads_to_32_and_maps_coordinates_back() {
        let frame = prepared(640, 360, vec![0; 640 * 360 * 4]);

        let (tensor, transform) =
            DetectorTensorBuilder::build(&frame, &detector_contract()).unwrap();

        assert_eq!(tensor.shape(), [1, 3, 384, 640]);
        assert_eq!(
            transform.map_to_frame(320.0, 180.0).unwrap(),
            [320.0, 180.0]
        );
        assert_eq!(
            transform.map_to_frame(640.0, 384.0).unwrap(),
            [639.0, 359.0]
        );
    }

    #[test]
    fn detector_normalizes_each_channel_exactly() {
        let frame = prepared(1, 1, vec![30, 20, 10, 255]);
        let mut contract = detector_contract();
        contract.mean = [0.1, 0.2, 0.3];
        contract.std = [0.5, 0.25, 0.1];

        let (tensor, _) = DetectorTensorBuilder::build(&frame, &contract).unwrap();

        assert_eq!(
            tensor.as_slice()[0],
            ((10.0 * contract.scale) - contract.mean[0]) / contract.std[0]
        );
        assert_eq!(
            tensor.as_slice()[32 * 32],
            ((20.0 * contract.scale) - contract.mean[1]) / contract.std[1]
        );
        assert_eq!(
            tensor.as_slice()[2 * 32 * 32],
            ((30.0 * contract.scale) - contract.mean[2]) / contract.std[2]
        );
    }

    #[test]
    fn detector_rejects_zero_or_non_finite_contract_parameters_and_unbounded_edges() {
        let frame = prepared(1, 1, vec![0, 0, 0, 255]);

        let mut zero = detector_contract();
        zero.minimum_width = 0;
        assert_eq!(
            DetectorTensorBuilder::build(&frame, &zero).unwrap_err(),
            OcrTensorError::InvalidContract
        );

        let mut nan = detector_contract();
        nan.mean[0] = f32::NAN;
        assert_eq!(
            DetectorTensorBuilder::build(&frame, &nan).unwrap_err(),
            OcrTensorError::InvalidContract
        );

        let mut unbounded = detector_contract();
        unbounded.maximum_width = 672;
        assert_eq!(
            DetectorTensorBuilder::build(&frame, &unbounded).unwrap_err(),
            OcrTensorError::InvalidContract
        );
    }

    #[test]
    fn detector_checks_generated_tensor_allocation() {
        assert_eq!(
            checked_tensor_elements(usize::MAX, 640),
            Err(OcrTensorError::ArithmeticOverflow)
        );
    }

    #[test]
    fn recognizer_uses_inverse_bilinear_rgb_sampling_and_pads_to_batch_width() {
        let pixels = (0..7)
            .flat_map(|y| {
                (0..13).flat_map(move |x| {
                    let red = (x * 10) as u8;
                    let green = (y * 10) as u8;
                    [0, green, red, 255]
                })
            })
            .collect();
        let frame = prepared(13, 7, pixels);
        let small = TextQuadrilateral::new(
            [[0.0, 0.0], [8.0, 0.0], [8.0, 6.0], [0.0, 6.0]],
            frame.dimensions(),
            &limits(),
        )
        .unwrap();
        let wide = TextQuadrilateral::new(
            [[0.0, 0.0], [12.0, 0.0], [12.0, 6.0], [0.0, 6.0]],
            frame.dimensions(),
            &limits(),
        )
        .unwrap();

        let contract = recognizer_contract();
        let batch =
            RecognizerTensorBuilder::build_batch(&frame, &[small, wide], &contract, &limits())
                .unwrap();

        assert_eq!(batch.shape(), [2, 3, 48, 96]);
        assert_eq!(batch.widths(), &[64, 96]);
        assert_eq!(batch.as_slice()[0], 0.0);
        assert_eq!(batch.as_slice()[95], 0.0);
        assert_eq!(batch.as_slice()[96], 0.0);
        assert_eq!(batch.as_slice()[3 * 96 * 48], 0.0);
        assert_eq!(batch.as_slice()[3 * 96 * 48 + 95], 120.0 * contract.scale);
        let debug = format!("{batch:?}");
        assert!(debug.contains("elements: 27648"));
        assert!(!debug.contains("0.470"));
    }

    #[test]
    fn recognizer_rejects_batches_larger_than_eight() {
        let frame = prepared(9, 7, vec![0; 9 * 7 * 4]);
        let quadrilateral = TextQuadrilateral::new(
            [[0.0, 0.0], [8.0, 0.0], [8.0, 6.0], [0.0, 6.0]],
            frame.dimensions(),
            &limits(),
        )
        .unwrap();

        assert_eq!(
            RecognizerTensorBuilder::build_batch(
                &frame,
                &vec![quadrilateral; 9],
                &recognizer_contract(),
                &limits(),
            )
            .unwrap_err(),
            OcrTensorError::BatchLimitExceeded
        );
    }
}
