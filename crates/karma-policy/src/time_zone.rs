//! The real-life time zone a super administrator pins for every time-based
//! setting, so schedules keep their wall-clock meaning even when the
//! operating system's time zone is changed.

use serde_json::Value;
use thiserror::Error;

/// UTC−12:00, the westernmost civil offset.
pub const MIN_UTC_OFFSET_MINUTES: i16 = -12 * 60;
/// UTC+14:00, the easternmost civil offset.
pub const MAX_UTC_OFFSET_MINUTES: i16 = 14 * 60;
/// Every civil offset in use is a whole number of quarter hours.
pub const UTC_OFFSET_STEP_MINUTES: i16 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TimeZoneError {
    #[error("timeZone must be an object")]
    NotAnObject,
    #[error("realUtcOffsetMinutes must be null or a whole quarter hour between -720 and 840")]
    InvalidOffset,
}

/// The `timeZone` policy section. Offsets count minutes east of UTC, so
/// UTC+08:00 is `480` and UTC−05:00 is `-300`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimeZonePolicy {
    /// `None` until a super administrator pins the real-life time zone;
    /// time-based settings then follow the operating system's zone.
    pub real_utc_offset_minutes: Option<i16>,
}

impl TimeZonePolicy {
    /// Reads the `timeZone` section of a policy. A missing section or a
    /// `null` offset means no real-life time zone has been set.
    pub fn from_value(policy: &Value) -> Result<Self, TimeZoneError> {
        let Some(section) = policy.get("timeZone") else {
            return Ok(Self::default());
        };
        if section.is_null() {
            return Ok(Self::default());
        }
        let section = section.as_object().ok_or(TimeZoneError::NotAnObject)?;
        let real_utc_offset_minutes = match section.get("realUtcOffsetMinutes") {
            None | Some(Value::Null) => None,
            Some(value) => Some(parse_offset(value)?),
        };
        Ok(Self {
            real_utc_offset_minutes,
        })
    }

    /// The UTC offset in seconds every time-based setting must use: the
    /// real-life time zone when set, otherwise `system_offset_seconds`.
    pub fn effective_offset_seconds(&self, system_offset_seconds: i32) -> i32 {
        self.real_utc_offset_minutes
            .map_or(system_offset_seconds, |minutes| i32::from(minutes) * 60)
    }
}

fn parse_offset(value: &Value) -> Result<i16, TimeZoneError> {
    let minutes = value
        .as_i64()
        .and_then(|minutes| i16::try_from(minutes).ok())
        .ok_or(TimeZoneError::InvalidOffset)?;
    if !(MIN_UTC_OFFSET_MINUTES..=MAX_UTC_OFFSET_MINUTES).contains(&minutes)
        || minutes % UTC_OFFSET_STEP_MINUTES != 0
    {
        return Err(TimeZoneError::InvalidOffset);
    }
    Ok(minutes)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn offset(policy: Value) -> Result<Option<i16>, TimeZoneError> {
        TimeZonePolicy::from_value(&policy).map(|zone| zone.real_utc_offset_minutes)
    }

    #[test]
    fn missing_or_null_time_zone_follows_the_system() {
        assert_eq!(offset(json!({})), Ok(None));
        assert_eq!(offset(json!({"timeZone": null})), Ok(None));
        assert_eq!(offset(json!({"timeZone": {}})), Ok(None));
        assert_eq!(
            offset(json!({"timeZone": {"realUtcOffsetMinutes": null}})),
            Ok(None)
        );
        assert_eq!(
            TimeZonePolicy::default().effective_offset_seconds(-18_000),
            -18_000
        );
    }

    #[test]
    fn real_time_zone_overrides_the_system_offset() {
        let zone = TimeZonePolicy::from_value(&json!({"timeZone": {"realUtcOffsetMinutes": 480}}))
            .unwrap();
        assert_eq!(zone.real_utc_offset_minutes, Some(480));
        assert_eq!(zone.effective_offset_seconds(-18_000), 28_800);
        for minutes in [-720, -300, 0, 330, 345, 840] {
            assert_eq!(
                offset(json!({"timeZone": {"realUtcOffsetMinutes": minutes}})),
                Ok(Some(minutes as i16))
            );
        }
    }

    #[test]
    fn out_of_range_or_odd_offsets_are_rejected() {
        for value in [
            json!(-735),
            json!(855),
            json!(7),
            json!(30.5),
            json!("480"),
            json!(true),
            json!(100_000),
        ] {
            assert_eq!(
                offset(json!({"timeZone": {"realUtcOffsetMinutes": value}})),
                Err(TimeZoneError::InvalidOffset)
            );
        }
        assert_eq!(
            offset(json!({"timeZone": 480})),
            Err(TimeZoneError::NotAnObject)
        );
    }
}
