use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// Upper bound on remembered titles so the agent's digest list stays well
/// inside one IPC frame; the oldest entries are forgotten first.
pub const MAX_REMEMBERED_TITLES: usize = 5000;
/// Bytes of SHA-256 kept in a title digest; 128 bits make collisions among
/// a few thousand titles negligible.
const TITLE_DIGEST_BYTES: usize = 16;
/// Titles shorter than this after normalization are too generic to remember.
const MIN_TITLE_KEY_CHARS: usize = 2;
/// Upper bound on titles marked as misjudged; the oldest are dropped first,
/// which only lets them be remembered again.
pub const MAX_MISJUDGED_TITLES: usize = 1000;

/// Why a title was remembered: the image recognition result that closed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TitleReason {
    pub reason_code: String,
    pub risk_millis: u16,
    /// The enforcement threshold in effect at that moment.
    pub threshold_millis: u16,
}

/// A window title that was open when image recognition closed its
/// application. Persisted by the service; a later window of the same
/// executable showing the same title is closed on sight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RememberedTitle {
    pub id: String,
    /// Normalized executable path (see `normalize_executable`).
    pub executable: String,
    /// The title as it was observed, for display in the console.
    pub title: String,
    pub browser_host: Option<String>,
    pub remembered_at_ms: i64,
    /// Absent for titles remembered before reasons were recorded.
    #[serde(default)]
    pub reason: Option<TitleReason>,
}

/// Case-, width- and whitespace-insensitive identity of a window title.
pub fn title_key(title: &str) -> String {
    title
        .nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The matching key for `title`, or `None` when the title is too generic to
/// remember: empty, very short, or only the application's own name.
pub fn rememberable_title_key(executable: &str, title: &str) -> Option<String> {
    let key = title_key(title);
    if key.chars().count() < MIN_TITLE_KEY_CHARS {
        return None;
    }
    let file_name = executable.rsplit(['\\', '/']).next().unwrap_or(executable);
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    if key == title_key(stem) || key == title_key(file_name) {
        return None;
    }
    Some(key)
}

/// Whether `recognition.rememberTitlesEnabled` is on (the default).
pub fn remember_titles_enabled(policy: &Value) -> bool {
    policy
        .pointer("/recognition/rememberTitlesEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// Remembers `entry` and reports whether it is new. Re-remembering the same
/// executable and title refreshes its timestamp instead of adding a copy.
pub fn remember_title(records: &mut Vec<RememberedTitle>, entry: RememberedTitle) -> bool {
    let key = title_key(&entry.title);
    if let Some(existing) = records
        .iter_mut()
        .find(|record| record.executable == entry.executable && title_key(&record.title) == key)
    {
        existing.remembered_at_ms = entry.remembered_at_ms;
        existing.browser_host = entry.browser_host;
        existing.reason = entry.reason;
        return false;
    }
    records.push(entry);
    if records.len() > MAX_REMEMBERED_TITLES {
        records.sort_by_key(|record| record.remembered_at_ms);
        let excess = records.len() - MAX_REMEMBERED_TITLES;
        records.drain(..excess);
    }
    true
}

/// Opaque matching identity of `title` shown by `executable` (normalized):
/// lowercase hex of a truncated SHA-256 over the executable and title key.
/// The agent matches foreground windows against digests, so remembered
/// titles never leave the service in plaintext. `None` for an empty title.
pub fn title_digest(executable: &str, title: &str) -> Option<String> {
    let key = title_key(title);
    if key.is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(executable.as_bytes());
    hasher.update([0]);
    hasher.update(key.as_bytes());
    let digest = hasher.finalize();
    Some(digest[..TITLE_DIGEST_BYTES].iter().fold(
        String::with_capacity(TITLE_DIGEST_BYTES * 2),
        |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        },
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        MAX_REMEMBERED_TITLES, RememberedTitle, remember_title, remember_titles_enabled,
        rememberable_title_key, title_digest, title_key,
    };

    const CHROME: &str = r"c:\program files\google\chrome\application\chrome.exe";

    fn entry(id: &str, title: &str, at: i64) -> RememberedTitle {
        RememberedTitle {
            id: id.into(),
            executable: CHROME.into(),
            title: title.into(),
            browser_host: Some("video.example".into()),
            remembered_at_ms: at,
            reason: None,
        }
    }

    #[test]
    fn title_key_ignores_case_width_and_whitespace() {
        assert_eq!(
            title_key("  Hot　VIDEO   Page - Google Chrome "),
            "hot video page - google chrome"
        );
        assert_eq!(title_key("ＡＢＣ"), "abc");
    }

    #[test]
    fn generic_titles_are_not_remembered() {
        assert_eq!(rememberable_title_key(CHROME, ""), None);
        assert_eq!(rememberable_title_key(CHROME, " x "), None);
        assert_eq!(rememberable_title_key(CHROME, "Chrome"), None);
        assert_eq!(rememberable_title_key(CHROME, "chrome.exe"), None);
        assert_eq!(
            rememberable_title_key(CHROME, "某视频 - Google Chrome").as_deref(),
            Some("某视频 - google chrome")
        );
    }

    #[test]
    fn title_digest_identifies_only_the_same_executable_and_title() {
        let digest = title_digest(CHROME, "Hot Video - Google Chrome").unwrap();
        assert_eq!(digest.len(), 32);
        assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(
            title_digest(CHROME, "hot  video - GOOGLE chrome").as_deref(),
            Some(digest.as_str())
        );
        assert_ne!(
            title_digest(CHROME, "Homework - Google Chrome").as_deref(),
            Some(digest.as_str())
        );
        assert_ne!(
            title_digest(r"c:\edge\msedge.exe", "Hot Video - Google Chrome").as_deref(),
            Some(digest.as_str())
        );
        assert_eq!(title_digest(CHROME, "  "), None);
    }

    #[test]
    fn remembering_a_known_title_refreshes_it_and_the_cap_drops_the_oldest() {
        let mut records = Vec::new();
        assert!(remember_title(&mut records, entry("t1", "Same Title", 1)));
        assert!(!remember_title(&mut records, entry("t2", "same title", 5)));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "t1");
        assert_eq!(records[0].remembered_at_ms, 5);

        let mut crowded: Vec<_> = (0..MAX_REMEMBERED_TITLES)
            .map(|index| {
                entry(
                    &format!("t{index}"),
                    &format!("title {index}"),
                    index as i64,
                )
            })
            .collect();
        let newest = MAX_REMEMBERED_TITLES;
        assert!(remember_title(
            &mut crowded,
            entry(
                &format!("t{newest}"),
                &format!("title {newest}"),
                newest as i64
            )
        ));
        assert_eq!(crowded.len(), MAX_REMEMBERED_TITLES);
        assert!(!crowded.iter().any(|record| record.id == "t0"));
        assert!(
            crowded
                .iter()
                .any(|record| record.id == format!("t{MAX_REMEMBERED_TITLES}"))
        );
    }

    #[test]
    fn remembering_is_enabled_unless_switched_off() {
        assert!(remember_titles_enabled(&json!({})));
        assert!(!remember_titles_enabled(
            &json!({"recognition": {"rememberTitlesEnabled": false}})
        ));
    }
}
