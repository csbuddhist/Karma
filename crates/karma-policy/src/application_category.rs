use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

use crate::application_policy::executable_name;

/// Categories every policy has. `custom` holds applications without a more
/// specific category.
pub const BUILT_IN_APPLICATION_CATEGORIES: &[&str] = &["browser", "player", "game", "custom"];

/// The built-in category whose members also include `KNOWN_BROWSER_EXECUTABLES`.
pub const BROWSER_CATEGORY: &str = "browser";

const MAX_CUSTOM_CATEGORIES: usize = 64;
const MAX_CATEGORY_ID_CHARS: usize = 128;
const MAX_CATEGORY_NAME_CHARS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ApplicationCategoryError {
    #[error("application categories are invalid")]
    InvalidPolicy,
}

/// The built-in and administrator-defined application categories
/// (`policy.applicationCategories`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationCategories {
    ids: BTreeSet<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CategoryInput {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct ApplicationInput {
    executable: String,
    category: String,
    enabled: bool,
}

impl ApplicationCategories {
    pub fn from_value(policy: &Value) -> Result<Self, ApplicationCategoryError> {
        let raw = policy
            .get("applicationCategories")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let raw: Vec<CategoryInput> =
            serde_json::from_value(raw).map_err(|_| ApplicationCategoryError::InvalidPolicy)?;
        if raw.len() > MAX_CUSTOM_CATEGORIES {
            return Err(ApplicationCategoryError::InvalidPolicy);
        }
        let mut ids: BTreeSet<String> = BUILT_IN_APPLICATION_CATEGORIES
            .iter()
            .map(|id| (*id).to_owned())
            .collect();
        for category in raw {
            let name = category.name.trim();
            if category.id.trim().is_empty()
                || category.id.chars().count() > MAX_CATEGORY_ID_CHARS
                || name.is_empty()
                || name.chars().count() > MAX_CATEGORY_NAME_CHARS
                || !ids.insert(category.id)
            {
                return Err(ApplicationCategoryError::InvalidPolicy);
            }
        }
        Ok(Self { ids })
    }

    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }
}

/// Lowercase executable file names of the enabled application rules in each
/// category. Placeholder rules without an `.exe` file name and malformed
/// entries are skipped; `ApplicationPolicy` validates the list.
pub(crate) fn category_members(policy: &Value) -> BTreeMap<String, BTreeSet<String>> {
    let mut members: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let Some(applications) = policy.get("applications").and_then(Value::as_array) else {
        return members;
    };
    for application in applications
        .iter()
        .filter_map(|value| serde_json::from_value::<ApplicationInput>(value.clone()).ok())
        .filter(|application| application.enabled)
    {
        if let Some(file_name) =
            executable_name(&application.executable).filter(|file_name| file_name.ends_with(".exe"))
        {
            members
                .entry(application.category)
                .or_default()
                .insert(file_name);
        }
    }
    members
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn custom_categories_extend_the_built_in_ones() {
        let categories = ApplicationCategories::from_value(&json!({
            "applicationCategories": [{"id": "cat-chat", "name": "聊天"}]
        }))
        .unwrap();
        assert!(categories.contains("browser"));
        assert!(categories.contains("cat-chat"));
        assert!(!categories.contains("cat-other"));
        assert!(ApplicationCategories::from_value(&Value::Null).is_ok());
    }

    #[test]
    fn invalid_categories_are_rejected() {
        for categories in [
            json!([{"id": "game", "name": "Games again"}]),
            json!([{"id": "a", "name": "A"}, {"id": "a", "name": "B"}]),
            json!([{"id": "", "name": "A"}]),
            json!([{"id": "a", "name": "  "}]),
            json!([{"id": "a", "name": "A", "extra": 1}]),
            json!({"id": "a", "name": "A"}),
        ] {
            assert_eq!(
                ApplicationCategories::from_value(&json!({ "applicationCategories": categories })),
                Err(ApplicationCategoryError::InvalidPolicy),
                "{categories}"
            );
        }
    }

    #[test]
    fn members_are_enabled_executables_grouped_by_category() {
        let members = category_members(&json!({"applications": [
            {"id":"a","name":"A","executable":r"C:\Games\Steam.exe","category":"cat-games","action":"content_only","enabled":true},
            {"id":"b","name":"B","executable":"launcher.exe","category":"cat-games","action":"block","enabled":true},
            {"id":"c","name":"C","executable":"off.exe","category":"cat-games","action":"content_only","enabled":false},
            {"id":"player","name":"播放器","executable":"受支持播放器","category":"player","action":"content_only","enabled":true}
        ]}));
        assert_eq!(
            members["cat-games"],
            BTreeSet::from(["steam.exe".to_owned(), "launcher.exe".to_owned()])
        );
        assert!(!members.contains_key("player"));
    }
}
