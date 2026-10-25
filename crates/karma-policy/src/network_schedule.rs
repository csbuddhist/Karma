use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

use crate::{
    application_category::{ApplicationCategories, BROWSER_CATEGORY, category_members},
    application_policy::{path_components, trailing_components_match},
};

pub const MINUTES_PER_DAY: u16 = 1440;
pub const MINUTES_PER_WEEK: u16 = 7 * MINUTES_PER_DAY;

const MAX_SCHEDULE_RULES: usize = 64;
const MAX_BLOCKS_PER_RULE: usize = 96;
const MAX_APPLICATIONS_PER_RULE: usize = 64;
const MAX_CATEGORIES_PER_RULE: usize = 32;
const MAX_RULE_ID_CHARS: usize = 128;
const MAX_RULE_NAME_CHARS: usize = 128;
const MAX_EXECUTABLE_CHARS: usize = 2048;

/// Browser executables that the `browser` category always covers, in addition
/// to console application rules categorized as `browser`. Entries are the
/// lowercase final path component of the process image.
pub const KNOWN_BROWSER_EXECUTABLES: &[&str] = &[
    "chrome.exe",
    "chromium.exe",
    "msedge.exe",
    "iexplore.exe",
    "firefox.exe",
    "waterfox.exe",
    "librewolf.exe",
    "brave.exe",
    "opera.exe",
    "opera_gx.exe",
    "vivaldi.exe",
    "arc.exe",
    "thorium.exe",
    "yandex.exe",
    "whale.exe",
    "maxthon.exe",
    "360se.exe",
    "360chrome.exe",
    "360chromex.exe",
    "qqbrowser.exe",
    "sogouexplorer.exe",
    "2345explorer.exe",
    "liebao.exe",
    "ucbrowser.exe",
    "quark.exe",
    // macOS application executables (`Contents/MacOS/<name>`); without an
    // `.exe` suffix they never match a Windows process image.
    "google chrome",
    "google chrome beta",
    "google chrome canary",
    "chromium",
    "microsoft edge",
    "microsoft edge beta",
    "microsoft edge dev",
    "microsoft edge canary",
    "safari",
    "safari technology preview",
    "firefox",
    "brave browser",
    "opera",
    "vivaldi",
    "arc",
    "orion",
    "duckduckgo",
    "zen",
    "waterfox",
    "librewolf",
    "thorium",
    "yandex",
];

/// Which applications a schedule rule covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkTarget {
    /// Saved by consoles before categories existed; the same as
    /// `categories: ["browser"]`.
    Browsers,
    /// Members of the listed application categories.
    Categories,
    /// Only the executables listed on the schedule rule.
    Applications,
    /// Every application that is not part of the operating system. Only
    /// network blocking supports this target.
    AllNetwork,
}

/// What a schedule rule does to the applications it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleAction {
    /// The applications lose network access.
    #[default]
    BlockNetwork,
    /// Running instances of the applications are terminated.
    BlockLaunch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum NetworkScheduleError {
    #[error("network schedule policy is invalid")]
    InvalidPolicy,
}

/// The console-managed weekly blocking schedules (`policy.schedules`).
///
/// Days are numbered Monday = 0 through Sunday = 6 and times are local wall
/// clock minutes. A block whose end is not after its start continues into the
/// following day, so `22:00–06:00` on Sunday ends at 06:00 on Monday.
/// Category targets resolve to the enabled application rules of each category
/// when the policy is read.
#[derive(Debug, Clone, Default)]
pub struct NetworkSchedulePolicy {
    rules: Vec<StoredRule>,
}

#[derive(Debug, Clone)]
struct StoredRule {
    id: String,
    action: ScheduleAction,
    block_all: bool,
    executables: BTreeSet<String>,
    ranges: Vec<WeekRange>,
}

/// A half-open span of `length` minutes starting at `start`, wrapping around
/// the end of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WeekRange {
    start: u16,
    length: u16,
}

impl WeekRange {
    fn offset_of(self, minute: u16) -> Option<u16> {
        let offset = (minute + MINUTES_PER_WEEK - self.start) % MINUTES_PER_WEEK;
        (offset < self.length).then_some(offset)
    }
}

/// The blocking in force at one instant.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkRestriction {
    /// Block network access for every non-system application.
    pub block_all: bool,
    /// Executables (lowercase file names or paths) that lose network access.
    pub network_executables: Vec<String>,
    /// Executables (lowercase file names or paths) that must not run.
    pub launch_executables: Vec<String>,
    /// Identifiers of the schedule rules that are currently active.
    pub rule_ids: Vec<String>,
}

impl NetworkRestriction {
    pub fn is_restricted(&self) -> bool {
        !self.rule_ids.is_empty()
    }

    /// Whether an application image path loses network access. `block_all`
    /// covers every path; enforcement must still exempt operating-system
    /// components and Karma itself.
    pub fn blocks_network(&self, executable_path: &str) -> bool {
        self.block_all || matches_any(&self.network_executables, executable_path)
    }

    /// Whether an application image path must not run. Enforcement must still
    /// exempt operating-system components and Karma itself.
    pub fn blocks_launch(&self, executable_path: &str) -> bool {
        matches_any(&self.launch_executables, executable_path)
    }
}

fn matches_any(rules: &[String], executable_path: &str) -> bool {
    let observed = path_components(executable_path);
    rules
        .iter()
        .any(|rule| trailing_components_match(&observed, &path_components(rule)))
}

/// Whether `executable_path` is a known browser or matches an enabled console
/// application rule categorized as a browser.
pub fn is_browser_executable(policy: &Value, executable_path: &str) -> bool {
    matches_browser(
        category_members(policy).get(BROWSER_CATEGORY),
        executable_path,
    )
}

/// Whether `executable_path` is a known browser or matches one of the
/// browser-category `members` from `category_members`.
pub(crate) fn matches_browser(members: Option<&BTreeSet<String>>, executable_path: &str) -> bool {
    let observed = path_components(executable_path);
    observed.last().is_some_and(|file_name| {
        KNOWN_BROWSER_EXECUTABLES
            .iter()
            .any(|browser| file_name == browser)
    }) || members.is_some_and(|members| {
        members
            .iter()
            .any(|rule| trailing_components_match(&observed, &path_components(rule)))
    })
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ScheduleRuleInput {
    Blocks(BlockRuleInput),
    Legacy(LegacyRuleInput),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockRuleInput {
    id: String,
    name: String,
    #[serde(default)]
    action: ScheduleAction,
    target: NetworkTarget,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    applications: Vec<String>,
    blocks: Vec<BlockInput>,
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockInput {
    day: u8,
    start: String,
    end: String,
}

/// The single-range rule shape saved by console builds before multi-block
/// schedules existed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRuleInput {
    id: String,
    name: String,
    days: Vec<u8>,
    start: String,
    end: String,
    target: LegacyTarget,
    enabled: bool,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum LegacyTarget {
    Internet,
    BrowserPlayer,
    AllRestricted,
}

/// A rule after its shape-specific fields are read, before validation.
struct ParsedRule {
    id: String,
    name: String,
    action: ScheduleAction,
    target: NetworkTarget,
    categories: Vec<String>,
    applications: Vec<String>,
    ranges: Vec<WeekRange>,
    enabled: bool,
}

impl NetworkSchedulePolicy {
    pub fn from_value(policy: &Value) -> Result<Self, NetworkScheduleError> {
        let raw_rules = policy
            .get("schedules")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let raw_rules: Vec<ScheduleRuleInput> =
            serde_json::from_value(raw_rules).map_err(|_| NetworkScheduleError::InvalidPolicy)?;
        if raw_rules.len() > MAX_SCHEDULE_RULES {
            return Err(NetworkScheduleError::InvalidPolicy);
        }
        let categories = ApplicationCategories::from_value(policy)
            .map_err(|_| NetworkScheduleError::InvalidPolicy)?;
        let members = category_members(policy);
        let mut rules = Vec::new();
        for raw in raw_rules {
            if let Some(rule) = resolve_rule(parse_rule(raw)?, &categories, &members)? {
                rules.push(rule);
            }
        }
        Ok(Self { rules })
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The restriction in force at a local minute of the week.
    pub fn restriction_at(&self, minute_of_week: u16) -> NetworkRestriction {
        let minute = minute_of_week % MINUTES_PER_WEEK;
        let mut restriction = NetworkRestriction::default();
        let mut network = BTreeSet::new();
        let mut launch = BTreeSet::new();
        for rule in &self.rules {
            if !rule
                .ranges
                .iter()
                .any(|range| range.offset_of(minute).is_some())
            {
                continue;
            }
            restriction.rule_ids.push(rule.id.clone());
            restriction.block_all |= rule.block_all;
            match rule.action {
                ScheduleAction::BlockNetwork => network.extend(rule.executables.iter().cloned()),
                ScheduleAction::BlockLaunch => launch.extend(rule.executables.iter().cloned()),
            }
        }
        restriction.network_executables = network.into_iter().collect();
        restriction.launch_executables = launch.into_iter().collect();
        restriction
    }

    /// Minutes from `minute_of_week` until no schedule block is active, or
    /// `None` when nothing is active now. A schedule that never lifts reports
    /// one full week.
    pub fn minutes_until_clear(&self, minute_of_week: u16) -> Option<u32> {
        let mut minute = minute_of_week % MINUTES_PER_WEEK;
        let mut elapsed: u32 = 0;
        loop {
            let remaining = self
                .rules
                .iter()
                .flat_map(|rule| rule.ranges.iter())
                .filter_map(|range| {
                    range
                        .offset_of(minute)
                        .map(|offset| u32::from(range.length - offset))
                })
                .max();
            match remaining {
                None if elapsed == 0 => return None,
                None => return Some(elapsed),
                Some(step) => {
                    elapsed += step;
                    if elapsed >= u32::from(MINUTES_PER_WEEK) {
                        return Some(u32::from(MINUTES_PER_WEEK));
                    }
                    minute = ((u32::from(minute) + step) % u32::from(MINUTES_PER_WEEK)) as u16;
                }
            }
        }
    }
}

fn parse_rule(raw: ScheduleRuleInput) -> Result<ParsedRule, NetworkScheduleError> {
    match raw {
        ScheduleRuleInput::Blocks(rule) => {
            if rule.blocks.len() > MAX_BLOCKS_PER_RULE {
                return Err(NetworkScheduleError::InvalidPolicy);
            }
            let ranges = rule
                .blocks
                .iter()
                .map(|block| week_range(block.day, &block.start, &block.end))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ParsedRule {
                id: rule.id,
                name: rule.name,
                action: rule.action,
                target: rule.target,
                categories: rule.categories,
                applications: rule.applications,
                ranges,
                enabled: rule.enabled,
            })
        }
        ScheduleRuleInput::Legacy(rule) => {
            if rule.days.len() > 7 {
                return Err(NetworkScheduleError::InvalidPolicy);
            }
            let ranges = rule
                .days
                .iter()
                .map(|day| week_range(*day, &rule.start, &rule.end))
                .collect::<Result<Vec<_>, _>>()?;
            let target = match rule.target {
                LegacyTarget::BrowserPlayer => NetworkTarget::Browsers,
                LegacyTarget::Internet | LegacyTarget::AllRestricted => NetworkTarget::AllNetwork,
            };
            Ok(ParsedRule {
                id: rule.id,
                name: rule.name,
                action: ScheduleAction::BlockNetwork,
                target,
                categories: Vec::new(),
                applications: Vec::new(),
                ranges,
                enabled: rule.enabled,
            })
        }
    }
}

fn resolve_rule(
    rule: ParsedRule,
    categories: &ApplicationCategories,
    members: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Option<StoredRule>, NetworkScheduleError> {
    if rule.id.is_empty()
        || rule.id.chars().count() > MAX_RULE_ID_CHARS
        || rule.name.chars().count() > MAX_RULE_NAME_CHARS
        || rule.applications.len() > MAX_APPLICATIONS_PER_RULE
        || rule.categories.len() > MAX_CATEGORIES_PER_RULE
        || rule
            .categories
            .iter()
            .any(|category| !categories.contains(category))
    {
        return Err(NetworkScheduleError::InvalidPolicy);
    }
    let mut applications = BTreeSet::new();
    for executable in &rule.applications {
        let components = path_components(executable);
        if executable.chars().count() > MAX_EXECUTABLE_CHARS || components.is_empty() {
            return Err(NetworkScheduleError::InvalidPolicy);
        }
        applications.insert(components.join("\\"));
    }
    let covered_categories = match rule.target {
        NetworkTarget::Browsers => vec![BROWSER_CATEGORY.to_owned()],
        NetworkTarget::Categories => rule.categories,
        NetworkTarget::Applications | NetworkTarget::AllNetwork => Vec::new(),
    };
    let block_all = rule.target == NetworkTarget::AllNetwork;
    if (rule.target == NetworkTarget::Applications && applications.is_empty())
        || (rule.target == NetworkTarget::Categories && covered_categories.is_empty())
        || (block_all && rule.action == ScheduleAction::BlockLaunch)
    {
        return Err(NetworkScheduleError::InvalidPolicy);
    }
    if !rule.enabled || rule.ranges.is_empty() {
        return Ok(None);
    }
    let mut executables = if rule.target == NetworkTarget::Applications {
        applications
    } else {
        BTreeSet::new()
    };
    for category in &covered_categories {
        if category == BROWSER_CATEGORY {
            executables.extend(
                KNOWN_BROWSER_EXECUTABLES
                    .iter()
                    .map(|name| (*name).to_owned()),
            );
        }
        if let Some(category_members) = members.get(category) {
            executables.extend(category_members.iter().cloned());
        }
    }
    Ok(Some(StoredRule {
        id: rule.id,
        action: rule.action,
        block_all,
        executables,
        ranges: rule.ranges,
    }))
}

fn week_range(day: u8, start: &str, end: &str) -> Result<WeekRange, NetworkScheduleError> {
    if day > 6 {
        return Err(NetworkScheduleError::InvalidPolicy);
    }
    let start = parse_clock(start)?;
    let end = parse_clock(end)?;
    if start == end {
        return Err(NetworkScheduleError::InvalidPolicy);
    }
    let length = (end + MINUTES_PER_DAY - start) % MINUTES_PER_DAY;
    Ok(WeekRange {
        start: u16::from(day) * MINUTES_PER_DAY + start,
        length,
    })
}

fn parse_clock(value: &str) -> Result<u16, NetworkScheduleError> {
    let (hours, minutes) = value
        .split_once(':')
        .ok_or(NetworkScheduleError::InvalidPolicy)?;
    if hours.len() != 2 || minutes.len() != 2 {
        return Err(NetworkScheduleError::InvalidPolicy);
    }
    let hours: u16 = hours
        .parse()
        .map_err(|_| NetworkScheduleError::InvalidPolicy)?;
    let minutes: u16 = minutes
        .parse()
        .map_err(|_| NetworkScheduleError::InvalidPolicy)?;
    if hours > 23 || minutes > 59 {
        return Err(NetworkScheduleError::InvalidPolicy);
    }
    Ok(hours * 60 + minutes)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn at(day: u16, hour: u16, minute: u16) -> u16 {
        day * MINUTES_PER_DAY + hour * 60 + minute
    }

    fn policy(schedules: Value) -> NetworkSchedulePolicy {
        NetworkSchedulePolicy::from_value(&json!({ "schedules": schedules })).unwrap()
    }

    #[test]
    fn multiple_blocks_per_day_restrict_only_inside_each_block() {
        let value = policy(json!([{
            "id": "study",
            "name": "学习时间",
            "target": "browsers",
            "blocks": [
                {"day": 0, "start": "08:00", "end": "12:00"},
                {"day": 0, "start": "14:00", "end": "17:30"},
                {"day": 2, "start": "19:00", "end": "21:00"}
            ],
            "enabled": true
        }]));
        assert!(
            value
                .restriction_at(at(0, 9, 0))
                .blocks_network("chrome.exe")
        );
        assert!(!value.restriction_at(at(0, 12, 0)).is_restricted());
        assert!(!value.restriction_at(at(0, 13, 59)).is_restricted());
        assert!(value.restriction_at(at(0, 17, 29)).is_restricted());
        assert!(!value.restriction_at(at(0, 17, 30)).is_restricted());
        assert!(!value.restriction_at(at(1, 9, 0)).is_restricted());
        assert!(value.restriction_at(at(2, 20, 0)).is_restricted());
    }

    #[test]
    fn overnight_block_on_sunday_continues_into_monday() {
        let value = policy(json!([{
            "id": "night",
            "name": "夜间",
            "target": "all_network",
            "blocks": [{"day": 6, "start": "22:00", "end": "06:00"}],
            "enabled": true
        }]));
        assert!(value.restriction_at(at(6, 23, 0)).block_all);
        assert!(value.restriction_at(at(0, 5, 59)).block_all);
        assert!(!value.restriction_at(at(0, 6, 0)).is_restricted());
        assert!(!value.restriction_at(at(6, 21, 59)).is_restricted());
    }

    #[test]
    fn application_target_blocks_only_listed_executables() {
        let value = policy(json!([{
            "id": "games",
            "name": "游戏",
            "target": "applications",
            "applications": ["Steam.exe", r"D:\Games\Launcher\launcher.exe"],
            "blocks": [{"day": 4, "start": "18:00", "end": "22:00"}],
            "enabled": true
        }]));
        let restriction = value.restriction_at(at(4, 19, 0));
        assert!(!restriction.block_all);
        assert!(restriction.blocks_network(r"C:\Program Files (x86)\Steam\steam.exe"));
        assert!(restriction.blocks_network(r"d:\games\launcher\LAUNCHER.EXE"));
        assert!(!restriction.blocks_network(r"C:\Other\launcher.exe"));
        assert!(!restriction.blocks_network(r"C:\Program Files\Google\Chrome\chrome.exe"));
        assert!(!restriction.blocks_launch(r"C:\Program Files (x86)\Steam\steam.exe"));
    }

    #[test]
    fn browser_target_covers_known_and_console_browsers() {
        let value = NetworkSchedulePolicy::from_value(&json!({
            "applications": [
                {"id":"custom","name":"Custom","executable":r"C:\Tools\mybrowser.exe","category":"browser","action":"content_only","enabled":true},
                {"id":"placeholder","name":"浏览器","executable":"受支持浏览器","category":"browser","action":"content_only","enabled":true}
            ],
            "schedules": [{
                "id": "b",
                "name": "浏览器",
                "target": "browsers",
                "blocks": [{"day": 1, "start": "00:00", "end": "23:59"}],
                "enabled": true
            }]
        }))
        .unwrap();
        let restriction = value.restriction_at(at(1, 10, 0));
        assert!(
            restriction.blocks_network(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
        );
        assert!(restriction.blocks_network(r"C:\Tools\MyBrowser.exe"));
        assert!(!restriction.blocks_network(r"C:\Windows\System32\svchost.exe"));
        let executables = &restriction.network_executables;
        assert!(executables.contains(&"msedge.exe".to_owned()));
        assert!(executables.contains(&r"c:\tools\mybrowser.exe".to_owned()));
        assert!(!executables.iter().any(|name| name.contains("受支持")));
    }

    #[test]
    fn category_target_covers_enabled_members_of_each_category() {
        let value = NetworkSchedulePolicy::from_value(&json!({
            "applicationCategories": [{"id": "cat-chat", "name": "聊天"}],
            "applications": [
                {"id":"a","name":"WeChat","executable":r"C:\Apps\WeChat.exe","category":"cat-chat","action":"content_only","enabled":true},
                {"id":"b","name":"QQ","executable":"qq.exe","category":"cat-chat","action":"allow","enabled":true},
                {"id":"c","name":"Off","executable":"old.exe","category":"cat-chat","action":"content_only","enabled":false},
                {"id":"d","name":"Game","executable":"game.exe","category":"game","action":"content_only","enabled":true}
            ],
            "schedules": [{
                "id": "s", "name": "s", "target": "categories",
                "categories": ["cat-chat", "browser"],
                "blocks": [{"day": 1, "start": "08:00", "end": "12:00"}],
                "enabled": true
            }]
        }))
        .unwrap();
        let restriction = value.restriction_at(at(1, 9, 0));
        for blocked in [r"c:\apps\wechat.exe", r"D:\QQ\QQ.exe", "msedge.exe"] {
            assert!(restriction.blocks_network(blocked), "{blocked}");
        }
        for open in ["old.exe", "game.exe", r"D:\Other\WeChat.exe"] {
            assert!(!restriction.blocks_network(open), "{open}");
        }
    }

    #[test]
    fn block_launch_rules_list_executables_that_must_not_run() {
        let value = NetworkSchedulePolicy::from_value(&json!({
            "applications": [
                {"id":"g","name":"Game","executable":r"D:\Games\game.exe","category":"game","action":"content_only","enabled":true}
            ],
            "schedules": [
                {"id":"launch","name":"游戏","action":"block_launch","target":"categories","categories":["game"],
                 "blocks":[{"day":5,"start":"09:00","end":"18:00"}],"enabled":true},
                {"id":"net","name":"浏览器","target":"applications","applications":["steam.exe"],
                 "blocks":[{"day":5,"start":"10:00","end":"11:00"}],"enabled":true}
            ]
        }))
        .unwrap();
        let morning = value.restriction_at(at(5, 9, 30));
        assert_eq!(morning.rule_ids, vec!["launch"]);
        assert!(morning.blocks_launch(r"d:\games\GAME.exe"));
        assert!(!morning.blocks_network(r"d:\games\game.exe"));
        let both = value.restriction_at(at(5, 10, 30));
        assert!(both.blocks_launch(r"d:\games\game.exe"));
        assert!(both.blocks_network("steam.exe") && !both.blocks_launch("steam.exe"));
    }

    #[test]
    fn browser_executables_include_known_and_console_browsers() {
        let policy = json!({
            "applications": [
                {"id":"custom","name":"Custom","executable":r"C:\Tools\mybrowser.exe","category":"browser","action":"content_only","enabled":true},
                {"id":"off","name":"Off","executable":r"C:\Tools\oldbrowser.exe","category":"browser","action":"content_only","enabled":false},
                {"id":"reader","name":"Reader","executable":r"C:\Tools\reader.exe","category":"custom","action":"allow","enabled":true}
            ]
        });
        for browser in [
            r"C:\Program Files\Google\Chrome\Application\CHROME.EXE",
            r"c:\tools\mybrowser.exe",
        ] {
            assert!(is_browser_executable(&policy, browser), "{browser}");
        }
        for other in [r"C:\Tools\oldbrowser.exe", r"C:\Tools\reader.exe"] {
            assert!(!is_browser_executable(&policy, other), "{other}");
        }
    }

    #[test]
    fn overlapping_rules_combine_targets() {
        let value = policy(json!([
            {"id":"a","name":"A","target":"browsers","blocks":[{"day":3,"start":"10:00","end":"11:00"}],"enabled":true},
            {"id":"b","name":"B","target":"applications","applications":["game.exe"],"blocks":[{"day":3,"start":"10:30","end":"12:00"}],"enabled":true},
            {"id":"c","name":"C","target":"all_network","blocks":[{"day":3,"start":"10:45","end":"10:50"}],"enabled":false}
        ]));
        let restriction = value.restriction_at(at(3, 10, 46));
        assert_eq!(restriction.rule_ids, vec!["a", "b"]);
        assert!(!restriction.block_all);
        assert!(restriction.blocks_network("chrome.exe") && restriction.blocks_network("game.exe"));
    }

    #[test]
    fn minutes_until_clear_follows_chained_blocks() {
        let value = policy(json!([
            {"id":"a","name":"A","target":"browsers","blocks":[
                {"day":0,"start":"20:00","end":"22:00"},
                {"day":0,"start":"21:30","end":"23:00"}
            ],"enabled":true},
            {"id":"b","name":"B","target":"all_network","blocks":[{"day":0,"start":"23:00","end":"01:00"}],"enabled":true}
        ]));
        assert_eq!(value.minutes_until_clear(at(0, 19, 0)), None);
        assert_eq!(value.minutes_until_clear(at(0, 21, 0)), Some(4 * 60));
        assert_eq!(value.minutes_until_clear(at(1, 0, 30)), Some(30));
    }

    #[test]
    fn schedule_that_never_lifts_reports_one_week() {
        let blocks: Vec<Value> = (0..7)
            .map(|day| json!({"day": day, "start": "00:00", "end": "00:00"}))
            .collect();
        assert!(
            NetworkSchedulePolicy::from_value(&json!({"schedules":[{"id":"x","name":"x","target":"all_network","blocks":blocks,"enabled":true}]}))
                .is_err()
        );
        let blocks: Vec<Value> = (0..7)
            .flat_map(|day| {
                [
                    json!({"day": day, "start": "00:00", "end": "12:00"}),
                    json!({"day": day, "start": "12:00", "end": "00:00"}),
                ]
            })
            .collect();
        let value = policy(
            json!([{"id":"x","name":"x","target":"all_network","blocks":blocks,"enabled":true}]),
        );
        assert_eq!(
            value.minutes_until_clear(at(2, 3, 0)),
            Some(u32::from(MINUTES_PER_WEEK))
        );
    }

    #[test]
    fn legacy_single_range_rules_are_still_enforced() {
        let value = policy(json!([{
            "id": "legacy",
            "name": "旧规则",
            "days": [0, 1, 2, 3, 4],
            "start": "21:00",
            "end": "07:00",
            "target": "browser_player",
            "enabled": true
        }]));
        assert!(
            value
                .restriction_at(at(4, 23, 0))
                .blocks_network("firefox.exe")
        );
        assert!(
            value
                .restriction_at(at(5, 6, 0))
                .blocks_network("firefox.exe")
        );
        assert!(!value.restriction_at(at(5, 21, 0)).is_restricted());
    }

    #[test]
    fn malformed_schedules_are_rejected() {
        for schedules in [
            json!([{"id":"x","name":"x","target":"browsers","blocks":[{"day":7,"start":"08:00","end":"09:00"}],"enabled":true}]),
            json!([{"id":"x","name":"x","target":"browsers","blocks":[{"day":1,"start":"24:00","end":"09:00"}],"enabled":true}]),
            json!([{"id":"x","name":"x","target":"browsers","blocks":[{"day":1,"start":"8:00","end":"09:00"}],"enabled":true}]),
            json!([{"id":"x","name":"x","target":"applications","applications":[],"blocks":[{"day":1,"start":"08:00","end":"09:00"}],"enabled":true}]),
            json!([{"id":"x","name":"x","target":"categories","categories":[],"blocks":[],"enabled":true}]),
            json!([{"id":"x","name":"x","target":"categories","categories":["cat-missing"],"blocks":[],"enabled":true}]),
            json!([{"id":"x","name":"x","action":"block_launch","target":"all_network","blocks":[],"enabled":true}]),
            json!([{"id":"x","name":"x","action":"uninstall","target":"browsers","blocks":[],"enabled":true}]),
            json!([{"id":"","name":"x","target":"browsers","blocks":[],"enabled":true}]),
            json!([{"id":"x","name":"x","target":"everything","blocks":[],"enabled":true}]),
            json!([{"id":"x","name":"x","target":"browsers","blocks":[],"enabled":true,"extra":1}]),
        ] {
            assert_eq!(
                NetworkSchedulePolicy::from_value(&json!({ "schedules": schedules })).unwrap_err(),
                NetworkScheduleError::InvalidPolicy,
                "{schedules}"
            );
        }
    }
}
