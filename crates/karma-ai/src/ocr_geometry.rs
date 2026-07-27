use std::cmp::Ordering;

use crate::{FrameDimensions, OcrResourceLimits, OcrTensorError};

/// Input points are measured in pixels. This is one ten-thousandth of a pixel, so it absorbs
/// floating-point roundoff without excluding valid boxes above the 6-pixel/48-pixel limits.
const PIXEL_COORDINATE_EPSILON: f32 = 1.0e-4;
const SQUARE_PIXEL_EPSILON: f32 = PIXEL_COORDINATE_EPSILON * PIXEL_COORDINATE_EPSILON;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextQuadrilateral {
    points: [[f32; 2]; 4],
}

impl TextQuadrilateral {
    pub fn new(
        points: [[f32; 2]; 4],
        frame: FrameDimensions,
        limits: &OcrResourceLimits,
    ) -> Result<Self, OcrTensorError> {
        if points
            .iter()
            .flatten()
            .any(|coordinate| !coordinate.is_finite())
        {
            return Err(OcrTensorError::InvalidCoordinate);
        }
        let maximum_x = frame.width() as f32 - 1.0;
        let maximum_y = frame.height() as f32 - 1.0;
        let mut points = points.map(|[x, y]| [x.clamp(0.0, maximum_x), y.clamp(0.0, maximum_y)]);
        normalize_clockwise_from_top_left(&mut points);
        let quadrilateral = Self { points };
        if quadrilateral.shortest_edge() < limits.minimum_box_side_pixels as f32
            || quadrilateral.area() < limits.minimum_box_area_pixels as f32
            || !has_strict_consistent_winding(points)
            || !has_valid_bilinear_jacobian(points)
        {
            return Err(OcrTensorError::InvalidGeometry);
        }
        Ok(quadrilateral)
    }

    pub fn points(self) -> [[f32; 2]; 4] {
        self.points
    }

    pub(crate) fn width(self) -> f32 {
        (distance(self.points[0], self.points[1]) + distance(self.points[3], self.points[2])) / 2.0
    }

    pub(crate) fn height(self) -> f32 {
        (distance(self.points[0], self.points[3]) + distance(self.points[1], self.points[2])) / 2.0
    }

    fn shortest_edge(self) -> f32 {
        (0..4)
            .map(|index| distance(self.points[index], self.points[(index + 1) % 4]))
            .fold(f32::INFINITY, f32::min)
    }

    fn area(self) -> f32 {
        let doubled_area: f32 = (0..4)
            .map(|index| {
                let current = self.points[index];
                let next = self.points[(index + 1) % 4];
                current[0] * next[1] - next[0] * current[1]
            })
            .sum();
        doubled_area.abs() / 2.0
    }

    fn center(self) -> [f32; 2] {
        let [sum_x, sum_y] = self
            .points
            .into_iter()
            .fold([0.0, 0.0], |[sum_x, sum_y], [x, y]| [sum_x + x, sum_y + y]);
        [sum_x / 4.0, sum_y / 4.0]
    }
}

pub fn sort_and_limit_boxes(
    mut boxes: Vec<TextQuadrilateral>,
    limits: &OcrResourceLimits,
) -> Vec<TextQuadrilateral> {
    boxes.sort_by(|left, right| {
        let left_center = left.center();
        let right_center = right.center();
        left_center[1]
            .partial_cmp(&right_center[1])
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                left_center[0]
                    .partial_cmp(&right_center[0])
                    .unwrap_or(Ordering::Equal)
            })
    });

    let mut sorted = Vec::with_capacity(boxes.len().min(limits.maximum_text_boxes));
    let mut row_start = 0;
    while row_start < boxes.len() && sorted.len() < limits.maximum_text_boxes {
        let baseline = boxes[row_start].center()[1];
        let mut row_end = row_start + 1;
        while row_end < boxes.len() {
            let candidate = boxes[row_end];
            let tolerance = (boxes[row_start].height() + candidate.height()) / 4.0;
            if candidate.center()[1] - baseline > tolerance {
                break;
            }
            row_end += 1;
        }
        boxes[row_start..row_end].sort_by(|left, right| {
            left.center()[0]
                .partial_cmp(&right.center()[0])
                .unwrap_or(Ordering::Equal)
        });
        let remaining = limits.maximum_text_boxes - sorted.len();
        sorted.extend(boxes[row_start..row_end].iter().copied().take(remaining));
        row_start = row_end;
    }
    sorted
}

fn normalize_clockwise_from_top_left(points: &mut [[f32; 2]; 4]) {
    let center = points.iter().fold([0.0, 0.0], |[sum_x, sum_y], [x, y]| {
        [sum_x + x / 4.0, sum_y + y / 4.0]
    });
    points.sort_by(|left, right| {
        let left_angle = (left[1] - center[1]).atan2(left[0] - center[0]);
        let right_angle = (right[1] - center[1]).atan2(right[0] - center[0]);
        left_angle
            .partial_cmp(&right_angle)
            .unwrap_or(Ordering::Equal)
    });
    let top_left = points
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            left[1]
                .partial_cmp(&right[1])
                .unwrap_or(Ordering::Equal)
                .then_with(|| left[0].partial_cmp(&right[0]).unwrap_or(Ordering::Equal))
        })
        .map(|(index, _)| index)
        .unwrap_or(0);
    points.rotate_left(top_left);
}

fn distance(left: [f32; 2], right: [f32; 2]) -> f32 {
    (right[0] - left[0]).hypot(right[1] - left[1])
}

fn has_strict_consistent_winding(points: [[f32; 2]; 4]) -> bool {
    has_consistent_nonzero_sign((0..4).map(|index| {
        let current = edge(points[index], points[(index + 1) % 4]);
        let next = edge(points[(index + 1) % 4], points[(index + 2) % 4]);
        cross(current, next)
    }))
}

fn has_valid_bilinear_jacobian(points: [[f32; 2]; 4]) -> bool {
    has_consistent_nonzero_sign(
        [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|(u, v)| {
            let derivative_u = [
                (points[1][0] - points[0][0]) * (1.0 - v) + (points[2][0] - points[3][0]) * v,
                (points[1][1] - points[0][1]) * (1.0 - v) + (points[2][1] - points[3][1]) * v,
            ];
            let derivative_v = [
                (points[3][0] - points[0][0]) * (1.0 - u) + (points[2][0] - points[1][0]) * u,
                (points[3][1] - points[0][1]) * (1.0 - u) + (points[2][1] - points[1][1]) * u,
            ];
            cross(derivative_u, derivative_v)
        }),
    )
}

fn has_consistent_nonzero_sign(values: impl IntoIterator<Item = f32>) -> bool {
    let mut sign = None;
    for value in values {
        if !value.is_finite() || value.abs() <= SQUARE_PIXEL_EPSILON {
            return false;
        }
        let current_sign = value.is_sign_positive();
        if sign.is_some_and(|expected| expected != current_sign) {
            return false;
        }
        sign = Some(current_sign);
    }
    sign.is_some()
}

fn edge(from: [f32; 2], to: [f32; 2]) -> [f32; 2] {
    [to[0] - from[0], to[1] - from[1]]
}

fn cross(left: [f32; 2], right: [f32; 2]) -> f32 {
    left[0] * right[1] - left[1] * right[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FrameDimensions, OcrResourceLimits, OcrTensorError};

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

    fn frame() -> FrameDimensions {
        FrameDimensions::new(100, 60).unwrap()
    }

    fn box_at(x: f32, y: f32) -> TextQuadrilateral {
        TextQuadrilateral::new(
            [[x, y], [x + 12.0, y], [x + 12.0, y + 10.0], [x, y + 10.0]],
            frame(),
            &limits(),
        )
        .unwrap()
    }

    #[test]
    fn quadrilateral_rejects_non_finite_coordinates() {
        assert_eq!(
            TextQuadrilateral::new(
                [[0.0, 0.0], [12.0, 0.0], [12.0, f32::NAN], [0.0, 10.0]],
                frame(),
                &limits(),
            )
            .unwrap_err(),
            OcrTensorError::InvalidCoordinate
        );
    }

    #[test]
    fn quadrilateral_clamps_and_normalizes_clockwise_from_top_left() {
        let quadrilateral = TextQuadrilateral::new(
            [[110.0, 70.0], [-5.0, -2.0], [110.0, -2.0], [-5.0, 70.0]],
            frame(),
            &limits(),
        )
        .unwrap();

        assert_eq!(
            quadrilateral.points(),
            [[0.0, 0.0], [99.0, 0.0], [99.0, 59.0], [0.0, 59.0]]
        );
    }

    #[test]
    fn quadrilateral_rejects_boxes_below_side_or_area_limits() {
        assert_eq!(
            TextQuadrilateral::new(
                [[0.0, 0.0], [5.0, 0.0], [5.0, 20.0], [0.0, 20.0]],
                frame(),
                &limits(),
            )
            .unwrap_err(),
            OcrTensorError::InvalidGeometry
        );
        assert_eq!(
            TextQuadrilateral::new(
                [[0.0, 0.0], [7.0, 0.0], [7.0, 6.0], [0.0, 6.0]],
                frame(),
                &limits(),
            )
            .unwrap_err(),
            OcrTensorError::InvalidGeometry
        );
    }

    #[test]
    fn quadrilateral_rejects_folded_or_self_collapsing_transforms() {
        for points in [
            [[0.0, 0.0], [20.0, 0.0], [10.0, 10.0], [0.0, 20.0]],
            [[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [10.0, 10.0]],
        ] {
            assert_eq!(
                TextQuadrilateral::new(points, frame(), &limits()).unwrap_err(),
                OcrTensorError::InvalidGeometry
            );
        }
    }

    #[test]
    fn boxes_sort_stably_by_rows_then_left_to_right_and_limit_to_64() {
        let left = box_at(5.0, 15.0);
        let right = box_at(30.0, 10.0);
        let lower = box_at(3.0, 31.0);
        let sorted = sort_and_limit_boxes(vec![right, lower, left], &limits());
        assert_eq!(sorted, vec![left, right, lower]);

        let limited = sort_and_limit_boxes(vec![box_at(0.0, 0.0); 65], &limits());
        assert_eq!(limited.len(), 64);
    }
}
