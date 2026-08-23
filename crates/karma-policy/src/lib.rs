#![forbid(unsafe_code)]

mod application;
mod application_policy;
mod context;
mod engine;
mod risk;
mod schedule;

pub use application::{
    ApplicationFacts, ApplicationMatcher, ApplicationRule, RuleEffect, resolve_application,
};
pub use application_policy::{
    ApplicationEffect, ApplicationPolicy, ApplicationPolicyError, context_enforcement,
};
pub use context::{
    ContextPolicy, ContextPolicyError, ContextVerdict, WebsiteRule, WebsiteRuleAction,
};
pub use engine::{DecisionEngine, EvaluationInput, EvaluationResult};
pub use risk::{RiskOutcome, RiskState};
pub use schedule::{MinuteRange, ScheduleError, WeeklySchedule};
