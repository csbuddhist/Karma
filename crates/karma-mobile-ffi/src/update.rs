//! Android update discovery: which published APK is newer than the installed
//! one, and which SHA-256 its checksum file announces. Pure functions over the
//! release-list JSON that the platform downloads; the platform performs the
//! HTTPS requests, checks the APK hash and signing certificate, and installs.
//!
//! Only GitHub `csbuddhist/Karma` and its Gitee mirror `karmakarma/Karma` are
//! accepted, and every download URL must be the fixed release-download URL of
//! that source, so a release list cannot point the phone anywhere else.

use std::cmp::Ordering;
use std::fmt;

use serde::Deserialize;

/// Tag prefix of Android releases; desktop releases use plain `v`.
const TAG_PREFIX: &str = "android-v";
/// Release lists larger than this are rejected before parsing.
pub const MAX_RELEASE_LIST_BYTES: u64 = 4 * 1024 * 1024;
/// A published APK larger than this is never downloaded.
pub const MAX_APK_BYTES: u64 = 256 * 1024 * 1024;
/// The checksum file holds one line per published file.
pub const MAX_CHECKSUM_BYTES: u64 = 4 * 1024;

/// A repository that publishes Karma APKs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ReleaseSource {
    GitHub,
    Gitee,
}

impl ReleaseSource {
    /// Every source, in the order downloads are attempted for one version.
    pub const ALL: [Self; 2] = [Self::GitHub, Self::Gitee];

    pub fn releases_url(self) -> &'static str {
        match self {
            Self::GitHub => "https://api.github.com/repos/csbuddhist/Karma/releases?per_page=30",
            Self::Gitee => {
                "https://gitee.com/api/v5/repos/karmakarma/Karma/releases?page=1&per_page=30&direction=desc"
            }
        }
    }

    fn download_prefix(self) -> &'static str {
        match self {
            Self::GitHub => "https://github.com/csbuddhist/Karma/releases/download/",
            Self::Gitee => "https://gitee.com/karmakarma/Karma/releases/download/",
        }
    }

    fn asset_url(self, version: AndroidVersion, name: &str) -> String {
        format!("{}{TAG_PREFIX}{version}/{name}", self.download_prefix())
    }

    fn rank(self) -> usize {
        Self::ALL
            .iter()
            .position(|source| *source == self)
            .unwrap_or(usize::MAX)
    }
}

/// `major.minor.patch` of an Android release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct AndroidVersion(u32, u32, u32);

impl AndroidVersion {
    fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split('.');
        let mut next = || -> Option<u32> {
            let part = parts.next()?;
            if part.is_empty() || part.len() > 6 || !part.bytes().all(|byte| byte.is_ascii_digit())
            {
                return None;
            }
            part.parse().ok()
        };
        let version = Self(next()?, next()?, next()?);
        parts.next().is_none().then_some(version)
    }
}

impl fmt::Display for AndroidVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// A published APK newer than the installed version.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AndroidRelease {
    /// `major.minor.patch`, without the tag prefix.
    pub version: String,
    pub source: ReleaseSource,
    pub apk_name: String,
    pub apk_url: String,
    /// Exact byte count when the source announces it (Gitee does not).
    pub apk_size: Option<u64>,
    pub checksum_url: String,
}

#[derive(Deserialize)]
struct PublishedRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<PublishedAsset>,
}

#[derive(Deserialize)]
struct PublishedAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: Option<u64>,
}

/// Picks the newest non-draft Android release (prereleases included) newer
/// than `installed` that carries both `Karma-android-v<version>.apk` and
/// `SHA256SUMS-android-v<version>.txt` at the source's fixed download URLs.
/// Returns `None` for unparsable input or when nothing newer is published.
pub fn select_release(
    source: ReleaseSource,
    releases_json: &[u8],
    installed: &str,
) -> Option<AndroidRelease> {
    let installed = AndroidVersion::parse(installed)?;
    if releases_json.len() as u64 > MAX_RELEASE_LIST_BYTES {
        return None;
    }
    let releases: Vec<PublishedRelease> = serde_json::from_slice(releases_json).ok()?;
    releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let version = AndroidVersion::parse(release.tag_name.strip_prefix(TAG_PREFIX)?)?;
            if version <= installed {
                return None;
            }
            let apk_name = format!("Karma-android-v{version}.apk");
            let checksum_name = format!("SHA256SUMS-android-v{version}.txt");
            let apk = release.assets.iter().find(|asset| {
                asset.name == apk_name
                    && asset.browser_download_url == source.asset_url(version, &apk_name)
                    && asset
                        .size
                        .is_none_or(|size| size > 0 && size <= MAX_APK_BYTES)
            })?;
            release.assets.iter().find(|asset| {
                asset.name == checksum_name
                    && asset.browser_download_url == source.asset_url(version, &checksum_name)
            })?;
            Some((
                version,
                AndroidRelease {
                    version: version.to_string(),
                    source,
                    apk_url: apk.browser_download_url.clone(),
                    apk_size: apk.size,
                    checksum_url: source.asset_url(version, &checksum_name),
                    apk_name,
                },
            ))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, release)| release)
}

/// Orders the releases found on several sources: newest version first, then
/// GitHub before its mirror, so a failed download falls back to the mirror.
pub fn order_releases(releases: &mut [AndroidRelease]) {
    releases.sort_by(|left, right| {
        let version = |release: &AndroidRelease| AndroidVersion::parse(&release.version);
        match version(right).cmp(&version(left)) {
            Ordering::Equal => left.source.rank().cmp(&right.source.rank()),
            other => other,
        }
    });
}

/// The lowercase SHA-256 that a `sha256sum`-style checksum file lists for
/// `file_name`, or `None` when it is missing, malformed or listed twice.
pub fn checksum_for(checksums: &str, file_name: &str) -> Option<String> {
    if checksums.len() as u64 > MAX_CHECKSUM_BYTES {
        return None;
    }
    let mut found = None;
    for line in checksums.lines() {
        let line = line.trim_end_matches('\r');
        let Some((hash, name)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        // `sha256sum` marks binary mode with a leading `*`.
        let name = name.trim_start().trim_start_matches('*');
        if name != file_name {
            continue;
        }
        if found.is_some() || hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        found = Some(hash.to_ascii_lowercase());
    }
    found
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn release(source: ReleaseSource, tag: &str, size: Option<u64>) -> serde_json::Value {
        let version = tag.trim_start_matches(TAG_PREFIX);
        let prefix = source.download_prefix();
        json!({
            "tag_name": tag,
            "draft": false,
            "prerelease": true,
            "assets": [
                {
                    "name": format!("Karma-android-v{version}.apk"),
                    "browser_download_url": format!("{prefix}{tag}/Karma-android-v{version}.apk"),
                    "size": size,
                },
                {
                    "name": format!("SHA256SUMS-android-v{version}.txt"),
                    "browser_download_url": format!("{prefix}{tag}/SHA256SUMS-android-v{version}.txt"),
                    "size": 91,
                }
            ]
        })
    }

    fn list(releases: &[serde_json::Value]) -> Vec<u8> {
        serde_json::to_vec(releases).unwrap()
    }

    #[test]
    fn newest_android_release_is_selected() {
        let json = list(&[
            release(ReleaseSource::GitHub, "android-v0.1.2", Some(63_725_181)),
            release(ReleaseSource::GitHub, "android-v0.1.10", Some(63_725_181)),
            release(ReleaseSource::GitHub, "android-v0.1.1", Some(63_725_181)),
            json!({"tag_name": "v9.9.9", "assets": []}),
        ]);
        let selected = select_release(ReleaseSource::GitHub, &json, "0.1.2").unwrap();
        assert_eq!(selected.version, "0.1.10");
        assert_eq!(selected.apk_name, "Karma-android-v0.1.10.apk");
        assert_eq!(
            selected.apk_url,
            "https://github.com/csbuddhist/Karma/releases/download/android-v0.1.10/Karma-android-v0.1.10.apk"
        );
        assert_eq!(
            selected.checksum_url,
            "https://github.com/csbuddhist/Karma/releases/download/android-v0.1.10/SHA256SUMS-android-v0.1.10.txt"
        );
        assert_eq!(selected.apk_size, Some(63_725_181));
    }

    #[test]
    fn installed_or_older_versions_are_not_updates() {
        let json = list(&[release(ReleaseSource::GitHub, "android-v0.1.2", Some(1))]);
        assert_eq!(select_release(ReleaseSource::GitHub, &json, "0.1.2"), None);
        assert_eq!(select_release(ReleaseSource::GitHub, &json, "0.2.0"), None);
        assert!(select_release(ReleaseSource::GitHub, &json, "0.1.1").is_some());
    }

    #[test]
    fn releases_without_both_assets_or_with_foreign_urls_are_skipped() {
        let mut foreign = release(ReleaseSource::GitHub, "android-v0.2.0", Some(10));
        foreign["assets"][0]["browser_download_url"] =
            json!("https://example.com/android-v0.2.0/Karma-android-v0.2.0.apk");
        let mut no_checksum = release(ReleaseSource::GitHub, "android-v0.3.0", Some(10));
        no_checksum["assets"].as_array_mut().unwrap().pop();
        let mut draft = release(ReleaseSource::GitHub, "android-v0.4.0", Some(10));
        draft["draft"] = json!(true);
        let oversized = release(
            ReleaseSource::GitHub,
            "android-v0.5.0",
            Some(MAX_APK_BYTES + 1),
        );
        let mirror_url = release(ReleaseSource::Gitee, "android-v0.6.0", None);
        let json = list(&[foreign, no_checksum, draft, oversized, mirror_url]);
        assert_eq!(select_release(ReleaseSource::GitHub, &json, "0.1.0"), None);
    }

    #[test]
    fn gitee_tag_archives_alone_are_not_releases() {
        let json = list(&[
            json!({
                "tag_name": "android-v0.1.2",
                "assets": [{
                    "name": "android-v0.1.2.zip",
                    "browser_download_url": "https://gitee.com/karmakarma/Karma/archive/refs/tags/android-v0.1.2.zip"
                }]
            }),
            release(ReleaseSource::Gitee, "android-v0.1.1", None),
        ]);
        let selected = select_release(ReleaseSource::Gitee, &json, "0.1.0").unwrap();
        assert_eq!(selected.version, "0.1.1");
        assert_eq!(selected.apk_size, None);
    }

    #[test]
    fn malformed_input_selects_nothing() {
        assert_eq!(select_release(ReleaseSource::GitHub, b"{", "0.1.0"), None);
        let json = list(&[release(ReleaseSource::GitHub, "android-v0.2.0", Some(1))]);
        assert_eq!(select_release(ReleaseSource::GitHub, &json, "0.1"), None);
        assert_eq!(
            select_release(ReleaseSource::GitHub, &json, "0.1.0-debug"),
            None
        );
        let tags = list(&[release(ReleaseSource::GitHub, "android-v0.2.0.1", Some(1))]);
        assert_eq!(select_release(ReleaseSource::GitHub, &tags, "0.1.0"), None);
    }

    #[test]
    fn newer_versions_come_first_and_github_precedes_its_mirror() {
        let found = |version: &str, source| AndroidRelease {
            version: version.into(),
            source,
            apk_name: String::new(),
            apk_url: String::new(),
            apk_size: None,
            checksum_url: String::new(),
        };
        let mut releases = vec![
            found("0.1.3", ReleaseSource::Gitee),
            found("0.1.10", ReleaseSource::Gitee),
            found("0.1.10", ReleaseSource::GitHub),
        ];
        order_releases(&mut releases);
        let order: Vec<_> = releases
            .iter()
            .map(|release| (release.version.as_str(), release.source))
            .collect();
        assert_eq!(
            order,
            [
                ("0.1.10", ReleaseSource::GitHub),
                ("0.1.10", ReleaseSource::Gitee),
                ("0.1.3", ReleaseSource::Gitee)
            ]
        );
    }

    #[test]
    fn checksum_file_lines_are_matched_by_exact_name() {
        let hash = "A".repeat(64);
        let sums = format!(
            "{hash}  Karma-android-v0.1.2.apk\r\n{}  other.apk\n",
            "b".repeat(64)
        );
        assert_eq!(
            checksum_for(&sums, "Karma-android-v0.1.2.apk"),
            Some("a".repeat(64))
        );
        assert_eq!(
            checksum_for(&format!("{hash} *Karma.apk"), "Karma.apk"),
            Some("a".repeat(64))
        );
        assert_eq!(checksum_for(&sums, "Karma-android-v0.1.3.apk"), None);
        assert_eq!(checksum_for("abc  Karma.apk", "Karma.apk"), None);
        let twice = format!("{hash}  Karma.apk\n{hash}  Karma.apk\n");
        assert_eq!(checksum_for(&twice, "Karma.apk"), None);
    }
}
