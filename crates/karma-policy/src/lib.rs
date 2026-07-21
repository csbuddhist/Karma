#![forbid(unsafe_code)]

mod schedule;

pub use schedule::{MinuteRange, ScheduleError, WeeklySchedule};
