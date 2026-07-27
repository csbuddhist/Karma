use std::fmt;

use thiserror::Error;
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DetectionMapError {
    #[error("OCR detection-map dimensions are invalid")]
    InvalidDimensions,
    #[error("OCR detection-map element count is invalid")]
    InvalidElementCount,
    #[error("OCR detection-map contains a non-finite value")]
    NonFiniteValue,
    #[error("OCR detection-map value is outside the probability range")]
    OutOfRangeValue,
    #[error("OCR detection-map threshold is invalid")]
    InvalidThreshold,
    #[error("OCR detection-map region is invalid")]
    InvalidRegion,
}

/// A portable probability map with zeroizing storage.
///
/// The map deliberately has no serialization implementation and exposes no probability slice.
///
/// ```compile_fail
/// use karma_ai::DetectionMap;
///
/// let map = DetectionMap::from_values(1, 1, vec![0.5]).unwrap();
/// let _ = serde_json::to_string(&map);
/// ```
pub struct DetectionMap {
    width: usize,
    height: usize,
    values: Zeroizing<Vec<f32>>,
}

impl DetectionMap {
    pub fn from_values(
        width: usize,
        height: usize,
        values: Vec<f32>,
    ) -> Result<Self, DetectionMapError> {
        let expected = width
            .checked_mul(height)
            .filter(|count| *count > 0)
            .ok_or(DetectionMapError::InvalidDimensions)?;
        if values.len() != expected {
            return Err(DetectionMapError::InvalidElementCount);
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(DetectionMapError::NonFiniteValue);
        }
        if values.iter().any(|value| !(0.0..=1.0).contains(value)) {
            return Err(DetectionMapError::OutOfRangeValue);
        }
        Ok(Self {
            width,
            height,
            values: Zeroizing::new(values),
        })
    }

    pub fn dimensions(&self) -> [usize; 2] {
        [self.width, self.height]
    }

    pub fn threshold(&self, threshold: f32) -> Result<DetectionMask, DetectionMapError> {
        if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
            return Err(DetectionMapError::InvalidThreshold);
        }
        Ok(DetectionMask {
            width: self.width,
            height: self.height,
            values: Zeroizing::new(
                self.values
                    .iter()
                    .map(|value| u8::from(*value >= threshold))
                    .collect(),
            ),
        })
    }

    /// Compares a polygon's mean score with a threshold without returning score values.
    pub fn region_mean_meets(
        &self,
        polygon: &[[f32; 2]],
        threshold: f32,
    ) -> Result<bool, DetectionMapError> {
        if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
            return Err(DetectionMapError::InvalidThreshold);
        }
        if polygon.len() < 3
            || polygon
                .iter()
                .flatten()
                .any(|coordinate| !coordinate.is_finite())
        {
            return Err(DetectionMapError::InvalidRegion);
        }
        let minimum_x = polygon
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let maximum_x = polygon
            .iter()
            .map(|point| point[0])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(self.width as f32) as usize;
        let minimum_y = polygon
            .iter()
            .map(|point| point[1])
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let maximum_y = polygon
            .iter()
            .map(|point| point[1])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(self.height as f32) as usize;

        let mut sum = 0.0_f64;
        let mut count = 0_usize;
        for y in minimum_y..maximum_y {
            for x in minimum_x..maximum_x {
                if point_in_polygon([x as f32 + 0.5, y as f32 + 0.5], polygon) {
                    let index = y
                        .checked_mul(self.width)
                        .and_then(|row| row.checked_add(x))
                        .ok_or(DetectionMapError::InvalidRegion)?;
                    sum += f64::from(self.values[index]);
                    count = count
                        .checked_add(1)
                        .ok_or(DetectionMapError::InvalidRegion)?;
                }
            }
        }
        if count == 0 {
            return Err(DetectionMapError::InvalidRegion);
        }
        Ok(sum / count as f64 >= f64::from(threshold))
    }
}

impl fmt::Debug for DetectionMap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DetectionMap")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

/// A threshold decision map. It contains no source probabilities.
pub struct DetectionMask {
    width: usize,
    height: usize,
    values: Zeroizing<Vec<u8>>,
}

impl DetectionMask {
    pub fn dimensions(&self) -> [usize; 2] {
        [self.width, self.height]
    }

    pub fn is_active(&self, x: usize, y: usize) -> Option<bool> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.values
            .get(y.checked_mul(self.width)?.checked_add(x)?)
            .map(|value| *value != 0)
    }
}

impl fmt::Debug for DetectionMask {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DetectionMask")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

fn point_in_polygon(point: [f32; 2], polygon: &[[f32; 2]]) -> bool {
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for current in 0..polygon.len() {
        let [current_x, current_y] = polygon[current];
        let [previous_x, previous_y] = polygon[previous];
        if (current_y > point[1]) != (previous_y > point[1])
            && point[0]
                < (previous_x - current_x) * (point[1] - current_y) / (previous_y - current_y)
                    + current_x
        {
            inside = !inside;
        }
        previous = current;
    }
    inside
}
