#![deny(unsafe_op_in_unsafe_fn)]

mod attribution;
mod capture_state;
#[cfg(windows)]
mod native;

pub use attribution::{
    AttributedWindow, AttributionResult, Rect, SourceAttributor, UnreliableReason, WindowCandidate,
};
pub use capture_state::{CaptureSessionEvent, CaptureSessionState, CaptureSessionStatus};
#[cfg(windows)]
pub use native::{
    ForegroundWindowSnapshot, MonitorHandle, MonitorSnapshot, WgcCaptureTarget,
    WindowsAdapterError, WindowsRuntimeApartment, enumerate_active_monitors, foreground_window,
};
