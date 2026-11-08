//! Daily time windows for single Android applications
//! (`policy.android.appSchedules`).
//!
//! Each rule names one package and either the windows in which it may be used
//! (`"mode": "allow"`, closed at every other minute) or the windows in which it
//! is closed (`"mode": "block"`). Times are local wall-clock `HH:MM`; a window
//! whose end is not after its start continues past midnight, so `22:00–06:30`
//! covers the night.

use serde::Deserialize;
use serde_json::Value;

use crate::packages::normalize_package;
use crate::types::CoreError;

const MINUTES_PER_DAY: u16 = 1440;
const MAX_RULES: usize = 64;
const MAX_WINDOWS_PER_RULE: usize = 8;
const MAX_NAME_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    /// The package may be used only inside the windows.
    Allow,
    /// The package is closed inside the windows.
    Block,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleInput {
    package: String,
    #[serde(default)]
    name: String,
    mode: Mode,
    windows: Vec<WindowInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowInput {
    start: String,
    end: String,
}

/// A half-open span of `length` minutes starting at `start`, wrapping past
/// midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DailyWindow {
    start: u16,
    length: u16,
}

impl DailyWindow {
    fn contains(self, minute_of_day: u16) -> bool {
        (minute_of_day + MINUTES_PER_DAY - self.start) % MINUTES_PER_DAY < self.length
    }
}

#[derive(Debug, Clone)]
struct Rule {
    package: String,
    mode: Mode,
    windows: Vec<DailyWindow>,
}

impl Rule {
    fn closes_at(&self, minute_of_day: u16) -> bool {
        let inside = self
            .windows
            .iter()
            .any(|window| window.contains(minute_of_day));
        match self.mode {
            Mode::Allow => !inside,
            Mode::Block => inside,
        }
    }
}

/// The compiled `appSchedules` of one policy.
#[derive(Debug, Clone, Default)]
pub struct AppSchedules {
    rules: Vec<Rule>,
}

impl AppSchedules {
    pub fn from_value(android: Option<&Value>) -> Result<Self, CoreError> {
        let Some(raw) = android.and_then(|android| android.get("appSchedules")) else {
            return Ok(Self::default());
        };
        let inputs: Vec<RuleInput> =
            serde_json::from_value(raw.clone()).map_err(|_| CoreError::InvalidPolicy)?;
        if inputs.len() > MAX_RULES {
            return Err(CoreError::InvalidPolicy);
        }
        let mut rules: Vec<Rule> = Vec::with_capacity(inputs.len());
        for input in inputs {
            let package = normalize_package(&input.package).ok_or(CoreError::InvalidPolicy)?;
            if input.name.chars().count() > MAX_NAME_CHARS
                || input.windows.is_empty()
                || input.windows.len() > MAX_WINDOWS_PER_RULE
                || rules.iter().any(|rule| rule.package == package)
            {
                return Err(CoreError::InvalidPolicy);
            }
            let windows = input
                .windows
                .iter()
                .map(|window| daily_window(&window.start, &window.end))
                .collect::<Result<Vec<_>, _>>()?;
            rules.push(Rule {
                package,
                mode: input.mode,
                windows,
            });
        }
        Ok(Self { rules })
    }

    /// Minutes from `minute_of_day` until a rule stops closing `package`
    /// (at least 1), or `None` when no rule closes it now. A rule that closes
    /// the package all day reports the rest of the day plus a full day.
    pub fn closed_for(&self, package: &str, minute_of_day: u16) -> Option<u16> {
        let rule = self.rules.iter().find(|rule| rule.package == package)?;
        let minute = minute_of_day % MINUTES_PER_DAY;
        if !rule.closes_at(minute) {
            return None;
        }
        Some(
            (1..MINUTES_PER_DAY)
                .find(|step| !rule.closes_at((minute + step) % MINUTES_PER_DAY))
                .unwrap_or(MINUTES_PER_DAY),
        )
    }
}

fn daily_window(start: &str, end: &str) -> Result<DailyWindow, CoreError> {
    let start = minute_of_day(start)?;
    let end = minute_of_day(end)?;
    if start == end {
        return Err(CoreError::InvalidPolicy);
    }
    Ok(DailyWindow {
        start,
        length: (end + MINUTES_PER_DAY - start) % MINUTES_PER_DAY,
    })
}

fn minute_of_day(value: &str) -> Result<u16, CoreError> {
    let bytes = value.as_bytes();
    if bytes.len() != 5 || bytes[2] != b':' {
        return Err(CoreError::InvalidPolicy);
    }
    let number = |digits: &[u8]| -> Option<u16> {
        digits.iter().try_fold(0u16, |total, digit| {
            digit
                .is_ascii_digit()
                .then(|| total * 10 + u16::from(digit - b'0'))
        })
    };
    match (number(&bytes[..2]), number(&bytes[3..])) {
        (Some(hour), Some(minute)) if hour < 24 && minute < 60 => Ok(hour * 60 + minute),
        _ => Err(CoreError::InvalidPolicy),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn schedules(rules: Value) -> AppSchedules {
        AppSchedules::from_value(Some(&json!({ "appSchedules": rules }))).unwrap()
    }

    const fn at(hour: u16, minute: u16) -> u16 {
        hour * 60 + minute
    }

    #[test]
    fn allow_windows_close_the_app_outside_them_to_the_minute() {
        let rules = schedules(json!([{
            "package": "com.example.game", "name": "Game", "mode": "allow",
            "windows": [{"start": "19:30", "end": "20:15"}]
        }]));
        assert_eq!(rules.closed_for("com.example.game", at(19, 29)), Some(1));
        assert_eq!(rules.closed_for("com.example.game", at(19, 30)), None);
        assert_eq!(rules.closed_for("com.example.game", at(20, 14)), None);
        // Closed from 20:15 until 19:30 the next day.
        assert_eq!(rules.closed_for("com.example.game", at(20, 15)), Some(1395));
        assert_eq!(rules.closed_for("com.example.other", at(8, 0)), None);
    }

    #[test]
    fn block_windows_close_the_app_inside_them_across_midnight() {
        let rules = schedules(json!([{
            "package": "com.example.video", "mode": "block",
            "windows": [{"start": "12:00", "end": "13:00"}, {"start": "22:00", "end": "06:30"}]
        }]));
        assert_eq!(rules.closed_for("com.example.video", at(11, 59)), None);
        assert_eq!(rules.closed_for("com.example.video", at(12, 0)), Some(60));
        assert_eq!(rules.closed_for("com.example.video", at(13, 0)), None);
        assert_eq!(rules.closed_for("com.example.video", at(23, 0)), Some(450));
        assert_eq!(rules.closed_for("com.example.video", at(6, 29)), Some(1));
        assert_eq!(rules.closed_for("com.example.video", at(6, 30)), None);
    }

    #[test]
    fn windows_that_cover_the_whole_day_never_lift() {
        let rules = schedules(json!([{
            "package": "com.example.chat", "mode": "block",
            "windows": [{"start": "00:00", "end": "12:00"}, {"start": "12:00", "end": "00:00"}]
        }]));
        assert_eq!(rules.closed_for("com.example.chat", at(9, 0)), Some(1440));
    }

    #[test]
    fn package_names_are_matched_case_insensitively() {
        let rules = schedules(json!([{
            "package": "Com.Example.Game", "mode": "block",
            "windows": [{"start": "00:00", "end": "23:59"}]
        }]));
        assert!(rules.closed_for("com.example.game", at(1, 0)).is_some());
    }

    #[test]
    fn missing_schedules_are_empty() {
        let none = AppSchedules::from_value(None).unwrap();
        assert_eq!(none.closed_for("com.example.game", 0), None);
        let android = AppSchedules::from_value(Some(&json!({"cooldownSeconds": 60}))).unwrap();
        assert_eq!(android.closed_for("com.example.game", 0), None);
    }

    #[test]
    fn malformed_rules_are_rejected() {
        let window = json!([{"start": "08:00", "end": "09:00"}]);
        for rules in [
            json!({}),
            json!([{"package": "com.example.a", "mode": "sometimes", "windows": window}]),
            json!([{"package": "", "mode": "block", "windows": window}]),
            json!([{"package": "com.example.a", "mode": "block", "windows": []}]),
            json!([{"package": "com.example.a", "mode": "block", "windows": [{"start": "8:00", "end": "09:00"}]}]),
            json!([{"package": "com.example.a", "mode": "block", "windows": [{"start": "24:00", "end": "09:00"}]}]),
            json!([{"package": "com.example.a", "mode": "block", "windows": [{"start": "08:60", "end": "09:00"}]}]),
            json!([{"package": "com.example.a", "mode": "block", "windows": [{"start": "+8:00", "end": "09:00"}]}]),
            json!([{"package": "com.example.a", "mode": "block", "windows": [{"start": "09:00", "end": "09:00"}]}]),
            json!([{"package": "com.example.a", "mode": "block", "windows": window, "extra": true}]),
            json!([
                {"package": "com.example.a", "mode": "block", "windows": window},
                {"package": "COM.EXAMPLE.A", "mode": "allow", "windows": window}
            ]),
            json!([{"package": "com.example.a", "mode": "block", "windows": vec![json!({"start": "08:00", "end": "09:00"}); 9]}]),
        ] {
            assert!(
                AppSchedules::from_value(Some(&json!({ "appSchedules": rules }))).is_err(),
                "{rules}"
            );
        }
    }
}
