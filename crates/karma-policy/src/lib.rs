#![forbid(unsafe_code)]

mod application;
mod application_policy;
mod context;
mod engine;
mod network_schedule;
mod remembered_title;
mod repeat_offender;
mod risk;
mod schedule;
mod weakening;

pub use application::{
    ApplicationFacts, ApplicationMatcher, ApplicationRule, RuleEffect, resolve_application,
};
pub use application_policy::{
    ApplicationEffect, ApplicationPolicy, ApplicationPolicyError, allow_application,
    context_enforcement,
};
pub use context::{
    ContextPolicy, ContextPolicyError, ContextVerdict, KeywordMatch, MAX_TITLE_KEYWORD_ADDITIONS,
    MAX_TITLE_KEYWORD_TERM_CHARS, WebsiteRule, WebsiteRuleAction,
};
pub use engine::{DecisionEngine, EvaluationInput, EvaluationResult};
pub use network_schedule::{
    KNOWN_BROWSER_EXECUTABLES, MINUTES_PER_DAY, MINUTES_PER_WEEK, NetworkRestriction,
    NetworkScheduleError, NetworkSchedulePolicy, NetworkTarget,
};
pub use remembered_title::{
    MAX_MISJUDGED_TITLES, MAX_REMEMBERED_TITLES, RememberedTitle, TitleReason, remember_title,
    remember_titles_enabled, rememberable_title_key, title_digest, title_key,
};
pub use repeat_offender::{
    DEFAULT_REPEAT_OFFENDER_STRIKE_LIMIT, DEFAULT_REPEAT_OFFENDER_WINDOW_MINUTES,
    MAX_REPEAT_OFFENDER_STRIKE_LIMIT, MAX_REPEAT_OFFENDER_WINDOW_MINUTES,
    MIN_REPEAT_OFFENDER_STRIKE_LIMIT, MIN_REPEAT_OFFENDER_WINDOW_MINUTES, REPEAT_OFFENDER_BAN_MS,
    RepeatOffenderError, RepeatOffenderRule, StrikeRecord, active_bans, is_banned,
    normalize_executable, record_closure,
};
pub use risk::{RiskOutcome, RiskState};
pub use schedule::{MinuteRange, ScheduleError, WeeklySchedule};
pub use weakening::{Weakening, policy_weakenings};
