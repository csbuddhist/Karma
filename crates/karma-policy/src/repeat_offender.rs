use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Default rolling window in which closures accumulate toward a ban.
pub const DEFAULT_REPEAT_OFFENDER_WINDOW_MINUTES: u32 = 20;
/// Default closures within the window required to ban an executable.
pub const DEFAULT_REPEAT_OFFENDER_STRIKE_LIMIT: u32 = 5;
/// How long a ban lasts once established: one hour.
pub const REPEAT_OFFENDER_BAN_MS: i64 = 60 * 60 * 1000;
pub const MIN_REPEAT_OFFENDER_WINDOW_MINUTES: u32 = 1;
pub const MAX_REPEAT_OFFENDER_WINDOW_MINUTES: u32 = 24 * 60;
pub const MIN_REPEAT_OFFENDER_STRIKE_LIMIT: u32 = 2;
pub const MAX_REPEAT_OFFENDER_STRIKE_LIMIT: u32 = 20;
/// Upper bound on tracked executables so persisted state stays small.
const MAX_TRACKED_EXECUTABLES: usize = 64;
const MS_PER_MINUTE: i64 = 60 * 1000;

/// Console-managed repeat-offender thresholds (`policy.repeatOffender`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepeatOffenderRule {
    pub window_ms: i64,
    pub strike_limit: usize,
    pub ban_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RepeatOffenderError {
    #[error("repeat-offender policy is invalid")]
    InvalidPolicy,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RepeatOffenderInput {
    window_minutes: u32,
    strike_limit: u32,
}

impl Default for RepeatOffenderRule {
    fn default() -> Self {
        Self::new(
            DEFAULT_REPEAT_OFFENDER_WINDOW_MINUTES,
            DEFAULT_REPEAT_OFFENDER_STRIKE_LIMIT,
        )
    }
}

impl RepeatOffenderRule {
    fn new(window_minutes: u32, strike_limit: u32) -> Self {
        Self {
            window_ms: i64::from(window_minutes) * MS_PER_MINUTE,
            strike_limit: strike_limit as usize,
            ban_ms: REPEAT_OFFENDER_BAN_MS,
        }
    }

    /// Reads `policy.repeatOffender`; an absent section selects the defaults.
    pub fn from_value(policy: &Value) -> Result<Self, RepeatOffenderError> {
        let Some(raw) = policy.get("repeatOffender") else {
            return Ok(Self::default());
        };
        let input: RepeatOffenderInput =
            serde_json::from_value(raw.clone()).map_err(|_| RepeatOffenderError::InvalidPolicy)?;
        if !(MIN_REPEAT_OFFENDER_WINDOW_MINUTES..=MAX_REPEAT_OFFENDER_WINDOW_MINUTES)
            .contains(&input.window_minutes)
            || !(MIN_REPEAT_OFFENDER_STRIKE_LIMIT..=MAX_REPEAT_OFFENDER_STRIKE_LIMIT)
                .contains(&input.strike_limit)
        {
            return Err(RepeatOffenderError::InvalidPolicy);
        }
        Ok(Self::new(input.window_minutes, input.strike_limit))
    }
}

/// Closure history for one executable. Persisted by the service so a ban
/// survives restarts and reboots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrikeRecord {
    pub executable: String,
    /// Timestamps of the most recent closures, ascending. Cleared once a
    /// ban is established.
    pub closed_at_ms: Vec<i64>,
    pub banned_until_ms: Option<i64>,
    /// Instance identity of the last counted closure, so a duplicate report
    /// for one process cannot add several strikes.
    pub last_process_id: Option<u32>,
    pub last_started_at_ms: Option<i64>,
}

/// Case-insensitive, separator-normalized identity of an executable path.
pub fn normalize_executable(executable_path: &str) -> String {
    executable_path.trim().replace('/', "\\").to_lowercase()
}

/// Whether the executable is banned at `now_ms` and must be terminated
/// immediately whenever it is observed.
pub fn is_banned(records: &[StrikeRecord], executable: &str, now_ms: i64) -> bool {
    records.iter().any(|record| {
        record.executable == executable
            && record.banned_until_ms.is_some_and(|until| until > now_ms)
    })
}

/// Executables banned at `now_ms`, with the instant each ban ends.
pub fn active_bans(records: &[StrikeRecord], now_ms: i64) -> impl Iterator<Item = (&str, i64)> {
    records.iter().filter_map(move |record| {
        record
            .banned_until_ms
            .filter(|until| *until > now_ms)
            .map(|until| (record.executable.as_str(), until))
    })
}

/// Records one completed closure and reports whether this closure just
/// established a ban. Closures of the same process instance count once, and
/// closures that happen while a ban is already active do not extend it, so
/// enforcement kills cannot keep a ban alive forever.
pub fn record_closure(
    records: &mut Vec<StrikeRecord>,
    rule: RepeatOffenderRule,
    executable: &str,
    process_id: u32,
    started_at_ms: i64,
    closed_at_ms: i64,
) -> bool {
    prune_expired(records, rule.window_ms, closed_at_ms);
    let position = match records
        .iter()
        .position(|record| record.executable == executable)
    {
        Some(position) => position,
        None => {
            records.push(StrikeRecord {
                executable: executable.to_owned(),
                closed_at_ms: Vec::new(),
                banned_until_ms: None,
                last_process_id: None,
                last_started_at_ms: None,
            });
            records.len() - 1
        }
    };
    let record = &mut records[position];
    if record.last_process_id == Some(process_id)
        && record.last_started_at_ms == Some(started_at_ms)
    {
        return false;
    }
    record.last_process_id = Some(process_id);
    record.last_started_at_ms = Some(started_at_ms);
    if record
        .banned_until_ms
        .is_some_and(|until| until > closed_at_ms)
    {
        return false;
    }
    // An expired ban resets the history: a fresh ban needs fresh closures.
    record.banned_until_ms = None;
    record.closed_at_ms.push(closed_at_ms);
    record
        .closed_at_ms
        .retain(|stamp| *stamp > closed_at_ms.saturating_sub(rule.window_ms));
    if record.closed_at_ms.len() < rule.strike_limit {
        enforce_tracking_cap(records);
        return false;
    }
    record.banned_until_ms = Some(closed_at_ms.saturating_add(rule.ban_ms));
    record.closed_at_ms.clear();
    enforce_tracking_cap(records);
    true
}

fn prune_expired(records: &mut Vec<StrikeRecord>, window_ms: i64, now_ms: i64) {
    records.retain(|record| {
        record.banned_until_ms.is_some_and(|until| until > now_ms)
            || record
                .closed_at_ms
                .iter()
                .any(|stamp| *stamp > now_ms.saturating_sub(window_ms))
    });
}

fn enforce_tracking_cap(records: &mut Vec<StrikeRecord>) {
    if records.len() <= MAX_TRACKED_EXECUTABLES {
        return;
    }
    // Keep the most recently active records and drop the stalest ones.
    records.sort_by_key(latest_activity);
    let excess = records.len() - MAX_TRACKED_EXECUTABLES;
    records.drain(..excess);
}

fn latest_activity(record: &StrikeRecord) -> i64 {
    record
        .banned_until_ms
        .unwrap_or(i64::MIN)
        .max(record.closed_at_ms.last().copied().unwrap_or(i64::MIN))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        MAX_TRACKED_EXECUTABLES, REPEAT_OFFENDER_BAN_MS, RepeatOffenderRule, StrikeRecord,
        active_bans, is_banned, normalize_executable, record_closure,
    };

    const GAME: &str = "d:\\games\\game.exe";
    const HOUR: i64 = 60 * 60 * 1000;
    const MINUTE: i64 = 60 * 1000;
    /// Three closures within one hour ban for one hour.
    const STRICT: RepeatOffenderRule = RepeatOffenderRule {
        window_ms: HOUR,
        strike_limit: 3,
        ban_ms: HOUR,
    };

    #[test]
    fn default_rule_bans_after_five_closures_within_twenty_minutes_for_one_hour() {
        let rule = RepeatOffenderRule::default();
        assert_eq!(rule.window_ms, 20 * MINUTE);
        assert_eq!(rule.strike_limit, 5);
        assert_eq!(rule.ban_ms, REPEAT_OFFENDER_BAN_MS);
        let mut records = Vec::new();
        for index in 0..4_u32 {
            let at = 1_000 + i64::from(index) * 4 * MINUTE;
            assert!(!record_closure(&mut records, rule, GAME, index, 1, at));
        }
        assert!(!is_banned(&records, GAME, 1_000 + 12 * MINUTE));
        let fifth = 1_000 + 16 * MINUTE;
        assert!(record_closure(&mut records, rule, GAME, 9, 1, fifth));
        assert!(is_banned(&records, GAME, fifth + HOUR - 1));
        assert!(!is_banned(&records, GAME, fifth + HOUR));
        assert_eq!(
            active_bans(&records, fifth).collect::<Vec<_>>(),
            vec![(GAME, fifth + HOUR)]
        );
        assert_eq!(active_bans(&records, fifth + HOUR).count(), 0);
    }

    #[test]
    fn five_closures_spread_over_more_than_the_window_do_not_ban() {
        let rule = RepeatOffenderRule::default();
        let mut records = Vec::new();
        for index in 0..5_u32 {
            let at = 1_000 + i64::from(index) * 6 * MINUTE;
            assert!(!record_closure(&mut records, rule, GAME, index, 1, at));
        }
        assert!(!is_banned(&records, GAME, 1_000 + 24 * MINUTE));
    }

    #[test]
    fn policy_section_configures_window_and_strike_limit_within_bounds() {
        assert_eq!(
            RepeatOffenderRule::from_value(&json!({})).unwrap(),
            RepeatOffenderRule::default()
        );
        let rule = RepeatOffenderRule::from_value(
            &json!({"repeatOffender": {"windowMinutes": 45, "strikeLimit": 3}}),
        )
        .unwrap();
        assert_eq!(rule.window_ms, 45 * MINUTE);
        assert_eq!(rule.strike_limit, 3);
        assert_eq!(rule.ban_ms, HOUR);
        for invalid in [
            json!({"repeatOffender": {"windowMinutes": 0, "strikeLimit": 5}}),
            json!({"repeatOffender": {"windowMinutes": 1441, "strikeLimit": 5}}),
            json!({"repeatOffender": {"windowMinutes": 20, "strikeLimit": 1}}),
            json!({"repeatOffender": {"windowMinutes": 20, "strikeLimit": 21}}),
            json!({"repeatOffender": {"windowMinutes": 20}}),
            json!({"repeatOffender": {"windowMinutes": 20, "strikeLimit": 5, "extra": 1}}),
            json!({"repeatOffender": "20"}),
        ] {
            assert!(
                RepeatOffenderRule::from_value(&invalid).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn three_closures_within_an_hour_ban_the_executable_for_one_hour() {
        let mut records = Vec::new();
        assert!(!record_closure(&mut records, STRICT, GAME, 10, 1, 1_000));
        assert!(!is_banned(&records, GAME, 1_000));
        assert!(!record_closure(&mut records, STRICT, GAME, 11, 2, 2_000));
        assert!(!is_banned(&records, GAME, 2_000));
        assert!(record_closure(&mut records, STRICT, GAME, 12, 3, 3_000));
        assert!(is_banned(&records, GAME, 3_000 + HOUR - 1));
        assert!(!is_banned(&records, GAME, 3_000 + HOUR));
    }

    #[test]
    fn closures_more_than_an_hour_apart_never_accumulate_three_strikes() {
        let mut records = Vec::new();
        assert!(!record_closure(&mut records, STRICT, GAME, 10, 1, HOUR));
        assert!(!record_closure(&mut records, STRICT, GAME, 11, 2, 2 * HOUR));
        assert!(!record_closure(&mut records, STRICT, GAME, 12, 3, 3 * HOUR));
        assert!(!is_banned(&records, GAME, 3 * HOUR));
        assert_eq!(
            records,
            vec![StrikeRecord {
                executable: GAME.into(),
                closed_at_ms: vec![3 * HOUR],
                banned_until_ms: None,
                last_process_id: Some(12),
                last_started_at_ms: Some(3),
            }]
        );
    }

    #[test]
    fn duplicate_report_for_one_process_instance_counts_once() {
        let mut records = Vec::new();
        assert!(!record_closure(&mut records, STRICT, GAME, 10, 1, 1_000));
        assert!(!record_closure(&mut records, STRICT, GAME, 10, 1, 1_100));
        assert!(!record_closure(&mut records, STRICT, GAME, 11, 2, 2_000));
        assert!(!record_closure(&mut records, STRICT, GAME, 11, 2, 2_100));
        assert!(!is_banned(&records, GAME, 2_100));
        assert!(record_closure(&mut records, STRICT, GAME, 12, 3, 3_000));
        assert!(is_banned(&records, GAME, 3_000));
    }

    #[test]
    fn closures_during_a_ban_do_not_extend_it() {
        let mut records = Vec::new();
        for (pid, closed_at) in [(10, 1_000), (11, 2_000), (12, 3_000)] {
            record_closure(&mut records, STRICT, GAME, pid, pid as i64, closed_at);
        }
        assert!(is_banned(&records, GAME, 3_000));
        // Relaunch attempts during the ban are killed but add no strikes,
        // so the ban still ends one hour after the third closure.
        assert!(!record_closure(&mut records, STRICT, GAME, 20, 20, 3_500));
        assert!(is_banned(&records, GAME, 3_500));
        let ban_end = 3_000 + HOUR;
        assert!(!is_banned(&records, GAME, ban_end));
        // After expiry a new ban needs three fresh closures.
        assert!(!record_closure(
            &mut records,
            STRICT,
            GAME,
            30,
            30,
            ban_end + 1
        ));
        assert!(!record_closure(
            &mut records,
            STRICT,
            GAME,
            31,
            31,
            ban_end + 2
        ));
        assert!(!is_banned(&records, GAME, ban_end + 2));
        assert!(record_closure(
            &mut records,
            STRICT,
            GAME,
            32,
            32,
            ban_end + 3
        ));
        assert!(is_banned(&records, GAME, ban_end + 4));
    }

    #[test]
    fn path_normalization_matches_case_and_separator_variants() {
        let mut records = Vec::new();
        record_closure(&mut records, STRICT, GAME, 10, 1, 1_000);
        record_closure(
            &mut records,
            STRICT,
            &normalize_executable("D:/Games/GAME.EXE"),
            11,
            2,
            2_000,
        );
        assert!(record_closure(
            &mut records,
            STRICT,
            &normalize_executable(r"D:\Games\Game.exe"),
            12,
            3,
            3_000
        ));
        assert!(is_banned(
            &records,
            &normalize_executable("d:/games/game.exe"),
            3_000
        ));
        // A different executable is unaffected.
        assert!(!is_banned(
            &records,
            &normalize_executable("d:/games/other.exe"),
            3_000
        ));
    }

    #[test]
    fn stale_histories_are_pruned_and_the_cap_bounds_tracking() {
        let mut records = Vec::new();
        // Expired histories disappear once later activity prunes the table.
        for index in 0..u32::try_from(MAX_TRACKED_EXECUTABLES).unwrap() {
            let executable = format!("d:\\games\\game{index}.exe");
            record_closure(
                &mut records,
                STRICT,
                &executable,
                index,
                1,
                1_000 + i64::from(index),
            );
        }
        assert_eq!(records.len(), MAX_TRACKED_EXECUTABLES);
        record_closure(
            &mut records,
            STRICT,
            "e:\\fresh\\fresh.exe",
            1,
            1,
            1_000 + 4 * HOUR,
        );
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].executable, "e:\\fresh\\fresh.exe");
        // The capped table keeps the newest activity and drops the stalest.
        let mut crowded = Vec::new();
        for index in 0..u32::try_from(MAX_TRACKED_EXECUTABLES).unwrap() {
            let executable = format!("d:\\games\\game{index}.exe");
            record_closure(
                &mut crowded,
                STRICT,
                &executable,
                index,
                1,
                1_000 + i64::from(index),
            );
        }
        assert!(!record_closure(
            &mut crowded,
            STRICT,
            "e:\\fresh\\fresh.exe",
            1,
            1,
            2_000
        ));
        assert_eq!(crowded.len(), MAX_TRACKED_EXECUTABLES);
        assert!(
            crowded
                .iter()
                .any(|record| record.executable == "e:\\fresh\\fresh.exe")
        );
        assert!(
            !crowded
                .iter()
                .any(|record| record.executable == "d:\\games\\game0.exe")
        );
    }
}
