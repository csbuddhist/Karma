use std::collections::BTreeSet;

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
/// the file name of the observed process image, so a program keeps its rule
/// wherever it is installed or copied. Rules saved with a full path match by
/// their file name too.
#[derive(Debug, Clone, Default)]
pub struct ApplicationPolicy {
    rules: Vec<StoredRule>,
}

#[derive(Debug, Clone)]
struct StoredRule {
    executable_name: String,
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
            let Some(executable_name) = executable_name(&rule.executable) else {
                return Err(ApplicationPolicyError::InvalidPolicy);
            };
            rules.push(StoredRule {
                executable_name,
                effect: match rule.action {
                    ApplicationAction::Allow => ApplicationEffect::Allow,
                    ApplicationAction::Block => ApplicationEffect::Block,
                    ApplicationAction::ContentOnly => ApplicationEffect::ContentOnly,
                },
            });
        }
        Ok(Self { rules })
    }

    /// Executable file names of the enabled rules with `effect`.
    pub(crate) fn executables_with(&self, effect: ApplicationEffect) -> BTreeSet<String> {
        self.rules
            .iter()
            .filter(|rule| rule.effect == effect)
            .map(|rule| rule.executable_name.clone())
            .collect()
    }

    pub fn effect_for(&self, executable_path: &str) -> ApplicationEffect {
        let Some(observed) = executable_name(executable_path) else {
            return ApplicationEffect::ContentOnly;
        };
        let mut resolved = ApplicationEffect::ContentOnly;
        for rule in &self.rules {
            if rule.executable_name == observed {
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

/// Returns `policy` with the program `executable_path` names always allowed.
/// Rules match by file name: an enabled "Always block" rule for it would
/// outrank any allow rule, so it becomes "Allow"; the first rule for it is
/// enabled as "Allow"; otherwise a new custom "Allow" rule with `new_rule_id`
/// is appended that records only the file name.
pub fn allow_application(
    policy: &Value,
    executable_path: &str,
    new_rule_id: &str,
) -> Result<Value, ApplicationPolicyError> {
    let Some(target) = executable_name(executable_path) else {
        return Err(ApplicationPolicyError::InvalidPolicy);
    };
    ApplicationPolicy::from_value(policy)?;
    let mut next = policy.clone();
    let rules = next
        .as_object_mut()
        .ok_or(ApplicationPolicyError::InvalidPolicy)?
        .entry("applications")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or(ApplicationPolicyError::InvalidPolicy)?;
    let mut found = false;
    for rule in rules.iter_mut() {
        if executable_name(rule["executable"].as_str().unwrap_or_default()).as_ref()
            != Some(&target)
        {
            continue;
        }
        if !found {
            rule["action"] = Value::from("allow");
            rule["enabled"] = Value::Bool(true);
            found = true;
        } else if rule["enabled"] == Value::Bool(true) && rule["action"] == "block" {
            rule["action"] = Value::from("allow");
        }
    }
    if !found {
        let name = executable_file_name(executable_path).unwrap_or(executable_path);
        rules.push(serde_json::json!({
            "id": new_rule_id,
            "name": name,
            "executable": name,
            "category": "custom",
            "action": "allow",
            "enabled": true
        }));
    }
    // Revalidates the rule cap and confirms no remaining rule blocks it.
    if ApplicationPolicy::from_value(&next)?.effect_for(executable_path) != ApplicationEffect::Allow
    {
        return Err(ApplicationPolicyError::InvalidPolicy);
    }
    Ok(next)
}

/// The last component of an executable path as written, such as `Game.exe`.
pub(crate) fn executable_file_name(value: &str) -> Option<&str> {
    value
        .trim()
        .rsplit(['\\', '/'])
        .find(|component| !component.is_empty())
}

/// The case-insensitive file name application rules match by.
pub(crate) fn executable_name(value: &str) -> Option<String> {
    executable_file_name(value).map(str::to_lowercase)
}

pub(crate) fn path_components(value: &str) -> Vec<String> {
    value
        .split(['\\', '/'])
        .filter(|component| !component.is_empty())
        .map(|component| component.to_lowercase())
        .collect()
}

pub(crate) fn trailing_components_match(observed: &[String], rule: &[String]) -> bool {
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
    fn rules_match_the_executable_file_name_wherever_it_is() {
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
        // A rule saved with a full path still follows the program when it moves.
        assert_eq!(
            applications.effect_for(r"D:\Portable\ChatGPT.exe"),
            ApplicationEffect::Allow
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
            applications.effect_for(r"E:\Games\GAME.EXE"),
            ApplicationEffect::Block
        );
        assert_eq!(
            applications.effect_for(r"E:\Games\game2.exe"),
            ApplicationEffect::ContentOnly
        );
        assert_eq!(
            applications.effect_for(r"C:\player.exe"),
            ApplicationEffect::ContentOnly
        );
    }

    #[test]
    fn allowing_an_application_appends_a_rule_with_only_its_file_name() {
        let banned = r"c:\games\Game.exe";
        let next = allow_application(
            &json!({"protectionEnabled": true, "applications": [
                {"id":"other","name":"Other","executable":"other.exe","category":"custom","action":"block","enabled":true}
            ]}),
            banned,
            "app-new",
        )
        .unwrap();

        assert_eq!(next["protectionEnabled"], true);
        let rules = next["applications"].as_array().unwrap();
        assert_eq!(rules[0]["action"], "block");
        assert_eq!(
            rules[1],
            json!({"id":"app-new","name":"Game.exe","executable":"Game.exe","category":"custom","action":"allow","enabled":true})
        );
        let applications = ApplicationPolicy::from_value(&next).unwrap();
        assert_eq!(applications.effect_for(banned), ApplicationEffect::Allow);
        assert_eq!(
            applications.effect_for(r"E:\Moved\game.exe"),
            ApplicationEffect::Allow
        );
    }

    #[test]
    fn allowing_an_application_reuses_its_rules_and_overrides_blocks() {
        let next = allow_application(
            &json!({"applications": [
                {"id":"player","name":"Player","executable":"C:\\Apps\\Player.exe","category":"player","action":"content_only","enabled":false},
                {"id":"block-name","name":"Player","executable":"player.exe","category":"player","action":"block","enabled":true},
                {"id":"other","name":"Other","executable":"other.exe","category":"custom","action":"block","enabled":true}
            ]}),
            r"d:\portable\player.exe",
            "app-new",
        )
        .unwrap();

        assert_eq!(
            next["applications"],
            json!([
                {"id":"player","name":"Player","executable":"C:\\Apps\\Player.exe","category":"player","action":"allow","enabled":true},
                {"id":"block-name","name":"Player","executable":"player.exe","category":"player","action":"allow","enabled":true},
                {"id":"other","name":"Other","executable":"other.exe","category":"custom","action":"block","enabled":true}
            ])
        );
        assert!(allow_application(&json!({}), "", "app-new").is_err());
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
