//! macOS platform integration for Karma: process identity and control,
//! the active graphical session, and (for the session agent) displays,
//! windows, and screen capture.

use thiserror::Error;

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
