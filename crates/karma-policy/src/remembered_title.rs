use serde::{Deserialize, Serialize};
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

/// Upper bound on remembered titles so persisted state and the agent policy
/// stay small; the oldest entries are forgotten first.
pub const MAX_REMEMBERED_TITLES: usize = 200;
/// Titles shorter than this after normalization are too generic to remember.
const MIN_TITLE_KEY_CHARS: usize = 2;

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

/// Whether `title` shown by `executable` (normalized) was remembered.
pub fn matches_remembered_title(
    records: &[RememberedTitle],
    executable: &str,
    title: &str,
) -> bool {
    let key = title_key(title);
    !key.is_empty()
        && records
            .iter()
            .any(|record| record.executable == executable && title_key(&record.title) == key)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        MAX_REMEMBERED_TITLES, RememberedTitle, matches_remembered_title, remember_title,
        remember_titles_enabled, rememberable_title_key, title_key,
    };

    const CHROME: &str = r"c:\program files\google\chrome\application\chrome.exe";

    fn entry(id: &str, title: &str, at: i64) -> RememberedTitle {
        RememberedTitle {
            id: id.into(),
            executable: CHROME.into(),
            title: title.into(),
            browser_host: Some("video.example".into()),
            remembered_at_ms: at,
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
    fn remembered_title_matches_only_the_same_executable_and_title() {
        let mut records = Vec::new();
        assert!(remember_title(
            &mut records,
            entry("t1", "Hot Video - Google Chrome", 1)
        ));
        assert!(matches_remembered_title(
            &records,
            CHROME,
            "hot  video - GOOGLE chrome"
        ));
        assert!(!matches_remembered_title(
            &records,
            CHROME,
            "Homework - Google Chrome"
        ));
        assert!(!matches_remembered_title(
            &records,
            r"c:\edge\msedge.exe",
            "Hot Video - Google Chrome"
        ));
        assert!(!matches_remembered_title(&records, CHROME, ""));
    }

    #[test]
    fn remembering_a_known_title_refreshes_it_and_the_cap_drops_the_oldest() {
        let mut records = Vec::new();
        assert!(remember_title(&mut records, entry("t1", "Same Title", 1)));
        assert!(!remember_title(&mut records, entry("t2", "same title", 5)));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "t1");
        assert_eq!(records[0].remembered_at_ms, 5);

        let mut crowded = Vec::new();
        for index in 0..=MAX_REMEMBERED_TITLES {
            remember_title(
                &mut crowded,
                entry(
                    &format!("t{index}"),
                    &format!("title {index}"),
                    index as i64,
                ),
            );
        }
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
