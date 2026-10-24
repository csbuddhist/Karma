//! Detects policy changes that weaken protection. Once a super password is
//! configured, the service accepts such a change only together with it, while
//! changes that keep or tighten protection need only the administrator
//! password.

use serde_json::Value;

use crate::{
    ApplicationEffect, ApplicationPolicy, ContextPolicy, MINUTES_PER_WEEK, NetworkSchedulePolicy,
    RepeatOffenderRule, WebsiteRuleAction, network_schedule::NetworkRestriction,
};

/// One way a policy change lowers protection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Weakening {
    /// The master protection switch was turned off.
    ProtectionDisabled,
    /// The console no longer starts at sign-in, so a restart would leave it
    /// closed.
    LaunchAtStartupDisabled,
    /// Image, OCR, window-title, or remembered-title recognition was turned off.
    RecognitionDisabled,
    /// The image risk threshold was raised, so fewer frames are enforced.
    SensitivityLowered,
    /// A website was allowlisted or removed from the blocklist.
    WebsiteRelaxed,
    /// An application was allowed or removed from the always-block list.
    ApplicationRelaxed,
    /// A custom title keyword or an addition to the built-in list was removed,
    /// ambiguous built-in keywords were turned off, or an exemption was added.
    KeywordRelaxed,
    /// A network schedule stopped covering a time, target, or application.
    ScheduleRelaxed,
    /// Repeat-offender bans now need more closures or a shorter window.
    RepeatOffenderRelaxed,
}

/// Every way `new` is weaker than `old`, sorted and without duplicates. Both
/// policies must already have passed validation; unreadable sections compare
/// as their defaults.
pub fn policy_weakenings(old: &Value, new: &Value) -> Vec<Weakening> {
    let mut found = Vec::new();
    if switched_off(old, new, "/protectionEnabled") {
        found.push(Weakening::ProtectionDisabled);
    }
    if switched_off(old, new, "/launchAtStartup") {
        found.push(Weakening::LaunchAtStartupDisabled);
    }
    if [
        "/recognition/imageEnabled",
        "/recognition/ocrEnabled",
        "/recognition/titleMatchingEnabled",
        "/recognition/rememberTitlesEnabled",
    ]
    .iter()
    .any(|pointer| switched_off(old, new, pointer))
    {
        found.push(Weakening::RecognitionDisabled);
    }
    if risk_threshold(new) > risk_threshold(old) {
        found.push(Weakening::SensitivityLowered);
    }

    let old_context = ContextPolicy::from_value(old).unwrap_or_default();
    let new_context = ContextPolicy::from_value(new).unwrap_or_default();
    if !new_context
        .website_hosts(WebsiteRuleAction::Allow)
        .is_subset(&old_context.website_hosts(WebsiteRuleAction::Allow))
        || !old_context
            .website_hosts(WebsiteRuleAction::Block)
            .is_subset(&new_context.website_hosts(WebsiteRuleAction::Block))
    {
        found.push(Weakening::WebsiteRelaxed);
    }
    if !old_context
        .title_keyword_set()
        .is_subset(&new_context.title_keyword_set())
        || !new_context
            .exempt_keyword_set()
            .is_subset(&old_context.exempt_keyword_set())
    {
        found.push(Weakening::KeywordRelaxed);
    }

    let old_applications = ApplicationPolicy::from_value(old).unwrap_or_default();
    let new_applications = ApplicationPolicy::from_value(new).unwrap_or_default();
    if !new_applications
        .executables_with(ApplicationEffect::Allow)
        .is_subset(&old_applications.executables_with(ApplicationEffect::Allow))
        || !old_applications
            .executables_with(ApplicationEffect::Block)
            .is_subset(&new_applications.executables_with(ApplicationEffect::Block))
    {
        found.push(Weakening::ApplicationRelaxed);
    }

    if schedules_relaxed(old, new) {
        found.push(Weakening::ScheduleRelaxed);
    }

    let old_rule = RepeatOffenderRule::from_value(old).unwrap_or_default();
    let new_rule = RepeatOffenderRule::from_value(new).unwrap_or_default();
    if new_rule.window_ms < old_rule.window_ms || new_rule.strike_limit > old_rule.strike_limit {
        found.push(Weakening::RepeatOffenderRelaxed);
    }
    found
}

/// Whether a switch that defaults to on went from on to off.
fn switched_off(old: &Value, new: &Value, pointer: &str) -> bool {
    let enabled = |policy: &Value| {
        policy
            .pointer(pointer)
            .and_then(Value::as_bool)
            .unwrap_or(true)
    };
    enabled(old) && !enabled(new)
}

/// The image risk threshold in percent; higher values enforce less.
fn risk_threshold(policy: &Value) -> u64 {
    policy
        .pointer("/recognition/sensitivity")
        .or_else(|| policy.pointer("/recognition/immediateThreshold"))
        .and_then(Value::as_u64)
        .unwrap_or(82)
        .min(100)
}

/// Whether some minute of the week that `old` restricted is restricted less,
/// or not at all, under `new`. Category members come from the application
/// rules, so those take part in the comparison as well.
fn schedules_relaxed(old: &Value, new: &Value) -> bool {
    if old.get("schedules") == new.get("schedules")
        && old.get("applications") == new.get("applications")
        && old.get("applicationCategories") == new.get("applicationCategories")
    {
        return false;
    }
    let old_schedule = NetworkSchedulePolicy::from_value(old).unwrap_or_default();
    if old_schedule.is_empty() {
        return false;
    }
    let new_schedule = NetworkSchedulePolicy::from_value(new).unwrap_or_default();
    (0..MINUTES_PER_WEEK).any(|minute| {
        let before = old_schedule.restriction_at(minute);
        before.is_restricted() && !covers(&new_schedule.restriction_at(minute), &before)
    })
}

/// Whether `after` blocks at least everything `before` blocks.
fn covers(after: &NetworkRestriction, before: &NetworkRestriction) -> bool {
    let network = after.block_all
        || (!before.block_all
            && before
                .network_executables
                .iter()
                .all(|executable| after.blocks_network(executable)));
    network
        && before
            .launch_executables
            .iter()
            .all(|executable| after.blocks_launch(executable))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{Weakening, policy_weakenings};

    fn base() -> Value {
        json!({
            "protectionEnabled": true,
            "launchAtStartup": true,
            "recognition": {
                "imageEnabled": true,
                "ocrEnabled": true,
                "titleMatchingEnabled": true,
                "rememberTitlesEnabled": true,
                "sensitivity": 80,
                "evidenceEnabled": false
            },
            "repeatOffender": {"windowMinutes": 20, "strikeLimit": 5},
            "websites": [
                {"id":"b","pattern":"blocked.example","action":"block","enabled":true}
            ],
            "applications": [
                {"id":"g","name":"Game","executable":"game.exe","category":"game","action":"block","enabled":true}
            ],
            "keywords": [
                {"id":"k","phrase":"赌球直播","category":"high_risk","enabled":true}
            ],
            "schedules": [{
                "id": "night", "name": "night", "target": "browsers",
                "blocks": [{"day": 0, "start": "21:00", "end": "23:00"}],
                "enabled": true
            }]
        })
    }

    fn weakenings(change: impl FnOnce(&mut Value)) -> Vec<Weakening> {
        let old = base();
        let mut new = base();
        change(&mut new);
        policy_weakenings(&old, &new)
    }

    #[test]
    fn unchanged_and_tightened_policies_are_not_weaker() {
        assert!(weakenings(|_| {}).is_empty());
        let tightened = weakenings(|policy| {
            policy["recognition"]["sensitivity"] = json!(70);
            policy["recognition"]["evidenceEnabled"] = json!(true);
            policy["repeatOffender"] = json!({"windowMinutes": 60, "strikeLimit": 3});
            policy["websites"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"b2","pattern":"other.example","action":"block","enabled":true}));
            policy["applications"].as_array_mut().unwrap().push(
                json!({"id":"g2","name":"Other","executable":"other.exe","category":"game","action":"block","enabled":true}),
            );
            policy["keywords"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"k2","phrase":"新词","category":"sensitive","enabled":true}));
            policy["schedules"][0]["blocks"][0]["end"] = json!("23:30");
            policy["schedules"][0]["target"] = json!("all_network");
        });
        assert!(tightened.is_empty(), "{tightened:?}");
    }

    #[test]
    fn switching_protection_or_recognition_off_is_weaker() {
        assert_eq!(
            weakenings(|policy| policy["protectionEnabled"] = json!(false)),
            vec![Weakening::ProtectionDisabled]
        );
        for switch in [
            "imageEnabled",
            "ocrEnabled",
            "titleMatchingEnabled",
            "rememberTitlesEnabled",
        ] {
            assert_eq!(
                weakenings(|policy| policy["recognition"][switch] = json!(false)),
                vec![Weakening::RecognitionDisabled],
                "{switch}"
            );
        }
        // Absent switches default to on, so deleting one is not weaker.
        assert!(
            weakenings(|policy| {
                policy["recognition"]
                    .as_object_mut()
                    .unwrap()
                    .remove("imageEnabled");
            })
            .is_empty()
        );
        assert_eq!(
            weakenings(|policy| policy["recognition"]["sensitivity"] = json!(90)),
            vec![Weakening::SensitivityLowered]
        );
    }

    #[test]
    fn turning_launch_at_startup_off_is_weaker() {
        assert_eq!(
            weakenings(|policy| policy["launchAtStartup"] = json!(false)),
            vec![Weakening::LaunchAtStartupDisabled]
        );
        let mut disabled = base();
        disabled["launchAtStartup"] = json!(false);
        assert!(policy_weakenings(&disabled, &base()).is_empty());
    }

    #[test]
    fn relaxing_website_application_and_keyword_rules_is_weaker() {
        assert_eq!(
            weakenings(|policy| policy["websites"][0]["enabled"] = json!(false)),
            vec![Weakening::WebsiteRelaxed]
        );
        assert_eq!(
            weakenings(|policy| policy["websites"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"a","pattern":"safe.example","action":"allow","enabled":true}))),
            vec![Weakening::WebsiteRelaxed]
        );
        assert_eq!(
            weakenings(|policy| policy["applications"][0]["action"] = json!("content_only")),
            vec![Weakening::ApplicationRelaxed]
        );
        assert_eq!(
            weakenings(|policy| {
                policy["applications"].as_array_mut().unwrap().push(
                json!({"id":"c","name":"Chrome","executable":"chrome.exe","category":"browser","action":"allow","enabled":true})
            )
            }),
            vec![Weakening::ApplicationRelaxed]
        );
        assert_eq!(
            weakenings(|policy| policy["keywords"] = json!([])),
            vec![Weakening::KeywordRelaxed]
        );
        assert_eq!(
            weakenings(|policy| policy["keywords"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"e","phrase":"医学","category":"exemption","enabled":true}))),
            vec![Weakening::KeywordRelaxed]
        );
        let addition =
            json!({"id":"a","term":"spicyclip","language":"en","category":"sexual","match":"word"});
        assert!(
            weakenings(|policy| policy["titleKeywordAdditions"] = json!([addition])).is_empty()
        );
        let mut with_addition = base();
        with_addition["titleKeywordAdditions"] = json!([addition]);
        assert_eq!(
            policy_weakenings(&with_addition, &base()),
            vec![Weakening::KeywordRelaxed]
        );
        assert!(
            weakenings(
                |policy| policy["recognition"]["ambiguousTitleKeywordsEnabled"] = json!(true)
            )
            .is_empty()
        );
        let mut ambiguous = base();
        ambiguous["recognition"]["ambiguousTitleKeywordsEnabled"] = json!(true);
        assert_eq!(
            policy_weakenings(&ambiguous, &base()),
            vec![Weakening::KeywordRelaxed]
        );
        // Editing a rule's id alone keeps the same enforcement.
        assert!(weakenings(|policy| policy["websites"][0]["id"] = json!("renamed")).is_empty());
    }

    #[test]
    fn shrinking_or_retargeting_a_schedule_is_weaker() {
        assert_eq!(
            weakenings(|policy| policy["schedules"][0]["blocks"][0]["end"] = json!("22:00")),
            vec![Weakening::ScheduleRelaxed]
        );
        assert_eq!(
            weakenings(|policy| policy["schedules"][0]["enabled"] = json!(false)),
            vec![Weakening::ScheduleRelaxed]
        );
        assert_eq!(
            weakenings(|policy| {
                policy["schedules"][0]["target"] = json!("applications");
                policy["schedules"][0]["applications"] = json!(["game.exe"]);
            }),
            vec![Weakening::ScheduleRelaxed]
        );
        // Moving a block to another day uncovers its original time.
        assert_eq!(
            weakenings(|policy| policy["schedules"][0]["blocks"][0]["day"] = json!(1)),
            vec![Weakening::ScheduleRelaxed]
        );
    }

    #[test]
    fn category_schedules_compare_by_their_members() {
        // The legacy browser target and the browser category are equivalent.
        assert!(
            weakenings(|policy| {
                policy["schedules"][0]["target"] = json!("categories");
                policy["schedules"][0]["categories"] = json!(["browser"]);
            })
            .is_empty()
        );
        let launch = |policy: &mut Value| {
            policy["applicationCategories"] = json!([{"id": "cat-chat", "name": "聊天"}]);
            policy["applications"].as_array_mut().unwrap().push(json!(
                {"id":"w","name":"WeChat","executable":"wechat.exe","category":"cat-chat","action":"content_only","enabled":true}
            ));
            policy["schedules"].as_array_mut().unwrap().push(json!({
                "id": "chat", "name": "chat", "action": "block_launch",
                "target": "categories", "categories": ["cat-chat"],
                "blocks": [{"day": 2, "start": "08:00", "end": "12:00"}],
                "enabled": true
            }));
        };
        assert!(weakenings(launch).is_empty());
        let mut old = base();
        launch(&mut old);
        let relaxed = |change: fn(&mut Value)| {
            let mut new = old.clone();
            change(&mut new);
            policy_weakenings(&old, &new)
        };
        // Moving a member out of a blocked category stops blocking it.
        assert_eq!(
            relaxed(|policy| policy["applications"][1]["category"] = json!("custom")),
            vec![Weakening::ScheduleRelaxed]
        );
        // Network blocking does not cover terminating the application.
        assert_eq!(
            relaxed(|policy| policy["schedules"][1]["action"] = json!("block_network")),
            vec![Weakening::ScheduleRelaxed]
        );
        // A new member blocks more.
        assert!(
            relaxed(|policy| policy["applications"].as_array_mut().unwrap().push(json!(
                {"id":"q","name":"QQ","executable":"qq.exe","category":"cat-chat","action":"content_only","enabled":true}
            )))
            .is_empty()
        );
    }

    #[test]
    fn relaxing_the_repeat_offender_rule_is_weaker() {
        assert_eq!(
            weakenings(|policy| policy["repeatOffender"]["strikeLimit"] = json!(6)),
            vec![Weakening::RepeatOffenderRelaxed]
        );
        assert_eq!(
            weakenings(|policy| policy["repeatOffender"]["windowMinutes"] = json!(10)),
            vec![Weakening::RepeatOffenderRelaxed]
        );
    }

    #[test]
    fn several_relaxations_are_all_reported() {
        assert_eq!(
            weakenings(|policy| {
                policy["protectionEnabled"] = json!(false);
                policy["websites"] = json!([]);
                policy["schedules"] = json!([]);
            }),
            vec![
                Weakening::ProtectionDisabled,
                Weakening::WebsiteRelaxed,
                Weakening::ScheduleRelaxed
            ]
        );
    }
}
