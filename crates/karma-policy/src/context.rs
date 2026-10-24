use std::collections::BTreeSet;
use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;
use url::{Host, Url};

const MAX_WEBSITE_RULES: usize = 256;
const MAX_KEYWORD_RULES: usize = 256;
/// Upper bound on keywords administrators add to the built-in list.
pub const MAX_TITLE_KEYWORD_ADDITIONS: usize = 1024;
/// Longest term accepted in the built-in list or as an addition to it.
pub const MAX_TITLE_KEYWORD_TERM_CHARS: usize = 64;
/// Longest product-code prefix, such as `SSIS`.
const MAX_CODE_PREFIX_CHARS: usize = 16;
const MAX_RULE_ID_CHARS: usize = 128;
const MAX_RULE_VALUE_CHARS: usize = 2048;
const BUNDLED_TITLE_KEYWORDS: &str =
    include_str!("../../../assets/keyword-lists/window-title-explicit.json");
const BUNDLED_FORMAT_VERSION: u32 = 2;

/// The bundled list, parsed and normalized once per process.
static BUNDLED: LazyLock<Result<BundledTitleKeywords, ContextPolicyError>> =
    LazyLock::new(load_bundled_keywords);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebsiteRuleAction {
    Allow,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebsiteRule {
    pub id: String,
    pub host: String,
    pub action: WebsiteRuleAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextVerdict {
    None,
    Allowlisted,
    Blocklisted,
    TitleKeyword,
}

/// How a title keyword is compared with the normalized window title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeywordMatch {
    /// The keyword's words appear as consecutive whole words. Keywords with
    /// CJK characters, which are not separated by spaces, match as substrings.
    Word,
    /// The keyword appears anywhere in the title.
    Substring,
    /// Like `Word`, but the last title word only has to start with the
    /// keyword's last word.
    Prefix,
    /// The keyword is a product-code prefix such as `SSIS`, matched when it
    /// is followed by three or four digits and, optionally, a hyphen between
    /// them with one space on either side: `SSIS-123`, `SSIS 123`, or
    /// `SSIS123`.
    Code,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TitleKeyword {
    term: String,
    words: Vec<String>,
    mode: KeywordMatch,
}

#[derive(Debug)]
struct BundledTitleKeywords {
    /// Enforced whenever title matching is enabled.
    strict: Vec<TitleKeyword>,
    /// Likely to produce false positives; enforced only when enabled.
    ambiguous: Vec<TitleKeyword>,
}

#[derive(Debug, Clone)]
pub struct ContextPolicy {
    title_matching_enabled: bool,
    ambiguous_keywords_enabled: bool,
    website_rules: Vec<WebsiteRule>,
    bundled: &'static BundledTitleKeywords,
    /// Custom high-risk and sensitive phrases plus additions to the built-in list.
    added_keywords: Vec<TitleKeyword>,
    exempt_keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ContextPolicyError {
    #[error("website or title-keyword policy is invalid")]
    InvalidPolicy,
    #[error("bundled title-keyword data is invalid")]
    InvalidBundledKeywords,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WebsiteRuleInput {
    id: String,
    pattern: String,
    action: WebsiteRuleAction,
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeywordRuleInput {
    id: String,
    phrase: String,
    category: KeywordCategory,
    enabled: bool,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum KeywordCategory {
    HighRisk,
    Sensitive,
    Exemption,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TitleKeywordCategory {
    Organ,
    Sexual,
    /// Product codes of adult videos, which always use `KeywordMatch::Code`.
    Code,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum KeywordConfidence {
    Strict,
    Ambiguous,
}

/// A keyword an administrator added to the built-in list. It is always
/// enforced, like the list's strict keywords.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TitleKeywordAdditionInput {
    id: String,
    term: String,
    language: String,
    category: TitleKeywordCategory,
    #[serde(rename = "match")]
    mode: KeywordMatch,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BundledKeywords {
    format_version: u32,
    languages: Vec<LanguageKeywords>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LanguageKeywords {
    language: String,
    keywords: Vec<BundledKeyword>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BundledKeyword {
    term: String,
    category: TitleKeywordCategory,
    confidence: KeywordConfidence,
    #[serde(rename = "match")]
    mode: KeywordMatch,
}

impl Default for ContextPolicy {
    fn default() -> Self {
        Self::from_value(&Value::Null).expect("bundled title keywords must be valid")
    }
}

impl ContextPolicy {
    pub fn from_value(policy: &Value) -> Result<Self, ContextPolicyError> {
        let title_matching_enabled = policy
            .pointer("/recognition/titleMatchingEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let ambiguous_keywords_enabled = policy
            .pointer("/recognition/ambiguousTitleKeywordsEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let raw_rules = policy
            .get("websites")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let raw_rules: Vec<WebsiteRuleInput> =
            serde_json::from_value(raw_rules).map_err(|_| ContextPolicyError::InvalidPolicy)?;
        if raw_rules.len() > MAX_WEBSITE_RULES {
            return Err(ContextPolicyError::InvalidPolicy);
        }
        let mut website_rules = Vec::with_capacity(raw_rules.len());
        for rule in raw_rules.into_iter().filter(|rule| rule.enabled) {
            if rule.id.is_empty()
                || rule.id.chars().count() > MAX_RULE_ID_CHARS
                || rule.pattern.is_empty()
                || rule.pattern.chars().count() > MAX_RULE_VALUE_CHARS
            {
                return Err(ContextPolicyError::InvalidPolicy);
            }
            website_rules.push(WebsiteRule {
                id: rule.id,
                host: normalize_rule_host(&rule.pattern)
                    .ok_or(ContextPolicyError::InvalidPolicy)?,
                action: rule.action,
            });
        }

        let raw_keywords = policy
            .get("keywords")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let raw_keywords: Vec<KeywordRuleInput> =
            serde_json::from_value(raw_keywords).map_err(|_| ContextPolicyError::InvalidPolicy)?;
        if raw_keywords.len() > MAX_KEYWORD_RULES {
            return Err(ContextPolicyError::InvalidPolicy);
        }
        let mut added_keywords = Vec::new();
        let mut exempt_keywords = Vec::new();
        for rule in raw_keywords.into_iter().filter(|rule| rule.enabled) {
            if rule.id.is_empty()
                || rule.id.chars().count() > MAX_RULE_ID_CHARS
                || rule.phrase.trim().is_empty()
                || rule.phrase.chars().count() > MAX_RULE_VALUE_CHARS
            {
                return Err(ContextPolicyError::InvalidPolicy);
            }
            match rule.category {
                KeywordCategory::HighRisk | KeywordCategory::Sensitive => {
                    added_keywords.push(TitleKeyword::new(&rule.phrase, KeywordMatch::Word))
                }
                KeywordCategory::Exemption => exempt_keywords.push(normalize_text(&rule.phrase)),
            }
        }

        let raw_additions = policy
            .get("titleKeywordAdditions")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let raw_additions: Vec<TitleKeywordAdditionInput> =
            serde_json::from_value(raw_additions).map_err(|_| ContextPolicyError::InvalidPolicy)?;
        if raw_additions.len() > MAX_TITLE_KEYWORD_ADDITIONS {
            return Err(ContextPolicyError::InvalidPolicy);
        }
        for addition in raw_additions {
            if addition.id.is_empty()
                || addition.id.chars().count() > MAX_RULE_ID_CHARS
                || addition.term.trim().is_empty()
                || addition.term.chars().count() > MAX_TITLE_KEYWORD_TERM_CHARS
                || !valid_language(&addition.language)
                || !valid_code_usage(&addition.term, addition.category, addition.mode)
            {
                return Err(ContextPolicyError::InvalidPolicy);
            }
            added_keywords.push(TitleKeyword::new(&addition.term, addition.mode));
        }

        let bundled = BUNDLED.as_ref().map_err(Clone::clone)?;

        Ok(Self {
            title_matching_enabled,
            ambiguous_keywords_enabled,
            website_rules,
            bundled,
            added_keywords,
            exempt_keywords,
        })
    }

    pub fn evaluate(&self, browser_host: Option<&str>, window_title: &str) -> ContextVerdict {
        let normalized_host = browser_host.and_then(normalize_observed_host);
        if normalized_host.as_deref().is_some_and(|host| {
            self.website_rules.iter().any(|rule| {
                rule.action == WebsiteRuleAction::Allow && host_matches(host, &rule.host)
            })
        }) {
            return ContextVerdict::Allowlisted;
        }
        if normalized_host.as_deref().is_some_and(|host| {
            self.website_rules.iter().any(|rule| {
                rule.action == WebsiteRuleAction::Block && host_matches(host, &rule.host)
            })
        }) {
            return ContextVerdict::Blocklisted;
        }
        if self.title_matching_enabled {
            let normalized_title = normalize_text(window_title);
            let words = title_words(&normalized_title);
            let is_exempt = self
                .exempt_keywords
                .iter()
                .any(|keyword| phrase_matches(&normalized_title, &words, keyword));
            if !is_exempt
                && !normalized_title.is_empty()
                && self
                    .enforced_keywords()
                    .any(|keyword| keyword.matches(&normalized_title, &words))
            {
                return ContextVerdict::TitleKeyword;
            }
        }
        ContextVerdict::None
    }

    /// Normalized hosts of the enabled website rules with `action`.
    pub(crate) fn website_hosts(&self, action: WebsiteRuleAction) -> BTreeSet<&str> {
        self.website_rules
            .iter()
            .filter(|rule| rule.action == action)
            .map(|rule| rule.host.as_str())
            .collect()
    }

    /// Bundled keywords in effect, then custom keywords and additions.
    fn enforced_keywords(&self) -> impl Iterator<Item = &TitleKeyword> {
        let ambiguous: &[TitleKeyword] = if self.ambiguous_keywords_enabled {
            &self.bundled.ambiguous
        } else {
            &[]
        };
        self.bundled
            .strict
            .iter()
            .chain(ambiguous)
            .chain(&self.added_keywords)
    }

    /// Normalized enforced title keywords (bundled, custom, and additions)
    /// with how each one matches.
    pub(crate) fn title_keyword_set(&self) -> BTreeSet<(KeywordMatch, &str)> {
        self.enforced_keywords()
            .map(|keyword| (keyword.mode, keyword.term.as_str()))
            .collect()
    }

    /// Normalized enabled exemption keywords.
    pub(crate) fn exempt_keyword_set(&self) -> BTreeSet<&str> {
        self.exempt_keywords.iter().map(String::as_str).collect()
    }

    pub fn allows_host(&self, browser_host: Option<&str>) -> bool {
        self.evaluate(browser_host, "") == ContextVerdict::Allowlisted
    }
}

impl TitleKeyword {
    fn new(term: &str, mode: KeywordMatch) -> Self {
        let term = normalize_text(term);
        // Whole-word matching cannot find words in unspaced CJK text.
        let mode = if contains_cjk(&term) {
            KeywordMatch::Substring
        } else {
            mode
        };
        let words = title_words(&term).into_iter().map(str::to_owned).collect();
        Self { term, words, mode }
    }

    fn matches(&self, normalized_title: &str, title_words: &[&str]) -> bool {
        let Some((last, leading)) = self.words.split_last() else {
            return self.mode == KeywordMatch::Substring && normalized_title.contains(&self.term);
        };
        match self.mode {
            KeywordMatch::Substring => normalized_title.contains(&self.term),
            KeywordMatch::Word => title_words.windows(self.words.len()).any(|window| {
                window
                    .iter()
                    .zip(&self.words)
                    .all(|(left, right)| left == right)
            }),
            KeywordMatch::Prefix => title_words.windows(self.words.len()).any(|window| {
                window
                    .iter()
                    .zip(leading)
                    .all(|(left, right)| left == right)
                    && window[leading.len()].starts_with(last.as_str())
            }),
            KeywordMatch::Code => code_matches(normalized_title, &self.term),
        }
    }
}

fn load_bundled_keywords() -> Result<BundledTitleKeywords, ContextPolicyError> {
    let document: BundledKeywords = serde_json::from_str(BUNDLED_TITLE_KEYWORDS)
        .map_err(|_| ContextPolicyError::InvalidBundledKeywords)?;
    if document.format_version != BUNDLED_FORMAT_VERSION || document.languages.is_empty() {
        return Err(ContextPolicyError::InvalidBundledKeywords);
    }
    let mut strict = Vec::new();
    let mut ambiguous = Vec::new();
    for group in document.languages {
        if !valid_language(&group.language) || group.keywords.is_empty() {
            return Err(ContextPolicyError::InvalidBundledKeywords);
        }
        for keyword in group.keywords {
            if keyword.term.trim().is_empty()
                || keyword.term.chars().count() > MAX_TITLE_KEYWORD_TERM_CHARS
                || !valid_code_usage(&keyword.term, keyword.category, keyword.mode)
            {
                return Err(ContextPolicyError::InvalidBundledKeywords);
            }
            let compiled = TitleKeyword::new(&keyword.term, keyword.mode);
            match keyword.confidence {
                KeywordConfidence::Strict => strict.push(compiled),
                KeywordConfidence::Ambiguous => ambiguous.push(compiled),
            }
        }
    }
    Ok(BundledTitleKeywords { strict, ambiguous })
}

/// Only product codes use code matching, and their terms must be code
/// prefixes: 2-16 ASCII letters and digits with at least one letter.
fn valid_code_usage(term: &str, category: TitleKeywordCategory, mode: KeywordMatch) -> bool {
    match (category, mode) {
        (TitleKeywordCategory::Code, KeywordMatch::Code) => {
            let prefix = normalize_text(term);
            (2..=MAX_CODE_PREFIX_CHARS).contains(&prefix.len())
                && prefix
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                && prefix.bytes().any(|byte| byte.is_ascii_lowercase())
        }
        (TitleKeywordCategory::Code, _) | (_, KeywordMatch::Code) => false,
        _ => true,
    }
}

/// Whether `prefix` appears in the title, not preceded by an ASCII letter or
/// digit, and is followed by a three- or four-digit number. A hyphen may
/// separate the two, and one space may sit on either side of the hyphen or,
/// without one, between them.
fn code_matches(normalized_title: &str, prefix: &str) -> bool {
    normalized_title.match_indices(prefix).any(|(start, _)| {
        let starts_word = !normalized_title[..start]
            .chars()
            .next_back()
            .is_some_and(|character| character.is_ascii_alphanumeric());
        starts_word && code_number_follows(&normalized_title[start + prefix.len()..])
    })
}

fn code_number_follows(rest: &str) -> bool {
    let rest = rest.strip_prefix(char::is_whitespace).unwrap_or(rest);
    let rest = rest.strip_prefix(is_dash).map_or(rest, |after| {
        after.strip_prefix(char::is_whitespace).unwrap_or(after)
    });
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    (3..=4).contains(&digits)
}

/// ASCII and Unicode hyphens and dashes. NFKC already folds full-width forms.
fn is_dash(character: char) -> bool {
    matches!(character, '-' | '\u{2010}'..='\u{2015}' | '\u{2212}')
}

/// A lowercase ISO 639 language code, such as `zh` or `fil`.
fn valid_language(value: &str) -> bool {
    (2..=3).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn title_words(normalized: &str) -> Vec<&str> {
    normalized
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect()
}

fn phrase_matches(normalized_title: &str, title_words: &[&str], keyword: &str) -> bool {
    if contains_cjk(keyword) {
        return normalized_title.contains(keyword);
    }
    let keyword_words = keyword
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    !keyword_words.is_empty()
        && title_words
            .windows(keyword_words.len())
            .any(|window| window == keyword_words.as_slice())
}

fn normalize_text(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn normalize_rule_host(value: &str) -> Option<String> {
    let value = value.trim();
    let parsed = Url::parse(value)
        .or_else(|_| Url::parse(&format!("https://{value}")))
        .ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !matches!(parsed.path(), "" | "/")
    {
        return None;
    }
    canonical_host(&parsed)
}

fn normalize_observed_host(value: &str) -> Option<String> {
    let parsed = Url::parse(&format!("https://{}", value.trim())).ok()?;
    canonical_host(&parsed)
}

fn canonical_host(url: &Url) -> Option<String> {
    match url.host()? {
        Host::Domain(domain) if domain.ends_with('.') => None,
        Host::Domain(domain) => Some(domain.to_owned()),
        Host::Ipv4(address) => Some(address.to_string()),
        Host::Ipv6(address) => Some(address.to_string()),
    }
}

fn host_matches(observed: &str, rule: &str) -> bool {
    observed == rule
        || (rule.parse::<std::net::IpAddr>().is_err()
            && observed
                .strip_suffix(rule)
                .is_some_and(|prefix| prefix.ends_with('.')))
}

fn contains_cjk(value: &str) -> bool {
    value.chars().any(|character| {
        matches!(character,
            '\u{3400}'..='\u{4dbf}'
                | '\u{4e00}'..='\u{9fff}'
                | '\u{3040}'..='\u{30ff}'
                | '\u{31f0}'..='\u{31ff}')
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn allowlist_precedes_blocklist_and_includes_subdomains() {
        let policy = ContextPolicy::from_value(&json!({
            "websites": [
                {"id":"block","pattern":"example.com","action":"block","enabled":true},
                {"id":"allow","pattern":"https://safe.example.com/","action":"allow","enabled":true}
            ]
        }))
        .unwrap();

        assert_eq!(
            policy.evaluate(Some("media.safe.example.com"), "porn"),
            ContextVerdict::Allowlisted
        );
        assert_eq!(
            policy.evaluate(Some("other.example.com"), "ordinary"),
            ContextVerdict::Blocklisted
        );
        assert_eq!(
            policy.evaluate(Some("notexample.com"), "ordinary"),
            ContextVerdict::None
        );
    }

    #[test]
    fn domains_are_canonicalized_and_paths_are_rejected() {
        let policy = ContextPolicy::from_value(&json!({
            "websites": [
                {"id":"unicode","pattern":"例え.テスト","action":"allow","enabled":true}
            ]
        }))
        .unwrap();
        assert!(policy.allows_host(Some("xn--r8jz45g.xn--zckzah")));
        assert!(
            ContextPolicy::from_value(&json!({
                "websites": [
                    {"id":"bad","pattern":"example.com/path","action":"block","enabled":true}
                ]
            }))
            .is_err()
        );
        assert!(
            ContextPolicy::from_value(&json!({
                "websites": [
                    {"id":"trailing-dot","pattern":"example.com.","action":"allow","enabled":true}
                ]
            }))
            .is_err()
        );
    }

    #[test]
    fn ip_rules_match_exactly_without_domain_suffix_logic() {
        let policy = ContextPolicy::from_value(&json!({
            "websites": [
                {"id":"ip","pattern":"10.0.0.1","action":"block","enabled":true}
            ]
        }))
        .unwrap();
        assert_eq!(
            policy.evaluate(Some("10.0.0.1"), "ordinary"),
            ContextVerdict::Blocklisted
        );
        assert_eq!(
            policy.evaluate(Some("x.10.0.0.1"), "ordinary"),
            ContextVerdict::None
        );
    }

    #[test]
    fn title_matching_uses_word_boundaries_and_cjk_phrases() {
        let policy = ContextPolicy::default();
        assert_eq!(
            policy.evaluate(None, "Free PORN videos"),
            ContextVerdict::TitleKeyword
        );
        assert_eq!(
            policy.evaluate(None, "Popular typography reference"),
            ContextVerdict::None
        );
        assert_eq!(
            policy.evaluate(None, "最新色情影片"),
            ContextVerdict::TitleKeyword
        );
        assert_eq!(
            policy.evaluate(None, "ПОРНО видео"),
            ContextVerdict::TitleKeyword
        );
    }

    #[test]
    fn title_matching_can_be_disabled() {
        let policy = ContextPolicy::from_value(&json!({
            "recognition": {"titleMatchingEnabled": false}
        }))
        .unwrap();
        assert_eq!(policy.evaluate(None, "porn"), ContextVerdict::None);
    }

    #[test]
    fn custom_keywords_join_title_matching_and_exemptions_suppress_it() {
        let policy = ContextPolicy::from_value(&json!({
            "keywords": [
                {"id":"custom-1","phrase":"赌球直播","category":"high_risk","enabled":true},
                {"id":"custom-2","phrase":"casino stream","category":"sensitive","enabled":true},
                {"id":"custom-off","phrase":"ignored phrase","category":"high_risk","enabled":false},
                {"id":"exempt-1","phrase":"医学教育","category":"exemption","enabled":true},
                {"id":"exempt-2","phrase":"art history","category":"exemption","enabled":true}
            ]
        }))
        .unwrap();

        assert_eq!(
            policy.evaluate(None, "今晚 赌球直播 现场"),
            ContextVerdict::TitleKeyword
        );
        assert_eq!(
            policy.evaluate(None, "Best CASINO Stream tonight"),
            ContextVerdict::TitleKeyword
        );
        assert_eq!(
            policy.evaluate(None, "ignored phrase in title"),
            ContextVerdict::None
        );
        // A bundled keyword is suppressed when an exemption phrase also matches.
        assert_eq!(
            policy.evaluate(None, "色情内容医学教育课件"),
            ContextVerdict::None
        );
        // Exemptions use word boundaries for non-CJK phrases and do not
        // suppress other titles.
        assert_eq!(
            policy.evaluate(None, "Modern ART History porn lecture"),
            ContextVerdict::None
        );
        assert_eq!(
            policy.evaluate(None, "Party porn videos"),
            ContextVerdict::TitleKeyword
        );
        // Website verdicts still outrank keyword exemptions.
        let strict = ContextPolicy::from_value(&json!({
            "websites": [
                {"id":"blocked","pattern":"blocked.example","action":"block","enabled":true}
            ],
            "keywords": [
                {"id":"exempt-1","phrase":"blocked.example","category":"exemption","enabled":true}
            ]
        }))
        .unwrap();
        assert_eq!(
            strict.evaluate(Some("blocked.example"), "blocked.example"),
            ContextVerdict::Blocklisted
        );
    }

    #[test]
    fn invalid_custom_keyword_rules_are_rejected() {
        let base = |keywords| json!({ "keywords": keywords });
        assert!(
            ContextPolicy::from_value(&base(vec![serde_json::json!(
                {"id":"k","phrase":"","category":"high_risk","enabled":true}
            )]))
            .is_err()
        );
        assert!(
            ContextPolicy::from_value(&base(vec![serde_json::json!(
                {"id":"k","phrase":"unknown category","category":"other","enabled":true}
            )]))
            .is_err()
        );
        assert!(
            ContextPolicy::from_value(&base(vec![serde_json::json!(
                {"id":"k","phrase":"extra field","category":"high_risk","enabled":true,"note":"x"}
            )]))
            .is_err()
        );
        let oversized: Vec<serde_json::Value> = (0..257)
            .map(|index| {
                serde_json::json!({"id":format!("k{index}"),"phrase":"词","category":"high_risk","enabled":true})
            })
            .collect();
        assert!(ContextPolicy::from_value(&base(oversized)).is_err());
    }

    #[test]
    fn bundled_strict_keywords_cover_every_listed_language_and_match_mode() {
        let policy = ContextPolicy::default();
        for title in [
            "Watch titten clips",
            "大鸡巴合集",
            "無料セックス動画",
            "Ultimate MASTURBATION guide",
            "rule 34 gallery",
        ] {
            assert_eq!(
                policy.evaluate(None, title),
                ContextVerdict::TitleKeyword,
                "{title}"
            );
        }
        // Prefix and word keywords still need a word boundary at the start.
        assert_eq!(
            policy.evaluate(None, "Remasturbate archive"),
            ContextVerdict::None
        );
        assert_eq!(
            policy.evaluate(None, "Analysis report"),
            ContextVerdict::None
        );
    }

    #[test]
    fn ambiguous_bundled_keywords_apply_only_when_enabled() {
        let default = ContextPolicy::default();
        assert_eq!(default.evaluate(None, "奶茶店推荐"), ContextVerdict::None);
        assert_eq!(
            default.evaluate(None, "Lesbian history"),
            ContextVerdict::None
        );
        let enabled = ContextPolicy::from_value(&json!({
            "recognition": {"ambiguousTitleKeywordsEnabled": true}
        }))
        .unwrap();
        assert_eq!(
            enabled.evaluate(None, "奶茶店推荐"),
            ContextVerdict::TitleKeyword
        );
        assert_eq!(
            enabled.evaluate(None, "Lesbian history"),
            ContextVerdict::TitleKeyword
        );
        assert!(
            default
                .title_keyword_set()
                .is_subset(&enabled.title_keyword_set())
        );
    }

    #[test]
    fn bundled_code_prefixes_match_three_or_four_digit_product_codes() {
        let policy = ContextPolicy::default();
        for title in [
            "SSIS-001 新人出道",
            "ssis001.mp4 - Player",
            "SSIS 1234",
            "SSIS - 123",
            "SSIS -123",
            "SSIS– 123",
            "ＳＳＩＳ－１２３",
            "三上悠亚SSIS-123中文字幕",
            "[hhd800.com]SSIS-123C",
            "Watch 300MIUM-456 online",
            // JUL is a major label, so it stays strict despite month names.
            "JUL-123",
            "Sales report Jul 2024",
        ] {
            assert_eq!(
                policy.evaluate(None, title),
                ContextVerdict::TitleKeyword,
                "{title}"
            );
        }
        for title in [
            "SSIS-12 meeting notes",
            "SSIS-12345 build log",
            "SSIS  123",
            "SSIS_123",
            "MSSIS-123",
            "1SSIS-123",
            "SSIS package designer",
        ] {
            assert_eq!(
                policy.evaluate(None, title),
                ContextVerdict::None,
                "{title}"
            );
        }
    }

    #[test]
    fn ambiguous_code_prefixes_apply_only_when_enabled() {
        let default = ContextPolicy::default();
        let enabled = ContextPolicy::from_value(&json!({
            "recognition": {"ambiguousTitleKeywordsEnabled": true}
        }))
        .unwrap();
        for title in ["Sales report Nov 2024", "NOV-123"] {
            assert_eq!(
                default.evaluate(None, title),
                ContextVerdict::None,
                "{title}"
            );
            assert_eq!(
                enabled.evaluate(None, title),
                ContextVerdict::TitleKeyword,
                "{title}"
            );
        }
    }

    #[test]
    fn title_keyword_additions_are_enforced_with_their_match_mode() {
        let policy = ContextPolicy::from_value(&json!({
            "titleKeywordAdditions": [
                {"id":"a1","term":"Spicyclip","language":"en","category":"sexual","match":"prefix"},
                {"id":"a2","term":"nsfwzone","language":"en","category":"sexual","match":"substring"},
                {"id":"a3","term":"露骨直播","language":"zh","category":"sexual","match":"word"},
                {"id":"a4","term":"QXYZ","language":"ja","category":"code","match":"code"}
            ]
        }))
        .unwrap();
        assert_eq!(
            policy.evaluate(None, "qxyz 0042 preview"),
            ContextVerdict::TitleKeyword
        );
        assert_eq!(policy.evaluate(None, "QXYZ-42"), ContextVerdict::None);
        assert_eq!(
            policy.evaluate(None, "spicyclips daily"),
            ContextVerdict::TitleKeyword
        );
        assert_eq!(
            policy.evaluate(None, "the-nsfwzonehub"),
            ContextVerdict::TitleKeyword
        );
        assert_eq!(
            policy.evaluate(None, "今晚露骨直播间"),
            ContextVerdict::TitleKeyword
        );
        assert_eq!(
            policy.evaluate(None, "ordinary title"),
            ContextVerdict::None
        );
    }

    #[test]
    fn invalid_title_keyword_additions_are_rejected() {
        let addition = |value: serde_json::Value| json!({ "titleKeywordAdditions": [value] });
        for invalid in [
            json!({"id":"","term":"x1","language":"en","category":"sexual","match":"word"}),
            json!({"id":"a","term":" ","language":"en","category":"sexual","match":"word"}),
            json!({"id":"a","term":"x".repeat(65),"language":"en","category":"sexual","match":"word"}),
            json!({"id":"a","term":"x1","language":"EN","category":"sexual","match":"word"}),
            json!({"id":"a","term":"x1","language":"en","category":"violence","match":"word"}),
            json!({"id":"a","term":"x1","language":"en","category":"sexual","match":"regex"}),
            json!({"id":"a","term":"x1","language":"en","category":"sexual","match":"word","enabled":true}),
            json!({"id":"a","term":"qxyz","language":"ja","category":"code","match":"word"}),
            json!({"id":"a","term":"qxyz","language":"ja","category":"sexual","match":"code"}),
            json!({"id":"a","term":"qx yz","language":"ja","category":"code","match":"code"}),
            json!({"id":"a","term":"1234","language":"ja","category":"code","match":"code"}),
            json!({"id":"a","term":"q","language":"ja","category":"code","match":"code"}),
            json!({"id":"a","term":"番号","language":"zh","category":"code","match":"code"}),
            json!({"id":"a","term":"q".repeat(17),"language":"ja","category":"code","match":"code"}),
        ] {
            assert!(
                ContextPolicy::from_value(&addition(invalid.clone())).is_err(),
                "{invalid}"
            );
        }
        let oversized: Vec<serde_json::Value> = (0..=MAX_TITLE_KEYWORD_ADDITIONS)
            .map(|index| {
                json!({"id":format!("a{index}"),"term":"词","language":"zh","category":"organ","match":"substring"})
            })
            .collect();
        assert!(ContextPolicy::from_value(&json!({ "titleKeywordAdditions": oversized })).is_err());
    }
}
