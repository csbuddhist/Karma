use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

use crate::context::ContextVerdict;

const MAX_APPLICATION_RULES: usize = 256;
const MAX_RULE_ID_CHARS: usize = 128;
const MAX_EXECUTABLE_CHARS: usize = 2048;

/// How the enforcement path treats one application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationEffect {
    /// Never close this application from content-triggered enforcement.
    Allow,
    /// Close this application whenever it is observed in the foreground.
    Block,
    /// Only content enforcement applies (the default).
    ContentOnly,
}

/// Console-managed application rules (`policy.applications`), matched against
/// the observed process image path.
#[derive(Debug, Clone, Default)]
pub struct ApplicationPolicy {
    rules: Vec<StoredRule>,
}

#[derive(Debug, Clone)]
struct StoredRule {
    executable_components: Vec<String>,
    effect: ApplicationEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ApplicationPolicyError {
    #[error("application policy is invalid")]
    InvalidPolicy,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplicationRuleInput {
    id: String,
    #[allow(dead_code)]
    name: String,
    executable: String,
    #[allow(dead_code)]
    category: String,
    action: ApplicationAction,
    enabled: bool,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum ApplicationAction {
    Allow,
    Block,
    ContentOnly,
}

impl ApplicationPolicy {
    pub fn from_value(policy: &Value) -> Result<Self, ApplicationPolicyError> {
        let raw_rules = policy
            .get("applications")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let raw_rules: Vec<ApplicationRuleInput> =
            serde_json::from_value(raw_rules).map_err(|_| ApplicationPolicyError::InvalidPolicy)?;
        if raw_rules.len() > MAX_APPLICATION_RULES {
            return Err(ApplicationPolicyError::InvalidPolicy);
        }
        let mut rules = Vec::new();
        for rule in raw_rules.into_iter().filter(|rule| rule.enabled) {
            if rule.id.is_empty()
                || rule.id.chars().count() > MAX_RULE_ID_CHARS
                || rule.executable.trim().is_empty()
                || rule.executable.chars().count() > MAX_EXECUTABLE_CHARS
            {
                return Err(ApplicationPolicyError::InvalidPolicy);
            }
            let executable_components = path_components(&rule.executable);
            if executable_components.is_empty() {
                return Err(ApplicationPolicyError::InvalidPolicy);
            }
            rules.push(StoredRule {
                executable_components,
                effect: match rule.action {
                    ApplicationAction::Allow => ApplicationEffect::Allow,
                    ApplicationAction::Block => ApplicationEffect::Block,
                    ApplicationAction::ContentOnly => ApplicationEffect::ContentOnly,
                },
            });
        }
        Ok(Self { rules })
    }

    pub fn effect_for(&self, executable_path: &str) -> ApplicationEffect {
        let observed = path_components(executable_path);
        let mut resolved = ApplicationEffect::ContentOnly;
        for rule in &self.rules {
            if trailing_components_match(&observed, &rule.executable_components) {
                // A block rule outranks an allow rule so a malicious or
                // mistaken allow entry cannot neutralize an explicit block.
                if rule.effect == ApplicationEffect::Block {
                    return ApplicationEffect::Block;
                }
                if rule.effect == ApplicationEffect::Allow {
                    resolved = ApplicationEffect::Allow;
                }
            }
        }
        resolved
    }
}

fn path_components(value: &str) -> Vec<String> {
    value
        .split(['\\', '/'])
        .filter(|component| !component.is_empty())
        .map(|component| component.to_lowercase())
        .collect()
}

fn trailing_components_match(observed: &[String], rule: &[String]) -> bool {
    !rule.is_empty()
        && observed.len() >= rule.len()
        && observed[observed.len() - rule.len()..] == *rule
}

/// The shared enforcement decision for context observations: URL rules
/// outrank application rules, an allowed application suppresses keyword
/// enforcement, and a blocked application is closed whenever it is observed.
pub fn context_enforcement(verdict: ContextVerdict, effect: ApplicationEffect) -> bool {
    match verdict {
        ContextVerdict::Allowlisted => false,
        ContextVerdict::Blocklisted => true,
        ContextVerdict::TitleKeyword => effect != ApplicationEffect::Allow,
        ContextVerdict::None => effect == ApplicationEffect::Block,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn policy(applications: Value) -> ApplicationPolicy {
        ApplicationPolicy::from_value(&json!({ "applications": applications })).unwrap()
    }

    #[test]
    fn allow_rules_match_executable_names_and_full_paths() {
        let applications = policy(json!([
            {"id":"zcode","name":"ZCode","executable":"ZCode.exe","category":"custom","action":"allow","enabled":true},
            {"id":"chatgpt","name":"ChatGPT","executable":"C:\\Program Files\\ChatGPT\\ChatGPT.exe","category":"custom","action":"allow","enabled":true}
        ]));

        assert_eq!(
            applications.effect_for(r"C:\Users\wei\AppData\Local\Programs\zcode.exe"),
            ApplicationEffect::Allow
        );
        assert_eq!(
            applications.effect_for(r"C:\Program Files\ChatGPT\ChatGPT.exe"),
            ApplicationEffect::Allow
        );
        assert_eq!(
            applications.effect_for(r"C:\Program Files\Other\ChatGPT.exe"),
            ApplicationEffect::ContentOnly
        );
        // Component matching must not let a short name hit a longer one.
        assert_eq!(
            applications.effect_for(r"C:\Tools\subzcode.exe"),
            ApplicationEffect::ContentOnly
        );
    }

    #[test]
    fn block_outranks_allow_and_disabled_rules_are_ignored() {
        let applications = policy(json!([
            {"id":"allow-name","name":"A","executable":"game.exe","category":"game","action":"allow","enabled":true},
            {"id":"block-path","name":"B","executable":"D:\\Games\\game.exe","category":"game","action":"block","enabled":true},
            {"id":"off","name":"C","executable":"player.exe","category":"player","action":"block","enabled":false}
        ]));

        assert_eq!(
            applications.effect_for(r"D:\Games\game.exe"),
            ApplicationEffect::Block
        );
        assert_eq!(
            applications.effect_for(r"E:\Games\game.exe"),
            ApplicationEffect::Allow
        );
        assert_eq!(
            applications.effect_for(r"C:\player.exe"),
            ApplicationEffect::ContentOnly
        );
    }

    #[test]
    fn invalid_application_rules_are_rejected() {
        assert!(
            ApplicationPolicy::from_value(&json!({
                "applications": [
                    {"id":"bad","name":"X","executable":"","category":"custom","action":"allow","enabled":true}
                ]
            }))
            .is_err()
        );
        assert!(
            ApplicationPolicy::from_value(&json!({
                "applications": [
                    {"id":"bad","name":"X","executable":"a.exe","category":"custom","action":"terminate","enabled":true}
                ]
            }))
            .is_err()
        );
        assert!(
            ApplicationPolicy::from_value(&json!({
                "applications": [
                    {"id":"bad","name":"X","executable":"a.exe","category":"custom","action":"allow","enabled":true,"extra":1}
                ]
            }))
            .is_err()
        );
        // Unknown categories are tolerated so the console can introduce new
        // grouping without breaking the enforcement path.
        assert!(ApplicationPolicy::from_value(&Value::Null).is_ok());
    }

    #[test]
    fn context_enforcement_combines_url_verdicts_with_application_effects() {
        use crate::context::ContextVerdict;

        // Keyword hits enforce unless the application is allowed.
        assert!(context_enforcement(
            ContextVerdict::TitleKeyword,
            ApplicationEffect::ContentOnly
        ));
        assert!(!context_enforcement(
            ContextVerdict::TitleKeyword,
            ApplicationEffect::Allow
        ));
        // URL verdicts outrank application rules.
        assert!(context_enforcement(
            ContextVerdict::Blocklisted,
            ApplicationEffect::Allow
        ));
        assert!(!context_enforcement(
            ContextVerdict::Allowlisted,
            ApplicationEffect::Block
        ));
        // A blocked application enforces even with an ordinary title.
        assert!(context_enforcement(
            ContextVerdict::None,
            ApplicationEffect::Block
        ));
        assert!(!context_enforcement(
            ContextVerdict::None,
            ApplicationEffect::ContentOnly
        ));
    }
}
