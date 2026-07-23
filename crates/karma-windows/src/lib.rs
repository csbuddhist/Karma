#![deny(unsafe_op_in_unsafe_fn)]

mod attribution;
mod capture_state;
#[cfg(windows)]
mod d3d11_device;
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
#[cfg(windows)]
pub use gpu_frame::NativeCaptureTexture;
#[cfg(windows)]
pub use gpu_scaler::{GpuFrameScaler, GpuScalerError};
#[cfg(windows)]
pub use native::{
    ForegroundWindowSnapshot, MonitorHandle, MonitorSnapshot, WgcCaptureTarget,
    WindowsAdapterError, WindowsRuntimeApartment, enumerate_active_monitors, foreground_window,
};
#[cfg(windows)]
pub use staging_reader::StagingTextureReader;
pub use staging_reader::{MappedBgraLayout, MappedFrameError};
#[cfg(windows)]
pub use wgc_session::{CapturedGpuFrame, WgcCaptureSession};
