use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationFacts {
    pub normalized_path: String,
    pub publisher: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleEffect {
    Allow,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplicationMatcher {
    PathSuffix(String),
    Publisher(String),
    Sha256(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationRule {
    pub id: String,
    pub priority: i32,
    pub matcher: ApplicationMatcher,
    pub effect: RuleEffect,
}

impl ApplicationRule {
    pub fn path(id: &str, priority: i32, value: &str, effect: RuleEffect) -> Self {
        Self {
            id: id.into(),
            priority,
            matcher: ApplicationMatcher::PathSuffix(value.into()),
            effect,
        }
    }

    pub fn publisher(id: &str, priority: i32, value: &str, effect: RuleEffect) -> Self {
        Self {
            id: id.into(),
            priority,
            matcher: ApplicationMatcher::Publisher(value.into()),
            effect,
        }
    }

    pub fn hash(id: &str, priority: i32, value: &str, effect: RuleEffect) -> Self {
        Self {
            id: id.into(),
            priority,
            matcher: ApplicationMatcher::Sha256(value.into()),
            effect,
        }
    }

    fn matches(&self, facts: &ApplicationFacts) -> bool {
        match &self.matcher {
            ApplicationMatcher::PathSuffix(value) => facts.normalized_path.ends_with(value),
            ApplicationMatcher::Publisher(value) => facts.publisher.as_ref() == Some(value),
            ApplicationMatcher::Sha256(value) => facts.sha256.as_ref() == Some(value),
        }
    }
}

pub fn resolve_application<'a>(
    rules: &'a [ApplicationRule],
    facts: &ApplicationFacts,
) -> Option<&'a ApplicationRule> {
    rules
        .iter()
        .filter(|rule| rule.matches(facts))
        .max_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then_with(|| right.id.cmp(&left.id))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> ApplicationFacts {
        ApplicationFacts {
            normalized_path: r"c:\browser\browser.exe".into(),
            publisher: Some("Browser Ltd".into()),
            sha256: Some("abc".into()),
        }
    }

    #[test]
    fn higher_priority_match_wins() {
        let rules = vec![
            ApplicationRule::path("allow", 10, "browser.exe", RuleEffect::Allow),
            ApplicationRule::publisher("block", 20, "Browser Ltd", RuleEffect::Block),
        ];

        assert_eq!(resolve_application(&rules, &facts()).unwrap().id, "block");
    }

    #[test]
    fn equal_priority_uses_stable_id_order() {
        let rules = vec![
            ApplicationRule::hash("z", 10, "abc", RuleEffect::Block),
            ApplicationRule::hash("a", 10, "abc", RuleEffect::Allow),
        ];

        assert_eq!(resolve_application(&rules, &facts()).unwrap().id, "a");
    }
}
