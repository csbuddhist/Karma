#![forbid(unsafe_code)]

use std::time::Duration;

use karma_ipc::{DispositionOutcome, DispositionReport, ProcessIdentity};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessOperationError;

pub trait ProcessInspector {
    fn inspect(&self, process_id: u32) -> Result<Option<ProcessIdentity>, ProcessOperationError>;
}

pub trait ProcessController {
    fn request_close(&self, process_id: u32) -> Result<(), ProcessOperationError>;
    fn terminate(&self, process_id: u32) -> Result<(), ProcessOperationError>;
}

pub trait GraceWaiter {
    fn wait(&self, duration: Duration);
}

pub struct DispositionExecutor<I, C, W> {
    inspector: I,
    controller: C,
    waiter: W,
}

impl<I, C, W> DispositionExecutor<I, C, W>
where
    I: ProcessInspector,
    C: ProcessController,
    W: GraceWaiter,
{
    pub fn new(inspector: I, controller: C, waiter: W) -> Self {
        Self {
            inspector,
            controller,
            waiter,
        }
    }

    pub fn execute(
        &self,
        event_id: String,
        expected: &ProcessIdentity,
        grace_period: Duration,
    ) -> DispositionReport {
        let outcome = match self.inspector.inspect(expected.process_id) {
            Err(_) => DispositionOutcome::AccessDenied,
            Ok(None) => DispositionOutcome::SourceUncertain,
            Ok(Some(actual)) if !same_identity(expected, &actual) => {
                DispositionOutcome::IdentityChanged
            }
            Ok(Some(_)) => self.close_then_terminate(expected, grace_period),
        };
        DispositionReport {
            event_id,
            process_id: expected.process_id,
            started_at_ms: expected.started_at_ms,
            outcome,
        }
    }

    fn close_then_terminate(
        &self,
        expected: &ProcessIdentity,
        grace_period: Duration,
    ) -> DispositionOutcome {
        let _ = self.controller.request_close(expected.process_id);
        self.waiter.wait(grace_period);
        match self.inspector.inspect(expected.process_id) {
            Ok(None) => DispositionOutcome::ClosedGracefully,
            Ok(Some(actual)) if !same_identity(expected, &actual) => {
                DispositionOutcome::IdentityChanged
            }
            Err(_) => DispositionOutcome::AccessDenied,
            Ok(Some(_)) => match self.controller.terminate(expected.process_id) {
                Ok(()) => DispositionOutcome::Terminated,
                Err(_) => DispositionOutcome::AccessDenied,
            },
        }
    }
}

fn same_identity(expected: &ProcessIdentity, actual: &ProcessIdentity) -> bool {
    expected.process_id == actual.process_id
        && expected.started_at_ms == actual.started_at_ms
        && expected
            .executable_name
            .eq_ignore_ascii_case(&actual.executable_name)
        && match (&expected.executable_sha256, &actual.executable_sha256) {
            (Some(expected), Some(actual)) => expected.eq_ignore_ascii_case(actual),
            (Some(_), None) => false,
            (None, _) => true,
        }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, time::Duration};

    use karma_ipc::{DispositionOutcome, ProcessIdentity};

    use super::{
        DispositionExecutor, GraceWaiter, ProcessController, ProcessInspector,
        ProcessOperationError,
    };

    fn identity(started_at_ms: i64) -> ProcessIdentity {
        ProcessIdentity {
            process_id: 42,
            started_at_ms,
            executable_name: r"C:\Program Files\Browser\browser.exe".into(),
            executable_sha256: None,
        }
    }

    struct Inspector {
        calls: Cell<u8>,
        first: Option<ProcessIdentity>,
        second: Option<ProcessIdentity>,
    }

    impl ProcessInspector for Inspector {
        fn inspect(
            &self,
            _process_id: u32,
        ) -> Result<Option<ProcessIdentity>, ProcessOperationError> {
            let call = self.calls.get();
            self.calls.set(call + 1);
            Ok(if call == 0 {
                self.first.clone()
            } else {
                self.second.clone()
            })
        }
    }

    #[derive(Default)]
    struct Controller {
        closed: Cell<bool>,
        terminated: Cell<bool>,
    }

    impl ProcessController for Controller {
        fn request_close(&self, _process_id: u32) -> Result<(), ProcessOperationError> {
            self.closed.set(true);
            Ok(())
        }

        fn terminate(&self, _process_id: u32) -> Result<(), ProcessOperationError> {
            self.terminated.set(true);
            Ok(())
        }
    }

    struct Waiter;
    impl GraceWaiter for Waiter {
        fn wait(&self, _duration: Duration) {}
    }

    #[test]
    fn pid_reuse_is_never_terminated() {
        let executor = DispositionExecutor::new(
            Inspector {
                calls: Cell::new(0),
                first: Some(identity(2)),
                second: None,
            },
            Controller::default(),
            Waiter,
        );
        let report = executor.execute("event-1".into(), &identity(1), Duration::ZERO);
        assert_eq!(report.outcome, DispositionOutcome::IdentityChanged);
        assert!(!executor.controller.terminated.get());
    }

    #[test]
    fn graceful_exit_is_preferred_and_matching_survivor_is_terminated() {
        let graceful = DispositionExecutor::new(
            Inspector {
                calls: Cell::new(0),
                first: Some(identity(1)),
                second: None,
            },
            Controller::default(),
            Waiter,
        );
        assert_eq!(
            graceful
                .execute("e1".into(), &identity(1), Duration::ZERO)
                .outcome,
            DispositionOutcome::ClosedGracefully
        );
        assert!(!graceful.controller.terminated.get());

        let forced = DispositionExecutor::new(
            Inspector {
                calls: Cell::new(0),
                first: Some(identity(1)),
                second: Some(identity(1)),
            },
            Controller::default(),
            Waiter,
        );
        assert_eq!(
            forced
                .execute("e2".into(), &identity(1), Duration::ZERO)
                .outcome,
            DispositionOutcome::Terminated
        );
        assert!(forced.controller.terminated.get());
    }
}
