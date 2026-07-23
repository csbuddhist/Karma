#[cfg(any(windows, test))]
use crate::CaptureSessionStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameWorkerStatus {
    Starting,
    Running,
    RecreateRequired,
    TargetClosed,
    DeviceLost,
    AccessDenied,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameWorkerReport {
    status: FrameWorkerStatus,
    processed_frames: u64,
    processing_failures: u64,
}

impl Default for FrameWorkerReport {
    fn default() -> Self {
        Self {
            status: FrameWorkerStatus::Starting,
            processed_frames: 0,
            processing_failures: 0,
        }
    }
}

impl FrameWorkerReport {
    pub fn status(self) -> FrameWorkerStatus {
        self.status
    }

    pub fn processed_frames(self) -> u64 {
        self.processed_frames
    }

    pub fn processing_failures(self) -> u64 {
        self.processing_failures
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(windows, test))]
enum WorkerAction {
    Process,
    Wait,
    Stop(FrameWorkerStatus),
}

#[cfg(any(windows, test))]
fn terminal_status(status: CaptureSessionStatus) -> Option<FrameWorkerStatus> {
    match status {
        CaptureSessionStatus::Starting | CaptureSessionStatus::Running => None,
        CaptureSessionStatus::RecreateRequired => Some(FrameWorkerStatus::RecreateRequired),
        CaptureSessionStatus::TargetClosed => Some(FrameWorkerStatus::TargetClosed),
        CaptureSessionStatus::DeviceLost => Some(FrameWorkerStatus::DeviceLost),
        CaptureSessionStatus::AccessDenied => Some(FrameWorkerStatus::AccessDenied),
        CaptureSessionStatus::Failed => Some(FrameWorkerStatus::Failed),
        CaptureSessionStatus::Stopped => Some(FrameWorkerStatus::Stopped),
    }
}

#[cfg(any(windows, test))]
fn next_action(
    stop_requested: bool,
    capture_status: CaptureSessionStatus,
    has_frame: bool,
) -> WorkerAction {
    if stop_requested {
        return WorkerAction::Stop(FrameWorkerStatus::Stopped);
    }
    if let Some(status) = terminal_status(capture_status) {
        return WorkerAction::Stop(status);
    }
    if has_frame {
        WorkerAction::Process
    } else {
        WorkerAction::Wait
    }
}

#[cfg(windows)]
mod native {
    use std::{
        sync::{
            Arc, Mutex, MutexGuard,
            atomic::{AtomicBool, Ordering},
            mpsc::{SyncSender, sync_channel},
        },
        thread::{self, JoinHandle},
    };

    use karma_ai::{FrameMetadata, FrameScheduler, FrameWork, PreparedFrame};
    use thiserror::Error;

    use super::{FrameWorkerReport, FrameWorkerStatus, WorkerAction, next_action};
    use crate::{WgcCaptureSession, WindowsFrameProcessor};

    pub trait PreparedFrameConsumer: Send + 'static {
        fn consume(&mut self, frame: PreparedFrame, work: FrameWork);
    }

    #[derive(Default)]
    pub struct NoopFrameConsumer;

    impl PreparedFrameConsumer for NoopFrameConsumer {
        fn consume(&mut self, _frame: PreparedFrame, _work: FrameWork) {}
    }

    #[derive(Debug, Error)]
    pub enum FrameWorkerError {
        #[error("failed to spawn Windows frame worker")]
        Spawn(#[source] std::io::Error),
        #[error("Windows frame worker panicked")]
        Panicked,
    }

    fn lock_report(report: &Mutex<FrameWorkerReport>) -> MutexGuard<'_, FrameWorkerReport> {
        report
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn set_status(report: &Mutex<FrameWorkerReport>, status: FrameWorkerStatus) {
        lock_report(report).status = status;
    }

    fn run_worker(
        mut session: WgcCaptureSession,
        mut processor: WindowsFrameProcessor,
        mut consumer: Box<dyn PreparedFrameConsumer>,
        stop_requested: Arc<AtomicBool>,
        receiver: std::sync::mpsc::Receiver<()>,
        notifier: SyncSender<()>,
        report: Arc<Mutex<FrameWorkerReport>>,
    ) {
        session.set_frame_notifier(notifier);
        set_status(&report, FrameWorkerStatus::Running);
        let mut scheduler = FrameScheduler::default();

        loop {
            let frame = session.take_latest_frame();
            match next_action(
                stop_requested.load(Ordering::Acquire),
                session.status(),
                frame.is_some(),
            ) {
                WorkerAction::Process => {
                    let frame = frame.expect("worker action requires a frame");
                    match processor.process(&frame) {
                        Ok(prepared) => {
                            let work = scheduler.select(FrameMetadata {
                                monitor_id: prepared.monitor_id().clone(),
                                captured_at_ms: prepared.captured_at_ms(),
                                fingerprint: prepared.fingerprint(),
                            });
                            consumer.consume(prepared, work);
                            let mut current = lock_report(&report);
                            current.processed_frames = current.processed_frames.saturating_add(1);
                        }
                        Err(_) => {
                            let mut current = lock_report(&report);
                            current.processing_failures =
                                current.processing_failures.saturating_add(1);
                            current.status = FrameWorkerStatus::Failed;
                            break;
                        }
                    }
                }
                WorkerAction::Wait => {
                    if receiver.recv().is_err() {
                        set_status(&report, FrameWorkerStatus::Stopped);
                        break;
                    }
                }
                WorkerAction::Stop(status) => {
                    set_status(&report, status);
                    break;
                }
            }
        }
        let _ = session.stop();
    }

    pub struct WindowsFrameWorker {
        stop_requested: Arc<AtomicBool>,
        notifier: SyncSender<()>,
        report: Arc<Mutex<FrameWorkerReport>>,
        join: Option<JoinHandle<()>>,
    }

    impl WindowsFrameWorker {
        pub fn start(
            session: WgcCaptureSession,
            processor: WindowsFrameProcessor,
            consumer: impl PreparedFrameConsumer,
        ) -> Result<Self, FrameWorkerError> {
            let (notifier, receiver) = sync_channel(1);
            let stop_requested = Arc::new(AtomicBool::new(false));
            let report = Arc::new(Mutex::new(FrameWorkerReport::default()));
            let worker_stop = Arc::clone(&stop_requested);
            let worker_report = Arc::clone(&report);
            let worker_notifier = notifier.clone();
            let join = thread::Builder::new()
                .name(format!("karma-frame-{}", session.monitor_id().0))
                .spawn(move || {
                    run_worker(
                        session,
                        processor,
                        Box::new(consumer),
                        worker_stop,
                        receiver,
                        worker_notifier,
                        worker_report,
                    );
                })
                .map_err(FrameWorkerError::Spawn)?;
            Ok(Self {
                stop_requested,
                notifier,
                report,
                join: Some(join),
            })
        }

        pub fn report(&self) -> FrameWorkerReport {
            *lock_report(&self.report)
        }

        pub fn stop(&mut self) -> Result<(), FrameWorkerError> {
            self.stop_requested.store(true, Ordering::Release);
            let _ = self.notifier.try_send(());
            if let Some(join) = self.join.take() {
                join.join().map_err(|_| FrameWorkerError::Panicked)?;
            }
            Ok(())
        }
    }

    impl Drop for WindowsFrameWorker {
        fn drop(&mut self) {
            let _ = self.stop();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn worker_handle_can_be_managed_from_the_control_thread() {
            fn require_send<T: Send>() {}
            require_send::<WindowsFrameWorker>();
        }
    }
}

#[cfg(windows)]
pub use native::{FrameWorkerError, NoopFrameConsumer, PreparedFrameConsumer, WindowsFrameWorker};

#[cfg(test)]
mod tests {
    use crate::CaptureSessionStatus;

    use super::{FrameWorkerStatus, WorkerAction, next_action};

    #[test]
    fn queued_frame_is_processed_only_while_capture_is_running() {
        assert_eq!(
            next_action(false, CaptureSessionStatus::Running, true),
            WorkerAction::Process
        );
        assert_eq!(
            next_action(false, CaptureSessionStatus::Running, false),
            WorkerAction::Wait
        );
    }

    #[test]
    fn stop_and_terminal_capture_states_end_the_worker() {
        assert_eq!(
            next_action(true, CaptureSessionStatus::Running, true),
            WorkerAction::Stop(FrameWorkerStatus::Stopped)
        );
        assert_eq!(
            next_action(false, CaptureSessionStatus::RecreateRequired, false),
            WorkerAction::Stop(FrameWorkerStatus::RecreateRequired)
        );
        assert_eq!(
            next_action(false, CaptureSessionStatus::TargetClosed, false),
            WorkerAction::Stop(FrameWorkerStatus::TargetClosed)
        );
        assert_eq!(
            next_action(false, CaptureSessionStatus::Failed, false),
            WorkerAction::Stop(FrameWorkerStatus::Failed)
        );
    }
}
