use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;
use url::{Host, Url};

const MAX_WEBSITE_RULES: usize = 256;
const MAX_KEYWORD_RULES: usize = 256;
const MAX_RULE_ID_CHARS: usize = 128;
const MAX_RULE_VALUE_CHARS: usize = 2048;
const BUNDLED_TITLE_KEYWORDS: &str =
    include_str!("../../../assets/keyword-lists/window-title-explicit.json");

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

#[derive(Debug, Clone)]
pub struct ContextPolicy {
    title_matching_enabled: bool,
    website_rules: Vec<WebsiteRule>,
    title_keywords: Vec<String>,
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
    keywords: Vec<String>,
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
        let mut custom_keywords = Vec::new();
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
                    custom_keywords.push(normalize_text(&rule.phrase))
                }
                KeywordCategory::Exemption => exempt_keywords.push(normalize_text(&rule.phrase)),
            }
        }

        let bundled: BundledKeywords = serde_json::from_str(BUNDLED_TITLE_KEYWORDS)
            .map_err(|_| ContextPolicyError::InvalidBundledKeywords)?;
        if bundled.format_version != 1
            || bundled.languages.is_empty()
            || bundled.languages.iter().any(|group| {
                group.language.is_empty()
                    || group.keywords.is_empty()
                    || group
                        .keywords
                        .iter()
                        .any(|keyword| keyword.trim().is_empty())
            })
        {
            return Err(ContextPolicyError::InvalidBundledKeywords);
        }
        let mut title_keywords: Vec<String> = bundled
            .languages
            .into_iter()
            .flat_map(|group| group.keywords)
            .map(|keyword| normalize_text(&keyword))
            .collect();
        title_keywords.append(&mut custom_keywords);

        Ok(Self {
            title_matching_enabled,
            website_rules,
            title_keywords,
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
                    .title_keywords
                    .iter()
                    .any(|keyword| phrase_matches(&normalized_title, &words, keyword))
            {
                return ContextVerdict::TitleKeyword;
            }
        }
        ContextVerdict::None
    }

    pub fn allows_host(&self, browser_host: Option<&str>) -> bool {
        self.evaluate(browser_host, "") == ContextVerdict::Allowlisted
    }
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
}
