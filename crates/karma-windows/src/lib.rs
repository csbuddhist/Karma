#![deny(unsafe_op_in_unsafe_fn)]

mod attribution;
#[cfg(windows)]
mod native;

pub use attribution::{
    AttributedWindow, AttributionResult, Rect, SourceAttributor, UnreliableReason, WindowCandidate,
};
#[cfg(windows)]
pub use native::{
    ForegroundWindowSnapshot, MonitorHandle, MonitorSnapshot, WgcCaptureTarget,
    WindowsAdapterError, WindowsRuntimeApartment, enumerate_active_monitors, foreground_window,
};
