use std::cmp::Ordering;

use image::{GrayImage, Luma};
use imageproc::{
    contours::{BorderType, find_contours},
    geometry::convex_hull,
    point::Point,
};
use karma_ai::{
    DetectionMap, DetectionTransform, FrameDimensions, OcrResourceLimits, OcrThresholds,
    TextQuadrilateral, sort_and_limit_boxes,
};
use thiserror::Error;
use zeroize::Zeroize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DbPostProcessError {
    #[error("OCR DB postprocessor configuration is invalid")]
    InvalidConfiguration,
    #[error("OCR DB detection map is invalid")]
    InvalidMap,
}

pub struct DbPostProcessor {
    thresholds: OcrThresholds,
    limits: OcrResourceLimits,
}

impl DbPostProcessor {
    pub fn new(
        thresholds: OcrThresholds,
        limits: OcrResourceLimits,
    ) -> Result<Self, DbPostProcessError> {
        if !thresholds.probability.is_finite()
            || !thresholds.text_box.is_finite()
            || !thresholds.expansion.is_finite()
            || !(0.0..=1.0).contains(&thresholds.probability)
            || !(0.0..=1.0).contains(&thresholds.text_box)
            || !(1.0..=3.0).contains(&thresholds.expansion)
            || limits.maximum_text_boxes == 0
            || limits.maximum_text_boxes > 64
            || limits.minimum_box_side_pixels < 6
            || limits.minimum_box_area_pixels < 48
        {
            return Err(DbPostProcessError::InvalidConfiguration);
        }
        Ok(Self { thresholds, limits })
    }

    pub fn extract(
        &self,
        map: &DetectionMap,
        transform: DetectionTransform,
        frame: FrameDimensions,
    ) -> Result<Vec<TextQuadrilateral>, DbPostProcessError> {
        let [width, height] = map.dimensions();
        let [content_width, content_height] = transform.content_dimensions();
        if width == 0
            || height == 0
            || content_width > width
            || content_height > height
            || frame.width() == 0
            || frame.height() == 0
        {
            return Err(DbPostProcessError::InvalidMap);
        }
        let mask = map
            .threshold(self.thresholds.probability)
            .map_err(|_| DbPostProcessError::InvalidMap)?;
        let image_width =
            u32::try_from(width.checked_add(2).ok_or(DbPostProcessError::InvalidMap)?)
                .map_err(|_| DbPostProcessError::InvalidMap)?;
        let image_height = u32::try_from(
            height
                .checked_add(2)
                .ok_or(DbPostProcessError::InvalidMap)?,
        )
        .map_err(|_| DbPostProcessError::InvalidMap)?;
        let mut binary = GrayImage::from_fn(image_width, image_height, |x, y| {
            let source_x = usize::try_from(x)
                .ok()
                .and_then(|value| value.checked_sub(1));
            let source_y = usize::try_from(y)
                .ok()
                .and_then(|value| value.checked_sub(1));
            let active = source_x
                .zip(source_y)
                .filter(|(x, y)| *x < content_width && *y < content_height)
                .and_then(|(x, y)| mask.is_active(x, y))
                .unwrap_or(false);
            Luma([if active { 255 } else { 0 }])
        });
        let contours = find_contours::<u32>(&binary);
        binary.as_mut().zeroize();
        drop(binary);
        drop(mask);

        let mut boxes = Vec::new();
        for contour in contours
            .iter()
            .filter(|contour| contour.border_type == BorderType::Outer)
        {
            let Some(points) = contour_points(&contour.points) else {
                continue;
            };
            let Some(rectangle) = minimum_area_rectangle(&points) else {
                continue;
            };
            match map.region_mean_meets(&rectangle, self.thresholds.text_box) {
                Ok(true) => {}
                Ok(false) | Err(_) => continue,
            }
            let Some(expanded) = expand_rectangle(rectangle, self.thresholds.expansion) else {
                continue;
            };
            let mut mapped = [[0.0; 2]; 4];
            let mut valid = true;
            for (destination, [x, y]) in mapped.iter_mut().zip(expanded) {
                match transform.map_to_frame(x, y) {
                    Ok(point) => *destination = point,
                    Err(_) => {
                        valid = false;
                        break;
                    }
                }
            }
            if !valid {
                continue;
            }
            if let Ok(quadrilateral) = TextQuadrilateral::new(mapped, frame, &self.limits) {
                boxes.push(quadrilateral);
            }
        }
        Ok(sort_and_limit_boxes(boxes, &self.limits))
    }
}

fn contour_points(points: &[Point<u32>]) -> Option<Vec<Point<i32>>> {
    if points.len() < 3 {
        return None;
    }
    points
        .iter()
        .map(|point| {
            let x = i32::try_from(point.x.checked_sub(1)?).ok()?;
            let y = i32::try_from(point.y.checked_sub(1)?).ok()?;
            Some(Point::new(x, y))
        })
        .collect()
}

fn minimum_area_rectangle(points: &[Point<i32>]) -> Option<[[f32; 2]; 4]> {
    let hull = convex_hull(points);
    if hull.len() < 3 {
        return None;
    }
    let mut best: Option<(f32, f32, [f32; 4])> = None;
    for index in 0..hull.len() {
        let current = hull[index];
        let next = hull[(index + 1) % hull.len()];
        let angle = (next.y - current.y) as f32;
        let angle = angle.atan2((next.x - current.x) as f32);
        if !angle.is_finite() {
            continue;
        }
        let cosine = angle.cos();
        let sine = angle.sin();
        let mut bounds = [
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];
        for point in &hull {
            let point_x = point.x as f32;
            let point_y = point.y as f32;
            let rotated_x = point_x * cosine + point_y * sine;
            let rotated_y = -point_x * sine + point_y * cosine;
            bounds[0] = bounds[0].min(rotated_x);
            bounds[1] = bounds[1].max(rotated_x);
            bounds[2] = bounds[2].min(rotated_y);
            bounds[3] = bounds[3].max(rotated_y);
        }
        let area = (bounds[1] - bounds[0]) * (bounds[3] - bounds[2]);
        if !area.is_finite() || area <= 0.0 {
            continue;
        }
        if best.as_ref().is_none_or(|(best_area, best_angle, _)| {
            area.partial_cmp(best_area).unwrap_or(Ordering::Greater) == Ordering::Less
                || ((area - *best_area).abs() <= f32::EPSILON && angle < *best_angle)
        }) {
            best = Some((area, angle, bounds));
        }
    }
    let (_, angle, [minimum_x, maximum_x, minimum_y, maximum_y]) = best?;
    let cosine = angle.cos();
    let sine = angle.sin();
    Some(
        [
            [minimum_x, minimum_y],
            [maximum_x, minimum_y],
            [maximum_x, maximum_y],
            [minimum_x, maximum_y],
        ]
        .map(|[x, y]| [x * cosine - y * sine, x * sine + y * cosine]),
    )
}

fn expand_rectangle(points: [[f32; 2]; 4], ratio: f32) -> Option<[[f32; 2]; 4]> {
    let area = polygon_area(&points);
    let perimeter: f32 = (0..4)
        .map(|index| distance(points[index], points[(index + 1) % 4]))
        .sum();
    let distance = area * ratio / perimeter;
    if !area.is_finite()
        || area <= 0.0
        || !perimeter.is_finite()
        || perimeter <= 0.0
        || !distance.is_finite()
    {
        return None;
    }
    let center = points.iter().fold([0.0, 0.0], |[x, y], point| {
        [x + point[0] / 4.0, y + point[1] / 4.0]
    });
    let mut expanded = [[0.0; 2]; 4];
    for (destination, point) in expanded.iter_mut().zip(points) {
        let vector = [point[0] - center[0], point[1] - center[1]];
        let radius = vector[0].hypot(vector[1]);
        if !radius.is_finite() || radius <= 0.0 {
            return None;
        }
        let scale = (radius + distance) / radius;
        *destination = [center[0] + vector[0] * scale, center[1] + vector[1] * scale];
    }
    Some(expanded)
}

fn polygon_area(points: &[[f32; 2]; 4]) -> f32 {
    (0..4)
        .map(|index| {
            points[index][0] * points[(index + 1) % 4][1]
                - points[(index + 1) % 4][0] * points[index][1]
        })
        .sum::<f32>()
        .abs()
        / 2.0
}

fn distance(left: [f32; 2], right: [f32; 2]) -> f32 {
    (right[0] - left[0]).hypot(right[1] - left[1])
}
