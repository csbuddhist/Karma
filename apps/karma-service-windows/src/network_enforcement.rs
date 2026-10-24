//! Translate schedule restrictions into an idempotent, atomically replaced WFP policy.

use std::collections::BTreeSet;

use karma_policy::NetworkRestriction;

#[cfg(windows)]
pub mod windows;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ApplicationMatcher {
    FullPath(String),
    /// Includes the leading separator so `chrome.exe` cannot match `mychrome.exe`.
    Suffix(String),
}

impl ApplicationMatcher {
    fn new(executable: &str) -> Self {
        let path = executable
            .split(['\\', '/'])
            .filter(|part| !part.is_empty())
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
            .join("\\");
        if path.as_bytes().get(1) == Some(&b':') || executable.starts_with("\\\\") {
            Self::FullPath(if executable.starts_with("\\\\") {
                format!("\\\\{path}")
            } else {
                path
            })
        } else {
            Self::Suffix(format!("\\{path}"))
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FilterPlan {
    block_all: bool,
    applications: BTreeSet<ApplicationMatcher>,
}

impl FilterPlan {
    fn from_restriction(restriction: &NetworkRestriction) -> Self {
        Self {
            block_all: restriction.block_all,
            applications: if restriction.block_all {
                BTreeSet::new()
            } else {
                restriction
                    .network_executables
                    .iter()
                    .map(|path| ApplicationMatcher::new(path))
                    .collect()
            },
        }
    }

    fn is_empty(&self) -> bool {
        !self.block_all && self.applications.is_empty()
    }
}

trait FilterBackend {
    type Error;

    /// On failure the previous policy must remain intact.
    fn replace(&mut self, plan: &FilterPlan) -> Result<(), Self::Error>;
}

struct Reconciler<B> {
    backend: B,
    applied: Option<FilterPlan>,
}

impl<B: FilterBackend> Reconciler<B> {
    fn new(backend: B) -> Self {
        Self {
            backend,
            // A fresh connection must reconcile persisted rules even if the plan is empty.
            applied: None,
        }
    }

    fn reconcile(&mut self, plan: FilterPlan) -> Result<(), B::Error> {
        if self.applied.as_ref() != Some(&plan) {
            self.backend.replace(&plan)?;
            self.applied = Some(plan);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use karma_policy::NetworkSchedulePolicy;
    use serde_json::json;

    #[derive(Default)]
    struct Backend {
        plans: Vec<FilterPlan>,
        fail: bool,
    }

    impl FilterBackend for Backend {
        type Error = ();

        fn replace(&mut self, plan: &FilterPlan) -> Result<(), ()> {
            if self.fail {
                return Err(());
            }
            self.plans.push(plan.clone());
            Ok(())
        }
    }

    fn plan(target: &str) -> FilterPlan {
        let policy = NetworkSchedulePolicy::from_value(&json!({
            "schedules": [{"id":"school", "name":"School", "target":target,
                "applications":["player.exe", "C:/Apps/Chat.exe", "player.exe"],
                "blocks":[{"day":0, "start":"08:00", "end":"10:00"}], "enabled":true}],
            "applications":[{"id":"browser", "name":"Browser", "executable":"Custom/Browser.exe",
                "category":"browser", "action":"content_only", "enabled":true}]
        }))
        .unwrap();
        FilterPlan::from_restriction(&policy.restriction_at(9 * 60))
    }

    #[test]
    fn browser_plan_covers_known_and_console_browsers_before_process_start() {
        let browsers = plan("browsers");
        assert!(
            browsers
                .applications
                .contains(&ApplicationMatcher::Suffix("\\chrome.exe".into()))
        );
        assert!(
            browsers
                .applications
                .contains(&ApplicationMatcher::Suffix("\\custom\\browser.exe".into()))
        );
        assert!(
            !browsers
                .applications
                .contains(&ApplicationMatcher::Suffix("\\player.exe".into()))
        );
    }

    #[test]
    fn application_plan_preserves_full_paths_and_deduplicates_names() {
        let applications = plan("applications");
        assert_eq!(
            applications.applications,
            BTreeSet::from([
                ApplicationMatcher::Suffix("\\player.exe".into()),
                ApplicationMatcher::FullPath("c:\\apps\\chat.exe".into()),
            ])
        );
        assert_eq!(
            ApplicationMatcher::new("CHROME.EXE"),
            ApplicationMatcher::Suffix("\\chrome.exe".into())
        );
    }

    #[test]
    fn launch_blocking_rules_add_no_network_filters() {
        let policy = NetworkSchedulePolicy::from_value(&json!({"schedules":[
            {"id":"games", "name":"Games", "action":"block_launch", "target":"applications",
                "applications":["game.exe"], "enabled":true,
                "blocks":[{"day":0,"start":"08:00","end":"10:00"}]}
        ]}))
        .unwrap();
        let restriction = policy.restriction_at(9 * 60);
        assert!(restriction.is_restricted());
        assert!(FilterPlan::from_restriction(&restriction).is_empty());
    }

    #[test]
    fn all_network_uses_a_catch_all_instead_of_a_process_snapshot() {
        let all = plan("all_network");
        assert!(all.block_all && all.applications.is_empty());
        assert!(!all.is_empty());
    }

    #[test]
    fn overlapping_browser_and_application_restrictions_are_combined() {
        let policy = NetworkSchedulePolicy::from_value(&json!({"schedules":[
            {"id":"browser", "name":"Browser", "target":"browsers", "enabled":true,
                "blocks":[{"day":0,"start":"08:00","end":"10:00"}]},
            {"id":"chat", "name":"Chat", "target":"applications", "applications":["chat.exe"],
                "enabled":true, "blocks":[{"day":0,"start":"09:00","end":"11:00"}]}
        ]}))
        .unwrap();
        let together = FilterPlan::from_restriction(&policy.restriction_at(9 * 60));
        assert!(
            together
                .applications
                .contains(&ApplicationMatcher::new("chrome.exe"))
        );
        assert!(
            together
                .applications
                .contains(&ApplicationMatcher::new("chat.exe"))
        );
        let later = FilterPlan::from_restriction(&policy.restriction_at(10 * 60));
        assert_eq!(
            later.applications,
            BTreeSet::from([ApplicationMatcher::new("chat.exe")])
        );
        assert!(FilterPlan::from_restriction(&policy.restriction_at(11 * 60)).is_empty());
    }

    #[test]
    fn unchanged_policy_is_not_reinstalled_and_restore_removes_all_filters() {
        let mut reconciler = Reconciler::new(Backend::default());
        let browsers = plan("browsers");
        reconciler.reconcile(browsers.clone()).unwrap();
        reconciler.reconcile(browsers.clone()).unwrap();
        reconciler.reconcile(FilterPlan::default()).unwrap();
        assert_eq!(
            reconciler.backend.plans,
            vec![browsers, FilterPlan::default()]
        );
    }

    #[test]
    fn failed_policy_replacement_keeps_previous_state_and_is_retried() {
        let mut reconciler = Reconciler::new(Backend::default());
        let browsers = plan("browsers");
        reconciler.reconcile(browsers.clone()).unwrap();
        reconciler.backend.fail = true;
        assert!(reconciler.reconcile(FilterPlan::default()).is_err());
        assert_eq!(reconciler.applied, Some(browsers));
        reconciler.backend.fail = false;
        reconciler.reconcile(FilterPlan::default()).unwrap();
        assert!(reconciler.applied.unwrap().is_empty());
    }

    #[test]
    fn restart_outside_a_schedule_clears_persisted_filters() {
        let mut reconciler = Reconciler::new(Backend::default());
        reconciler.reconcile(FilterPlan::default()).unwrap();
        assert_eq!(reconciler.backend.plans, vec![FilterPlan::default()]);
    }
}
