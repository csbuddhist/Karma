//! One thread per display: takes the newest captured frame, prepares and
//! schedules it like the Windows pipeline, and hands it to the inference
//! consumer.

use std::{
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use karma_ai::{FramePipeline, PreparedFrameConsumer};
use karma_ipc::ComponentState;
use karma_macos::capture::{FrameSource, StreamStatus};

const FRAME_WAIT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerReport {
    pub status: StreamStatus,
    pub processed_frames: u64,
    pub processing_failures: u64,
}

impl WorkerReport {
    pub fn frame_state(&self) -> ComponentState {
        match self.status {
            StreamStatus::Starting => ComponentState::Starting,
            StreamStatus::Running => ComponentState::Healthy,
            StreamStatus::Stopped => ComponentState::Stopped,
            StreamStatus::AccessDenied | StreamStatus::Failed => ComponentState::Unavailable,
        }
    }
}

pub struct DisplayWorker {
    stop_requested: Arc<AtomicBool>,
    report: Arc<Mutex<WorkerReport>>,
    join: Option<JoinHandle<()>>,
}

impl DisplayWorker {
    pub fn start(
        source: FrameSource,
        consumer: impl PreparedFrameConsumer,
    ) -> std::io::Result<Self> {
        let stop_requested = Arc::new(AtomicBool::new(false));
        let report = Arc::new(Mutex::new(WorkerReport {
            status: source.status(),
            processed_frames: 0,
            processing_failures: 0,
        }));
        let worker_stop = Arc::clone(&stop_requested);
        let worker_report = Arc::clone(&report);
        let join = thread::Builder::new()
            .name(format!("karma-frame-{}", source.monitor_id().0))
            .spawn(move || run(source, Box::new(consumer), &worker_stop, &worker_report))?;
        Ok(Self {
            stop_requested,
            report,
            join: Some(join),
        })
    }

    pub fn report(&self) -> WorkerReport {
        *lock(&self.report)
    }

    pub fn is_active(&self) -> bool {
        self.report().status.is_active()
    }

    pub fn stop(&mut self) {
        self.stop_requested.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for DisplayWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

fn lock(report: &Mutex<WorkerReport>) -> MutexGuard<'_, WorkerReport> {
    report
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn run(
    source: FrameSource,
    mut consumer: Box<dyn PreparedFrameConsumer>,
    stop_requested: &AtomicBool,
    report: &Mutex<WorkerReport>,
) {
    let mut pipeline = FramePipeline::default();
    while !stop_requested.load(Ordering::Acquire) {
        let status = source.status();
        lock(report).status = status;
        let Some(frame) = source.next_frame(FRAME_WAIT) else {
            if !status.is_active() {
                break;
            }
            continue;
        };
        // Attribute the frame to its window before the slower preparation.
        consumer.begin_frame();
        match pipeline.process(frame) {
            Ok(scheduled) => {
                consumer.consume(scheduled.frame, scheduled.work);
                let mut current = lock(report);
                current.processed_frames = current.processed_frames.saturating_add(1);
            }
            Err(_) => {
                let mut current = lock(report);
                current.processing_failures = current.processing_failures.saturating_add(1);
            }
        }
    }
    let mut current = lock(report);
    if current.status.is_active() {
        current.status = StreamStatus::Stopped;
    }
}
