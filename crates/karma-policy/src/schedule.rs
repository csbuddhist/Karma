use serde::{Deserialize, Serialize};
use thiserror::Error;

const MINUTES_PER_WEEK: u16 = 10080;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinuteRange {
    pub start: u16,
    pub end: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduleError {
    #[error("minute is outside the week")]
    OutOfRange,
    #[error("schedule boundaries must align to 15 minutes")]
    NotQuarterHourAligned,
    #[error("empty ranges are not allowed")]
    EmptyRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeeklySchedule {
    pub id: String,
    ranges: Vec<MinuteRange>,
}

impl WeeklySchedule {
    pub fn new(id: impl Into<String>, ranges: Vec<MinuteRange>) -> Result<Self, ScheduleError> {
        for range in &ranges {
            if range.start >= MINUTES_PER_WEEK || range.end > MINUTES_PER_WEEK {
                return Err(ScheduleError::OutOfRange);
            }
            if range.start % 15 != 0 || range.end % 15 != 0 {
                return Err(ScheduleError::NotQuarterHourAligned);
            }
            if range.start == range.end {
                return Err(ScheduleError::EmptyRange);
            }
        }

        Ok(Self {
            id: id.into(),
            ranges,
        })
    }

    pub fn is_blocked(&self, minute: u16) -> bool {
        minute < MINUTES_PER_WEEK
            && self.ranges.iter().any(|range| {
                if range.start < range.end {
                    (range.start..range.end).contains(&minute)
                } else {
                    minute >= range.start || minute < range.end
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_is_start_inclusive_and_end_exclusive() {
        let value = WeeklySchedule::new(
            "bedtime",
            vec![MinuteRange {
                start: 120,
                end: 180,
            }],
        )
        .unwrap();

        assert!(value.is_blocked(120));
        assert!(value.is_blocked(179));
        assert!(!value.is_blocked(180));
    }

    #[test]
    fn rejects_non_quarter_hour_boundaries() {
        let error =
            WeeklySchedule::new("bad", vec![MinuteRange { start: 1, end: 30 }]).unwrap_err();

        assert_eq!(error, ScheduleError::NotQuarterHourAligned);
    }

    #[test]
    fn supports_week_boundary_wraparound() {
        let value = WeeklySchedule::new(
            "weekend",
            vec![MinuteRange {
                start: 10020,
                end: 60,
            }],
        )
        .unwrap();

        assert!(value.is_blocked(10050));
        assert!(value.is_blocked(30));
        assert!(!value.is_blocked(600));
    }
}
