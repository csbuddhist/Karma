//! macOS platform integration for Karma: process identity and control,
//! the active graphical session, and (for the session agent) displays,
//! windows, screen capture, and browser address bars.

use thiserror::Error;

#[cfg(all(target_os = "macos", feature = "session-agent"))]
pub mod accessibility;
#[cfg(all(target_os = "macos", feature = "session-agent"))]
pub mod capture;
#[cfg(all(target_os = "macos", feature = "session-agent"))]
pub mod display;
#[cfg(target_os = "macos")]
pub mod process;
#[cfg(target_os = "macos")]
pub mod session;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ProcessError {
    #[error("the process identifier is invalid")]
    InvalidProcess,
    #[error("the process is not running")]
    NotRunning,
    #[error("access to the process was denied")]
    AccessDenied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CaptureError {
    #[error("the active displays could not be listed")]
    DisplaysUnavailable,
    #[error("Screen Recording is not allowed for this process")]
    AccessDenied,
    #[error("the display is no longer available for capture")]
    DisplayNotFound,
    #[error("the capture stream failed")]
    StreamFailed,
    #[error("ScreenCaptureKit did not respond in time")]
    TimedOut,
}
