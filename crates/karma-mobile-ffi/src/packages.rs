//! Android package knowledge shared by the policy and the frame scheduler.
//!
//! Desktop policies name applications by executable file name. On Android the
//! package name takes that role: application rules, categories, and schedule
//! targets match it exactly (case-insensitively), because a package name has
//! no path separators.

use serde_json::{Value, json};

/// The Karma application itself. It is never captured or closed.
pub const KARMA_PACKAGE: &str = "com.karma.android";

/// Packages that are never captured or closed: Karma, the system UI, and the
/// phone and emergency dialers, which must stay usable during any schedule.
pub const EXEMPT_PACKAGES: &[&str] = &[
    KARMA_PACKAGE,
    "android",
    "com.android.systemui",
    "com.android.phone",
    "com.android.server.telecom",
    "com.android.dialer",
    "com.google.android.dialer",
    "com.android.emergency",
    "com.google.android.apps.safetyhub",
    "com.android.incallui",
    "com.android.permissioncontroller",
    "com.google.android.permissioncontroller",
    "com.android.packageinstaller",
    "com.google.android.packageinstaller",
];

/// Browsers whose address bar the accessibility service reads and whose
/// `FLAG_SECURE` windows (private tabs) count as unverifiable content.
pub const KNOWN_BROWSER_PACKAGES: &[&str] = &[
    "com.android.chrome",
    "com.chrome.beta",
    "com.chrome.dev",
    "com.chrome.canary",
    "org.chromium.chrome",
    "com.microsoft.emmx",
    "org.mozilla.firefox",
    "org.mozilla.firefox_beta",
    "org.mozilla.fenix",
    "org.mozilla.focus",
    "org.mozilla.klar",
    "com.sec.android.app.sbrowser",
    "com.brave.browser",
    "com.opera.browser",
    "com.opera.mini.native",
    "com.opera.gx",
    "com.vivaldi.browser",
    "com.duckduckgo.mobile.android",
    "com.yandex.browser",
    "com.kiwibrowser.browser",
    "com.UCMobile",
    "com.uc.browser.en",
    "com.quark.browser",
    "com.tencent.mtt",
    "com.baidu.browser.apps",
    "com.qihoo.browser",
    "sogou.mobile.explorer",
    "com.huawei.browser",
    "com.hihonor.baidu.browser",
    "com.heytap.browser",
    "com.coloros.browser",
    "com.android.browser",
    "com.mi.globalbrowser",
    "com.vivo.browser",
    "mark.via",
    "mark.via.gp",
];

/// Video and short-video applications sampled at a fixed rate because their
/// content changes without accessibility events.
pub const KNOWN_VIDEO_PACKAGES: &[&str] = &[
    "com.google.android.youtube",
    "com.google.android.apps.youtube.music",
    "com.zhiliaoapp.musically",
    "com.ss.android.ugc.trill",
    "com.ss.android.ugc.aweme",
    "com.ss.android.ugc.aweme.lite",
    "com.smile.gifmaker",
    "com.kuaishou.nebula",
    "tv.danmaku.bili",
    "com.bilibili.app.in",
    "com.tencent.qqlive",
    "com.qiyi.video",
    "com.youku.phone",
    "com.hunantv.imgo.activity",
    "com.xingin.xhs",
    "com.netflix.mediaclient",
    "com.instagram.android",
    "com.twitter.android",
    "com.reddit.frontpage",
    "com.tumblr",
    "com.sina.weibo",
    "org.videolan.vlc",
    "com.mxtech.videoplayer.ad",
    "com.mxtech.videoplayer.pro",
];

/// The lowercase form application rules compare, or `None` when the value is
/// not a plausible package name.
pub fn normalize_package(value: &str) -> Option<String> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 256
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
    valid.then(|| value.to_ascii_lowercase())
}

pub fn is_exempt(package: &str) -> bool {
    EXEMPT_PACKAGES
        .iter()
        .any(|p| p.eq_ignore_ascii_case(package))
}

pub fn is_known_browser(package: &str) -> bool {
    KNOWN_BROWSER_PACKAGES
        .iter()
        .any(|p| p.eq_ignore_ascii_case(package))
}

pub fn is_known_video(package: &str) -> bool {
    KNOWN_VIDEO_PACKAGES
        .iter()
        .any(|p| p.eq_ignore_ascii_case(package))
}

/// Maximum application rules a policy may hold (mirrors `karma-policy`).
const MAX_APPLICATION_RULES: usize = 256;

/// Returns `policy` with the built-in Android browsers and video players added
/// as enabled `content_only` application rules in the `browser` and `player`
/// categories, so the shared desktop policy code resolves browser categories,
/// schedule targets, and address-bar rules for Android packages. Packages the
/// administrator already lists keep their own rule.
pub fn with_builtin_applications(policy: &Value) -> Value {
    let mut policy = if policy.is_object() {
        policy.clone()
    } else {
        json!({})
    };
    let object = policy.as_object_mut().expect("policy is an object");
    let applications = object
        .entry("applications")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(applications) = applications.as_array_mut() else {
        // Leave malformed input for the policy parsers to reject.
        return policy;
    };
    let listed: Vec<String> = applications
        .iter()
        .filter_map(|rule| rule.get("executable").and_then(Value::as_str))
        .filter_map(normalize_package)
        .collect();
    let builtins = KNOWN_BROWSER_PACKAGES
        .iter()
        .map(|p| (*p, "browser"))
        .chain(KNOWN_VIDEO_PACKAGES.iter().map(|p| (*p, "player")));
    for (package, category) in builtins {
        if applications.len() >= MAX_APPLICATION_RULES {
            break;
        }
        let normalized = package.to_ascii_lowercase();
        if listed.contains(&normalized) {
            continue;
        }
        applications.push(json!({
            "id": format!("android-builtin:{normalized}"),
            "name": package,
            "executable": package,
            "category": category,
            "action": "content_only",
            "enabled": true,
        }));
    }
    policy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_names_are_validated_and_lowercased() {
        assert_eq!(
            normalize_package(" com.UCMobile "),
            Some("com.ucmobile".into())
        );
        assert_eq!(normalize_package("../etc/passwd"), None);
        assert_eq!(normalize_package(""), None);
    }

    #[test]
    fn builtin_applications_do_not_override_administrator_rules() {
        let policy = json!({"applications": [{
            "id": "chrome-block",
            "name": "Chrome",
            "executable": "com.android.chrome",
            "category": "browser",
            "action": "block",
            "enabled": true
        }]});
        let merged = with_builtin_applications(&policy);
        let rules = merged["applications"].as_array().unwrap();
        let chrome: Vec<_> = rules
            .iter()
            .filter(|rule| rule["executable"] == "com.android.chrome")
            .collect();
        assert_eq!(chrome.len(), 1);
        assert_eq!(chrome[0]["action"], "block");
        assert!(
            rules
                .iter()
                .any(|rule| rule["executable"] == "org.mozilla.firefox")
        );
    }

    #[test]
    fn system_dialer_is_exempt() {
        assert!(is_exempt("com.google.android.dialer"));
        assert!(is_exempt(KARMA_PACKAGE));
        assert!(!is_exempt("com.android.chrome"));
    }
}
