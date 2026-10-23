use crate::{
    BgraFrame, FrameError, FrameMetadata, FramePreparer, FrameScheduler, FrameWork, PreparedFrame,
};

/// Receives each prepared frame a platform capture worker admits, together
/// with the inference work scheduled for it.
pub trait PreparedFrameConsumer: Send + 'static {
    fn begin_frame(&mut self) {}

    fn consume(&mut self, frame: PreparedFrame, work: FrameWork);
}

pub struct ScheduledFrame {
    pub frame: PreparedFrame,
    pub work: FrameWork,
}

#[derive(Default)]
pub struct FramePipeline {
    preparer: FramePreparer,
    scheduler: FrameScheduler,
}

impl FramePipeline {
    pub fn process(&mut self, input: BgraFrame) -> Result<ScheduledFrame, FrameError> {
        let frame = self.preparer.prepare(input)?;
        let work = self.scheduler.select(FrameMetadata {
            monitor_id: frame.monitor_id().clone(),
            captured_at_ms: frame.captured_at_ms(),
            fingerprint: frame.fingerprint(),
        });
        Ok(ScheduledFrame { frame, work })
    }
}
