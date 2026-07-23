use std::sync::{Arc, Mutex, MutexGuard};

use karma_ai::LatestFrameMailbox;
use karma_domain::MonitorId;
use windows::{
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{
            Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
            GraphicsCaptureSession,
        },
        DirectX::{Direct3D11::IDirect3DSurface, DirectXPixelFormat},
        SizeInt32,
    },
    core::IInspectable,
};

use crate::{
    CaptureSessionEvent, CaptureSessionState, CaptureSessionStatus, D3d11CaptureDevice,
    WgcCaptureTarget, WindowsAdapterError,
};

pub struct CapturedGpuFrame {
    monitor_id: MonitorId,
    inner: Option<Direct3D11CaptureFrame>,
}

impl CapturedGpuFrame {
    fn new(monitor_id: MonitorId, inner: Direct3D11CaptureFrame) -> Self {
        Self {
            monitor_id,
            inner: Some(inner),
        }
    }

    fn frame(&self) -> &Direct3D11CaptureFrame {
        self.inner
            .as_ref()
            .expect("captured frame is unavailable after close")
    }

    pub fn content_size(&self) -> Result<(u32, u32), WindowsAdapterError> {
        let size = self.frame().ContentSize().map_err(|source| {
            WindowsAdapterError::api("Direct3D11CaptureFrame.ContentSize", source)
        })?;
        if size.Width <= 0 || size.Height <= 0 {
            return Err(WindowsAdapterError::InvalidCaptureSize);
        }
        Ok((size.Width as u32, size.Height as u32))
    }

    pub fn monitor_id(&self) -> &MonitorId {
        &self.monitor_id
    }

    pub fn surface(&self) -> Result<IDirect3DSurface, WindowsAdapterError> {
        self.frame()
            .Surface()
            .map_err(|source| WindowsAdapterError::api("Direct3D11CaptureFrame.Surface", source))
    }

    pub fn captured_at_ms(&self) -> Result<i64, WindowsAdapterError> {
        let duration = self
            .frame()
            .SystemRelativeTime()
            .map_err(|source| {
                WindowsAdapterError::api("Direct3D11CaptureFrame.SystemRelativeTime", source)
            })?
            .Duration;
        if duration < 0 {
            return Err(WindowsAdapterError::InvalidCaptureTimestamp);
        }
        Ok(duration / 10_000)
    }
}

impl Drop for CapturedGpuFrame {
    fn drop(&mut self) {
        if let Some(frame) = self.inner.take() {
            let _ = frame.Close();
        }
    }
}

fn lock_state(state: &Mutex<CaptureSessionState>) -> MutexGuard<'_, CaptureSessionState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn apply_event(state: &Mutex<CaptureSessionState>, event: CaptureSessionEvent) {
    lock_state(state).apply(event);
}

pub struct WgcCaptureSession {
    monitor_id: MonitorId,
    target: WgcCaptureTarget,
    frame_pool: Direct3D11CaptureFramePool,
    capture_session: GraphicsCaptureSession,
    frame_arrived_token: Option<i64>,
    target_closed_token: Option<i64>,
    mailbox: Arc<LatestFrameMailbox<CapturedGpuFrame>>,
    state: Arc<Mutex<CaptureSessionState>>,
    stopped: bool,
}

impl WgcCaptureSession {
    pub fn start(
        monitor_id: MonitorId,
        target: WgcCaptureTarget,
        device: &D3d11CaptureDevice,
    ) -> Result<Self, WindowsAdapterError> {
        let (width, height) = target.size()?;
        let frame_size = SizeInt32 {
            Width: width as i32,
            Height: height as i32,
        };
        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            device.winrt_device(),
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            frame_size,
        )
        .map_err(|source| {
            WindowsAdapterError::api("Direct3D11CaptureFramePool.CreateFreeThreaded", source)
        })?;

        let mailbox = Arc::new(LatestFrameMailbox::new());
        let state = Arc::new(Mutex::new(CaptureSessionState::new()));
        let expected_size = (width, height);

        let callback_mailbox = Arc::clone(&mailbox);
        let callback_state = Arc::clone(&state);
        let callback_monitor_id = monitor_id.clone();
        let frame_handler =
            TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(move |sender, _| {
                let Some(sender) = sender.as_ref() else {
                    apply_event(&callback_state, CaptureSessionEvent::Failed);
                    return Ok(());
                };
                let frame = match sender.TryGetNextFrame() {
                    Ok(frame) => CapturedGpuFrame::new(callback_monitor_id.clone(), frame),
                    Err(_) => {
                        apply_event(&callback_state, CaptureSessionEvent::Failed);
                        let _ = callback_mailbox.take();
                        return Ok(());
                    }
                };
                match frame.content_size() {
                    Ok(size) if size == expected_size => {
                        let mut state = lock_state(&callback_state);
                        if matches!(
                            state.status(),
                            CaptureSessionStatus::Starting | CaptureSessionStatus::Running
                        ) {
                            callback_mailbox.push(frame);
                            state.apply(CaptureSessionEvent::FrameArrived);
                        }
                    }
                    Ok(_) => {
                        apply_event(&callback_state, CaptureSessionEvent::SizeChanged);
                        let _ = callback_mailbox.take();
                    }
                    Err(_) => {
                        apply_event(&callback_state, CaptureSessionEvent::Failed);
                        let _ = callback_mailbox.take();
                    }
                }
                Ok(())
            });
        let frame_arrived_token = frame_pool.FrameArrived(&frame_handler).map_err(|source| {
            let _ = frame_pool.Close();
            WindowsAdapterError::api("Direct3D11CaptureFramePool.FrameArrived", source)
        })?;

        let closed_mailbox = Arc::clone(&mailbox);
        let closed_state = Arc::clone(&state);
        let closed_handler =
            TypedEventHandler::<GraphicsCaptureItem, IInspectable>::new(move |_, _| {
                apply_event(&closed_state, CaptureSessionEvent::TargetClosed);
                let _ = closed_mailbox.take();
                Ok(())
            });
        let target_closed_token = match target.capture_item().Closed(&closed_handler) {
            Ok(token) => token,
            Err(source) => {
                let _ = frame_pool.RemoveFrameArrived(frame_arrived_token);
                let _ = frame_pool.Close();
                return Err(WindowsAdapterError::api(
                    "GraphicsCaptureItem.Closed",
                    source,
                ));
            }
        };

        let capture_session = match frame_pool.CreateCaptureSession(target.capture_item()) {
            Ok(session) => session,
            Err(source) => {
                let _ = target.capture_item().RemoveClosed(target_closed_token);
                let _ = frame_pool.RemoveFrameArrived(frame_arrived_token);
                let _ = frame_pool.Close();
                return Err(WindowsAdapterError::api(
                    "Direct3D11CaptureFramePool.CreateCaptureSession",
                    source,
                ));
            }
        };
        if let Err(source) = capture_session.StartCapture() {
            let _ = capture_session.Close();
            let _ = target.capture_item().RemoveClosed(target_closed_token);
            let _ = frame_pool.RemoveFrameArrived(frame_arrived_token);
            let _ = frame_pool.Close();
            return Err(WindowsAdapterError::api(
                "GraphicsCaptureSession.StartCapture",
                source,
            ));
        }
        apply_event(&state, CaptureSessionEvent::Started);

        Ok(Self {
            monitor_id,
            target,
            frame_pool,
            capture_session,
            frame_arrived_token: Some(frame_arrived_token),
            target_closed_token: Some(target_closed_token),
            mailbox,
            state,
            stopped: false,
        })
    }

    pub fn monitor_id(&self) -> &MonitorId {
        &self.monitor_id
    }

    pub fn status(&self) -> CaptureSessionStatus {
        lock_state(&self.state).status()
    }

    pub fn take_latest_frame(&self) -> Option<CapturedGpuFrame> {
        self.mailbox.take()
    }

    pub fn stop(&mut self) -> Result<(), WindowsAdapterError> {
        if self.stopped {
            return Ok(());
        }
        self.stopped = true;
        apply_event(&self.state, CaptureSessionEvent::Stopped);
        let _ = self.mailbox.take();

        let mut first_error = None;
        if let Some(token) = self.frame_arrived_token.take() {
            if let Err(source) = self.frame_pool.RemoveFrameArrived(token) {
                first_error.get_or_insert_with(|| {
                    WindowsAdapterError::api(
                        "Direct3D11CaptureFramePool.RemoveFrameArrived",
                        source,
                    )
                });
            }
        }
        if let Some(token) = self.target_closed_token.take() {
            if let Err(source) = self.target.capture_item().RemoveClosed(token) {
                first_error.get_or_insert_with(|| {
                    WindowsAdapterError::api("GraphicsCaptureItem.RemoveClosed", source)
                });
            }
        }
        if let Err(source) = self.capture_session.Close() {
            first_error.get_or_insert_with(|| {
                WindowsAdapterError::api("GraphicsCaptureSession.Close", source)
            });
        }
        if let Err(source) = self.frame_pool.Close() {
            first_error.get_or_insert_with(|| {
                WindowsAdapterError::api("Direct3D11CaptureFramePool.Close", source)
            });
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Drop for WgcCaptureSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::{CapturedGpuFrame, MonitorId, WgcCaptureSession};
    use crate::{D3d11CaptureDevice, WgcCaptureTarget, WindowsAdapterError};

    fn require_send<T: Send>() {}

    #[test]
    fn captured_frames_and_sessions_can_cross_worker_boundaries() {
        require_send::<CapturedGpuFrame>();
        require_send::<WgcCaptureSession>();
    }

    #[test]
    fn start_signature_keeps_monitor_identity_with_the_session() {
        let _start: fn(
            MonitorId,
            WgcCaptureTarget,
            &D3d11CaptureDevice,
        ) -> Result<WgcCaptureSession, WindowsAdapterError> = WgcCaptureSession::start;
    }

    #[test]
    fn captured_frame_exposes_monotonic_relative_milliseconds() {
        let _read_time: fn(&CapturedGpuFrame) -> Result<i64, WindowsAdapterError> =
            CapturedGpuFrame::captured_at_ms;
    }

    #[test]
    fn captured_frame_keeps_monitor_identity() {
        let _monitor: fn(&CapturedGpuFrame) -> &MonitorId = CapturedGpuFrame::monitor_id;
    }
}
