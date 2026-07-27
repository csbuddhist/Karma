use karma_ai::{
    BgraFrame, ColorOrder, DetectionMap, DetectorTensorBuilder, FrameDimensions,
    FramePreparationConfig, FramePreparer, OcrResourceLimits, OcrTensorContract,
    OcrTensorElementType, OcrThresholds, TensorLayout,
};
use karma_domain::MonitorId;
use karma_onnx::DbPostProcessor;

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

fn detector_contract() -> OcrTensorContract {
    OcrTensorContract {
        input_name: "x".into(),
        output_name: "probability_map".into(),
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
        mean: [0.485, 0.456, 0.406],
        std: [0.229, 0.224, 0.225],
    }
}

fn transform(width: u32, height: u32) -> karma_ai::DetectionTransform {
    let dimensions = FrameDimensions::new(width, height).unwrap();
    let input = BgraFrame::new(
        MonitorId("fixture-display".into()),
        1,
        dimensions,
        dimensions.tight_stride().unwrap(),
        vec![0; dimensions.tight_byte_len().unwrap()],
    )
    .unwrap();
    let frame = FramePreparer::new(FramePreparationConfig::new(width.max(height)).unwrap())
        .prepare(input)
        .unwrap();
    DetectorTensorBuilder::build(&frame, &detector_contract())
        .unwrap()
        .1
}

fn rectangular_map(
    width: usize,
    height: usize,
    rectangles: &[(usize, usize, usize, usize, f32)],
) -> DetectionMap {
    let mut values = vec![0.0; width * height];
    for &(left, top, right, bottom, value) in rectangles {
        for y in top..bottom {
            for x in left..right {
                values[y * width + x] = value;
            }
        }
    }
    DetectionMap::from_values(width, height, values).unwrap()
}

#[test]
fn rejects_configuration_that_weakens_db_resource_limits() {
    let mut unbounded = limits();
    unbounded.maximum_text_boxes = 65;
    assert!(DbPostProcessor::new(OcrThresholds::default(), unbounded).is_err());

    let invalid_thresholds = OcrThresholds {
        expansion: f32::NAN,
        ..OcrThresholds::default()
    };
    assert!(DbPostProcessor::new(invalid_thresholds, limits()).is_err());
}

#[test]
fn thresholds_connected_regions_scores_unclips_and_clips_quadrilaterals() {
    let processor = DbPostProcessor::new(OcrThresholds::default(), limits()).unwrap();
    let map = rectangular_map(
        64,
        64,
        &[
            (0, 4, 16, 16, 0.9),
            (30, 30, 46, 44, 0.9),
            (50, 4, 62, 16, 0.4),
        ],
    );

    let boxes = processor
        .extract(
            &map,
            transform(64, 64),
            FrameDimensions::new(64, 64).unwrap(),
        )
        .unwrap();

    assert_eq!(boxes.len(), 2);
    let first = boxes[0].points();
    assert_eq!(first[0][0], 0.0);
    assert!(first[1][0] > 15.0);
    assert!(first[3][1] > 15.0);
    assert!(boxes[1].points()[0][1] > first[0][1]);
}

#[test]
fn skips_small_or_malformed_candidates_without_losing_valid_regions() {
    let processor = DbPostProcessor::new(OcrThresholds::default(), limits()).unwrap();
    let map = rectangular_map(64, 64, &[(2, 2, 5, 5, 0.9), (20, 20, 36, 32, 0.9)]);

    let boxes = processor
        .extract(
            &map,
            transform(64, 64),
            FrameDimensions::new(64, 64).unwrap(),
        )
        .unwrap();

    assert_eq!(boxes.len(), 1);
    assert!(boxes[0].points()[0][0] < 20.0);
}

#[test]
fn orders_regions_stably_and_enforces_the_sixty_four_box_limit() {
    let processor = DbPostProcessor::new(OcrThresholds::default(), limits()).unwrap();
    let mut rectangles = Vec::new();
    for row in 0..7 {
        for column in 0..10 {
            let x = 4 + column * 28;
            let y = 4 + row * 28;
            rectangles.push((x, y, x + 10, y + 10, 0.9));
        }
    }
    rectangles.reverse();
    let map = rectangular_map(320, 224, &rectangles);

    let boxes = processor
        .extract(
            &map,
            transform(320, 224),
            FrameDimensions::new(320, 224).unwrap(),
        )
        .unwrap();

    assert_eq!(boxes.len(), 64);
    for (index, quadrilateral) in boxes.iter().enumerate() {
        let center = quadrilateral
            .points()
            .into_iter()
            .fold([0.0, 0.0], |[x, y], point| {
                [x + point[0] / 4.0, y + point[1] / 4.0]
            });
        let expected_row = index / 10;
        let expected_column = index % 10;
        let expected = [
            8.5 + expected_column as f32 * 28.0,
            8.5 + expected_row as f32 * 28.0,
        ];
        assert!(
            (center[0] - expected[0]).abs() < 1.0e-3,
            "box {index} has x center {}, expected {}",
            center[0],
            expected[0]
        );
        assert!(
            (center[1] - expected[1]).abs() < 1.0e-3,
            "box {index} has y center {}, expected {}",
            center[1],
            expected[1]
        );
    }
}
