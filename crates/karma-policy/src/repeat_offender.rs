use serde::{Deserialize, Serialize};

/// Rolling window in which closures accumulate toward a ban, and the
/// duration of the ban itself: one hour.
pub const REPEAT_OFFENDER_WINDOW_MS: i64 = 60 * 60 * 1000;
/// Closures within the window required to ban an executable.
pub const REPEAT_OFFENDER_STRIKE_LIMIT: usize = 3;
/// Upper bound on tracked executables so persisted state stays small.
const MAX_TRACKED_EXECUTABLES: usize = 64;

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

/// Records one completed closure and reports whether this closure just
/// established a ban. Closures of the same process instance count once, and
/// closures that happen while a ban is already active do not extend it, so
/// enforcement kills cannot keep a ban alive forever.
pub fn record_closure(
    records: &mut Vec<StrikeRecord>,
    executable: &str,
    process_id: u32,
    started_at_ms: i64,
    closed_at_ms: i64,
) -> bool {
    prune_expired(records, closed_at_ms);
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
        .retain(|stamp| *stamp > closed_at_ms.saturating_sub(REPEAT_OFFENDER_WINDOW_MS));
    if record.closed_at_ms.len() < REPEAT_OFFENDER_STRIKE_LIMIT {
        enforce_tracking_cap(records);
        return false;
    }
    record.banned_until_ms = Some(closed_at_ms.saturating_add(REPEAT_OFFENDER_WINDOW_MS));
    record.closed_at_ms.clear();
    enforce_tracking_cap(records);
    true
}

fn prune_expired(records: &mut Vec<StrikeRecord>, now_ms: i64) {
    records.retain(|record| {
        record.banned_until_ms.is_some_and(|until| until > now_ms)
            || record
                .closed_at_ms
                .iter()
                .any(|stamp| *stamp > now_ms.saturating_sub(REPEAT_OFFENDER_WINDOW_MS))
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
    use super::{
        MAX_TRACKED_EXECUTABLES, REPEAT_OFFENDER_WINDOW_MS, StrikeRecord, is_banned,
        normalize_executable, record_closure,
    };

    const GAME: &str = "d:\\games\\game.exe";

    #[test]
    fn three_closures_within_an_hour_ban_the_executable_for_one_hour() {
        let mut records = Vec::new();
        assert!(!record_closure(&mut records, GAME, 10, 1, 1_000));
        assert!(!is_banned(&records, GAME, 1_000));
        assert!(!record_closure(&mut records, GAME, 11, 2, 2_000));
        assert!(!is_banned(&records, GAME, 2_000));
        assert!(record_closure(&mut records, GAME, 12, 3, 3_000));
        assert!(is_banned(
            &records,
            GAME,
            3_000 + REPEAT_OFFENDER_WINDOW_MS - 1
        ));
        assert!(!is_banned(
            &records,
            GAME,
            3_000 + REPEAT_OFFENDER_WINDOW_MS
        ));
    }

    #[test]
    fn closures_more_than_an_hour_apart_never_accumulate_three_strikes() {
        let mut records = Vec::new();
        let hour = REPEAT_OFFENDER_WINDOW_MS;
        assert!(!record_closure(&mut records, GAME, 10, 1, hour));
        assert!(!record_closure(&mut records, GAME, 11, 2, 2 * hour));
        assert!(!record_closure(&mut records, GAME, 12, 3, 3 * hour));
        assert!(!is_banned(&records, GAME, 3 * hour));
        assert_eq!(
            records,
            vec![StrikeRecord {
                executable: GAME.into(),
                closed_at_ms: vec![3 * hour],
                banned_until_ms: None,
                last_process_id: Some(12),
                last_started_at_ms: Some(3),
            }]
        );
    }

    #[test]
    fn duplicate_report_for_one_process_instance_counts_once() {
        let mut records = Vec::new();
        assert!(!record_closure(&mut records, GAME, 10, 1, 1_000));
        assert!(!record_closure(&mut records, GAME, 10, 1, 1_100));
        assert!(!record_closure(&mut records, GAME, 11, 2, 2_000));
        assert!(!record_closure(&mut records, GAME, 11, 2, 2_100));
        assert!(!is_banned(&records, GAME, 2_100));
        assert!(record_closure(&mut records, GAME, 12, 3, 3_000));
        assert!(is_banned(&records, GAME, 3_000));
    }

    #[test]
    fn closures_during_a_ban_do_not_extend_it() {
        let mut records = Vec::new();
        for (pid, closed_at) in [(10, 1_000), (11, 2_000), (12, 3_000)] {
            record_closure(&mut records, GAME, pid, pid as i64, closed_at);
        }
        assert!(is_banned(&records, GAME, 3_000));
        // Relaunch attempts during the ban are killed but add no strikes,
        // so the ban still ends one hour after the third closure.
        assert!(!record_closure(&mut records, GAME, 20, 20, 3_500));
        assert!(is_banned(&records, GAME, 3_500));
        let ban_end = 3_000 + REPEAT_OFFENDER_WINDOW_MS;
        assert!(!is_banned(&records, GAME, ban_end));
        // After expiry a new ban needs three fresh closures.
        assert!(!record_closure(&mut records, GAME, 30, 30, ban_end + 1));
        assert!(!record_closure(&mut records, GAME, 31, 31, ban_end + 2));
        assert!(!is_banned(&records, GAME, ban_end + 2));
        assert!(record_closure(&mut records, GAME, 32, 32, ban_end + 3));
        assert!(is_banned(&records, GAME, ban_end + 4));
    }

    #[test]
    fn path_normalization_matches_case_and_separator_variants() {
        let mut records = Vec::new();
        record_closure(&mut records, GAME, 10, 1, 1_000);
        record_closure(
            &mut records,
            &normalize_executable("D:/Games/GAME.EXE"),
            11,
            2,
            2_000,
        );
        assert!(record_closure(
            &mut records,
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
        let hour = REPEAT_OFFENDER_WINDOW_MS;
        // Expired histories disappear once later activity prunes the table.
        for index in 0..u32::try_from(MAX_TRACKED_EXECUTABLES).unwrap() {
            let executable = format!("d:\\games\\game{index}.exe");
            record_closure(
                &mut records,
                &executable,
                index,
                1,
                1_000 + i64::from(index),
            );
        }
        assert_eq!(records.len(), MAX_TRACKED_EXECUTABLES);
        record_closure(&mut records, "e:\\fresh\\fresh.exe", 1, 1, 1_000 + 4 * hour);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].executable, "e:\\fresh\\fresh.exe");
        // The capped table keeps the newest activity and drops the stalest.
        let mut crowded = Vec::new();
        for index in 0..u32::try_from(MAX_TRACKED_EXECUTABLES).unwrap() {
            let executable = format!("d:\\games\\game{index}.exe");
            record_closure(
                &mut crowded,
                &executable,
                index,
                1,
                1_000 + i64::from(index),
            );
        }
        assert!(!record_closure(
            &mut crowded,
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
