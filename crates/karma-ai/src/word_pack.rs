use karma_domain::OcrRisk;
use regex::{Regex, RegexBuilder};
use serde::Serialize;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

use crate::OcrTextBatch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordRuleKind {
    Literal,
    Regex,
    Exemption,
}

pub struct WordRule {
    pub category: String,
    pub pattern: String,
    pub kind: WordRuleKind,
    pub risk: OcrRisk,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OcrMatchSummary {
    pub risk: OcrRisk,
    pub categories: Vec<String>,
    pub exemption_context: bool,
}

enum CompiledRule {
    Literal {
        category: String,
        value: String,
        risk: OcrRisk,
    },
    Regex {
        category: String,
        value: Regex,
        risk: OcrRisk,
    },
    Exemption {
        category: String,
        value: String,
    },
}

pub struct WordPack {
    rules: Vec<CompiledRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WordPackError {
    #[error("invalid regex in category {category}")]
    InvalidRegex { category: String },
}

impl WordRule {
    pub fn literal(category: &str, pattern: &str, risk: OcrRisk) -> Self {
        Self {
            category: category.into(),
            pattern: pattern.into(),
            kind: WordRuleKind::Literal,
            risk,
        }
    }

    pub fn regex(category: &str, pattern: &str, risk: OcrRisk) -> Self {
        Self {
            category: category.into(),
            pattern: pattern.into(),
            kind: WordRuleKind::Regex,
            risk,
        }
    }

    pub fn exemption(category: &str, pattern: &str) -> Self {
        Self {
            category: category.into(),
            pattern: pattern.into(),
            kind: WordRuleKind::Exemption,
            risk: OcrRisk::None,
        }
    }
}

fn normalize(value: &str) -> String {
    value.nfkc().flat_map(char::to_lowercase).collect()
}

fn risk_rank(value: OcrRisk) -> u8 {
    match value {
        OcrRisk::None => 0,
        OcrRisk::Keyword => 1,
        OcrRisk::HighRiskPhrase => 2,
    }
}

impl WordPack {
    pub fn compile(rules: Vec<WordRule>) -> Result<Self, WordPackError> {
        let rules = rules
            .into_iter()
            .map(|rule| match rule.kind {
                WordRuleKind::Literal => Ok(CompiledRule::Literal {
                    category: rule.category,
                    value: normalize(&rule.pattern),
                    risk: rule.risk,
                }),
                WordRuleKind::Exemption => Ok(CompiledRule::Exemption {
                    category: rule.category,
                    value: normalize(&rule.pattern),
                }),
                WordRuleKind::Regex => RegexBuilder::new(&rule.pattern)
                    .case_insensitive(true)
                    .unicode(true)
                    .build()
                    .map(|value| CompiledRule::Regex {
                        category: rule.category.clone(),
                        value,
                        risk: rule.risk,
                    })
                    .map_err(|_| WordPackError::InvalidRegex {
                        category: rule.category,
                    }),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { rules })
    }

    pub fn classify(&self, lines: &[&str]) -> OcrMatchSummary {
        let mut risk = OcrRisk::None;
        let mut categories = Vec::new();
        let mut exemption_context = false;

        for line in lines {
            let normalized = normalize(line);
            for rule in &self.rules {
                let (matched, category, rule_risk, exemption) = match rule {
                    CompiledRule::Literal {
                        category,
                        value,
                        risk,
                    } => (normalized.contains(value), category, *risk, false),
                    CompiledRule::Regex {
                        category,
                        value,
                        risk,
                    } => (value.is_match(&normalized), category, *risk, false),
                    CompiledRule::Exemption { category, value } => {
                        (normalized.contains(value), category, OcrRisk::None, true)
                    }
                };

                if matched {
                    categories.push(category.clone());
                    exemption_context |= exemption;
                    if risk_rank(rule_risk) > risk_rank(risk) {
                        risk = rule_risk;
                    }
                }
            }
        }

        categories.sort();
        categories.dedup();
        OcrMatchSummary {
            risk,
            categories,
            exemption_context,
        }
    }

    /// Classifies a zeroizing OCR batch without exposing its private line references cross-crate.
    pub fn classify_batch(&self, batch: &OcrTextBatch) -> OcrMatchSummary {
        self.classify(&batch.line_refs().collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_matching_normalizes_width_and_case() {
        let pack = WordPack::compile(vec![WordRule::literal(
            "adult_service",
            "ＡＤＵＬＴ",
            OcrRisk::Keyword,
        )])
        .unwrap();
        let result = pack.classify(&["adult"]);
        assert_eq!(result.risk, OcrRisk::Keyword);
        assert_eq!(result.categories, vec!["adult_service"]);
    }

    #[test]
    fn regex_can_mark_high_risk_phrase() {
        let pack = WordPack::compile(vec![WordRule::regex(
            "explicit_term",
            r"explicit\s+phrase",
            OcrRisk::HighRiskPhrase,
        )])
        .unwrap();
        assert_eq!(
            pack.classify(&["Explicit phrase"]).risk,
            OcrRisk::HighRiskPhrase
        );
    }

    #[test]
    fn exemption_is_reported_without_raw_text() {
        let pack = WordPack::compile(vec![WordRule::exemption("medical", "anatomy")]).unwrap();
        let value = pack.classify(&["Anatomy lesson"]);
        assert!(value.exemption_context);
        assert_eq!(value.categories, vec!["medical"]);
        assert!(
            !serde_json::to_string(&value)
                .unwrap()
                .contains("anatomy lesson")
        );
    }

    #[test]
    fn invalid_regex_is_rejected() {
        assert!(matches!(
            WordPack::compile(vec![WordRule::regex("bad", "(", OcrRisk::Keyword)]),
            Err(WordPackError::InvalidRegex { .. })
        ));
    }
}
