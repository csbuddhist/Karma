#![deny(unsafe_op_in_unsafe_fn)]

mod attribution;
mod capture_state;
#[cfg(windows)]
mod d3d11_device;
mod frame_processor;
mod frame_worker;
#[cfg(windows)]
mod gpu_frame;
#[cfg(windows)]
mod gpu_scaler;
#[cfg(windows)]
mod native;
mod staging_reader;
#[cfg(windows)]
mod wgc_session;

pub use attribution::{
    AttributedWindow, AttributionResult, Rect, SourceAttributor, UnreliableReason, WindowCandidate,
};
pub use capture_state::{CaptureSessionEvent, CaptureSessionState, CaptureSessionStatus};
#[cfg(windows)]
pub use d3d11_device::{CaptureDriver, D3d11CaptureDevice};
pub use frame_processor::FrameProcessingHealth;
#[cfg(windows)]
pub use frame_processor::{FrameProcessingError, WindowsFrameProcessor};
#[cfg(windows)]
pub use frame_worker::{
    FrameWorkerError, NoopFrameConsumer, PreparedFrameConsumer, WindowsFrameWorker,
};
pub use frame_worker::{FrameWorkerReport, FrameWorkerStatus};
#[cfg(windows)]
pub use gpu_frame::NativeCaptureTexture;
#[cfg(windows)]
pub use gpu_scaler::{GpuFrameScaler, GpuScalerError};
#[cfg(windows)]
pub use native::{
    ForegroundWindowSnapshot, MonitorHandle, MonitorSnapshot, ProcessSnapshot, WgcCaptureTarget,
    WindowsAdapterError, WindowsRuntimeApartment, browser_host, enumerate_active_monitors,
    foreground_window, inspect_process, window_title,
};
#[cfg(windows)]
pub use staging_reader::StagingTextureReader;
pub use staging_reader::{MappedBgraLayout, MappedFrameError};
#[cfg(windows)]
pub use wgc_session::{CapturedGpuFrame, WgcCaptureSession};
