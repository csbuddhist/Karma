#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureSessionStatus {
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
pub enum CaptureSessionEvent {
    Started,
    FrameArrived,
    SizeChanged,
    TargetClosed,
    DeviceLost,
    AccessDenied,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureSessionState {
    status: CaptureSessionStatus,
}

impl Default for CaptureSessionState {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureSessionState {
    pub fn new() -> Self {
        Self {
            status: CaptureSessionStatus::Starting,
        }
    }

    pub fn status(&self) -> CaptureSessionStatus {
        self.status
    }

    pub fn apply(&mut self, event: CaptureSessionEvent) {
        if self.status == CaptureSessionStatus::Stopped {
            return;
        }
        if event == CaptureSessionEvent::Stopped {
            self.status = CaptureSessionStatus::Stopped;
            return;
        }
        if matches!(
            self.status,
            CaptureSessionStatus::TargetClosed
                | CaptureSessionStatus::DeviceLost
                | CaptureSessionStatus::AccessDenied
                | CaptureSessionStatus::Failed
        ) {
            return;
        }
        if self.status == CaptureSessionStatus::RecreateRequired
            && event == CaptureSessionEvent::FrameArrived
        {
            return;
        }

        self.status = match event {
            CaptureSessionEvent::Started | CaptureSessionEvent::FrameArrived => {
                CaptureSessionStatus::Running
            }
            CaptureSessionEvent::SizeChanged => CaptureSessionStatus::RecreateRequired,
            CaptureSessionEvent::TargetClosed => CaptureSessionStatus::TargetClosed,
            CaptureSessionEvent::DeviceLost => CaptureSessionStatus::DeviceLost,
            CaptureSessionEvent::AccessDenied => CaptureSessionStatus::AccessDenied,
            CaptureSessionEvent::Failed => CaptureSessionStatus::Failed,
            CaptureSessionEvent::Stopped => unreachable!(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_frame_resize_close_and_stop_are_explicit() {
        let mut state = CaptureSessionState::new();
        assert_eq!(state.status(), CaptureSessionStatus::Starting);
        state.apply(CaptureSessionEvent::Started);
        assert_eq!(state.status(), CaptureSessionStatus::Running);
        state.apply(CaptureSessionEvent::SizeChanged);
        assert_eq!(state.status(), CaptureSessionStatus::RecreateRequired);
        state.apply(CaptureSessionEvent::FrameArrived);
        assert_eq!(state.status(), CaptureSessionStatus::RecreateRequired);
        state.apply(CaptureSessionEvent::TargetClosed);
        assert_eq!(state.status(), CaptureSessionStatus::TargetClosed);
        state.apply(CaptureSessionEvent::FrameArrived);
        assert_eq!(state.status(), CaptureSessionStatus::TargetClosed);
        state.apply(CaptureSessionEvent::Stopped);
        assert_eq!(state.status(), CaptureSessionStatus::Stopped);
    }

    #[test]
    fn terminal_stop_is_idempotent_and_ignores_late_callbacks() {
        let mut state = CaptureSessionState::new();
        state.apply(CaptureSessionEvent::Stopped);
        state.apply(CaptureSessionEvent::FrameArrived);
        state.apply(CaptureSessionEvent::Failed);
        assert_eq!(state.status(), CaptureSessionStatus::Stopped);
    }

    #[test]
    fn access_and_device_failures_remain_distinct() {
        let mut access = CaptureSessionState::new();
        access.apply(CaptureSessionEvent::AccessDenied);
        assert_eq!(access.status(), CaptureSessionStatus::AccessDenied);
        let mut device = CaptureSessionState::new();
        device.apply(CaptureSessionEvent::DeviceLost);
        assert_eq!(device.status(), CaptureSessionStatus::DeviceLost);
    }
}
