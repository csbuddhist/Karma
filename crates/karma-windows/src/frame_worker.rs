#[cfg(any(windows, test))]
use crate::CaptureSessionStatus;

#[cfg(any(windows, test))]
use std::time::Duration;

#[cfg(any(windows, test))]
const ACTIVE_FRAME_PROCESSING_INTERVAL: Duration = Duration::from_millis(250);
#[cfg(any(windows, test))]
const IDLE_FRAME_PROCESSING_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(any(windows, test))]
const ACTIVE_FRAME_PROCESSING_WINDOW: Duration = Duration::from_secs(2);

#[derive(Debug, Default)]
#[cfg(any(windows, test))]
struct FrameProcessingGate {
    last_admitted_at: Option<Duration>,
    last_fingerprint: Option<u64>,
    active_until: Option<Duration>,
}

#[cfg(any(windows, test))]
impl FrameProcessingGate {
    fn admit(&mut self, now: Duration) -> bool {
        let interval = if self.active_until.is_none_or(|until| now <= until) {
            ACTIVE_FRAME_PROCESSING_INTERVAL
        } else {
            IDLE_FRAME_PROCESSING_INTERVAL
        };
        if self
            .last_admitted_at
            .is_some_and(|previous| now.saturating_sub(previous) < interval)
        {
            return false;
        }
        self.last_admitted_at = Some(now);
        true
    }

    fn observe(&mut self, now: Duration, fingerprint: u64) {
        if self.last_fingerprint != Some(fingerprint) {
            self.active_until = Some(now.saturating_add(ACTIVE_FRAME_PROCESSING_WINDOW));
            self.last_fingerprint = Some(fingerprint);
        }
    }
}

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
    gpu_fallbacks: u64,
    processing_failures: u64,
}

impl Default for FrameWorkerReport {
    fn default() -> Self {
        Self {
            status: FrameWorkerStatus::Starting,
            processed_frames: 0,
            gpu_fallbacks: 0,
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

    pub fn gpu_fallbacks(self) -> u64 {
        self.gpu_fallbacks
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
        time::Instant,
    };

    use karma_ai::{FrameMetadata, FrameScheduler, FrameWork, PreparedFrame};
    use thiserror::Error;

    use super::{
        FrameProcessingGate, FrameWorkerReport, FrameWorkerStatus, WorkerAction, next_action,
    };
    use crate::{WgcCaptureSession, WindowsFrameProcessor, WindowsRuntimeApartment};

    pub trait PreparedFrameConsumer: Send + 'static {
        fn begin_frame(&mut self) {}

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
        let Ok(_runtime) = WindowsRuntimeApartment::initialize_mta() else {
            set_status(&report, FrameWorkerStatus::Failed);
            let _ = session.stop();
            return;
        };
        session.set_frame_notifier(notifier);
        set_status(&report, FrameWorkerStatus::Running);
        let mut scheduler = FrameScheduler::default();
        let mut processing_gate = FrameProcessingGate::default();
        let worker_started = Instant::now();

        loop {
            let frame = session.take_latest_frame();
            match next_action(
                stop_requested.load(Ordering::Acquire),
                session.status(),
                frame.is_some(),
            ) {
                WorkerAction::Process => {
                    let frame = frame.expect("worker action requires a frame");
                    let admitted_at = worker_started.elapsed();
                    if !processing_gate.admit(admitted_at) {
                        continue;
                    }
                    consumer.begin_frame();
                    match processor.process(&frame) {
                        Ok(prepared) => {
                            processing_gate.observe(admitted_at, prepared.fingerprint());
                            let work = scheduler.select(FrameMetadata {
                                monitor_id: prepared.monitor_id().clone(),
                                captured_at_ms: prepared.captured_at_ms(),
                                fingerprint: prepared.fingerprint(),
                            });
                            consumer.consume(prepared, work);
                            let mut current = lock_report(&report);
                            current.processed_frames = current.processed_frames.saturating_add(1);
                            current.gpu_fallbacks = processor.health().gpu_fallbacks();
                        }
                        Err(_) => {
                            let mut current = lock_report(&report);
                            current.processing_failures =
                                current.processing_failures.saturating_add(1);
                            current.gpu_fallbacks = processor.health().gpu_fallbacks();
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
    use std::time::Duration;

    use crate::CaptureSessionStatus;

    use super::{FrameProcessingGate, FrameWorkerStatus, WorkerAction, next_action};

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
    fn sixty_hz_capture_admits_at_most_four_expensive_frames_per_second() {
        let mut gate = FrameProcessingGate::default();
        let admitted = (0..60)
            .filter(|frame| gate.admit(Duration::from_millis(frame * 1_000 / 60)))
            .count();

        assert!(admitted <= 4, "admitted {admitted} expensive frames");
    }

    #[test]
    fn unchanged_capture_settles_to_one_expensive_frame_per_second() {
        let mut gate = FrameProcessingGate::default();
        let mut settled_frames = 0;

        for frame in 0..240 {
            let now = Duration::from_millis(frame * 1_000 / 60);
            if gate.admit(now) {
                gate.observe(now, 7);
                if now >= Duration::from_secs(3) {
                    settled_frames += 1;
                }
            }
        }

        assert_eq!(settled_frames, 1);
    }

    #[test]
    fn changed_capture_temporarily_returns_to_four_fps() {
        let mut gate = FrameProcessingGate::default();
        let mut frames_after_change = 0;

        for frame in 0..300 {
            let now = Duration::from_millis(frame * 1_000 / 60);
            if gate.admit(now) {
                let fingerprint = if now < Duration::from_secs(4) { 7 } else { 8 };
                gate.observe(now, fingerprint);
                if now >= Duration::from_secs(4) {
                    frames_after_change += 1;
                }
            }
        }

        assert_eq!(frames_after_change, 4);
    }

    #[test]
    fn worker_report_contains_only_health_counters() {
        let report = super::FrameWorkerReport::default();
        assert_eq!(report.processed_frames(), 0);
        assert_eq!(report.gpu_fallbacks(), 0);
        assert_eq!(report.processing_failures(), 0);
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
