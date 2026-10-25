//! Recognizes a browser's address bar among a window's text fields and reads
//! the host it shows. The Windows agent reads the fields through UI
//! Automation and the macOS agent through the Accessibility API.

/// The score a text field needs to count as the address bar.
pub const ADDRESS_BAR_MINIMUM_SCORE: u8 = 4;

/// The host of an address bar value, which browsers may show without the
/// scheme; `None` for search text, non-web schemes, and credentials.
pub fn host_from_address_bar_value(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return None;
    }
    let url = url::Url::parse(value)
        .or_else(|_| url::Url::parse(&format!("https://{value}")))
        .ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    url.host_str().map(str::to_owned)
}

/// How strongly a text field looks like the address bar, from its accessible
/// name (or description and placeholder), its automation identifier, and its
/// value.
pub fn address_bar_score(name: &str, automation_id: &str, value: &str) -> u8 {
    let name = name.to_lowercase();
    let automation_id = automation_id.to_lowercase();
    let mut score = u8::from(value.starts_with("http://") || value.starts_with("https://"));
    if [
        "address",
        "地址",
        "アドレス",
        "адрес",
        "网址",
        "網址",
        "omnibox",
        "url",
    ]
    .iter()
    .any(|hint| name.contains(hint))
    {
        score = score.saturating_add(4);
    }
    if ["address", "location", "omnibox", "url"]
        .iter()
        .any(|hint| automation_id.contains(hint))
    {
        score = score.saturating_add(5);
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_bar_values_yield_their_host() {
        assert_eq!(
            host_from_address_bar_value("https://gitee.com/karma").as_deref(),
            Some("gitee.com")
        );
        assert_eq!(
            host_from_address_bar_value("github.com/csbuddhist/Karma").as_deref(),
            Some("github.com")
        );
        assert_eq!(host_from_address_bar_value("karma family"), None);
        assert_eq!(host_from_address_bar_value("file:///Users/kid"), None);
        assert_eq!(host_from_address_bar_value("https://a:b@example.com"), None);
        assert_eq!(host_from_address_bar_value(""), None);
    }

    #[test]
    fn address_bars_are_told_apart_from_other_text_fields() {
        // Chrome and Edge describe the omnibox; Safari names it in its identifier.
        assert!(address_bar_score("Address and search bar", "", "") >= ADDRESS_BAR_MINIMUM_SCORE);
        assert!(address_bar_score("地址和搜索栏", "", "") >= ADDRESS_BAR_MINIMUM_SCORE);
        assert!(
            address_bar_score("", "WEB_BROWSER_ADDRESS_AND_SEARCH_FIELD", "")
                >= ADDRESS_BAR_MINIMUM_SCORE
        );
        assert!(address_bar_score("Search", "", "https://example.com") < ADDRESS_BAR_MINIMUM_SCORE);
    }
}
