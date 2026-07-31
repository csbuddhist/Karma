use thiserror::Error;

#[cfg(not(windows))]
use karma_ipc::{RequestEnvelope, ResponseEnvelope};

pub const SERVICE_PIPE_NAME: &str = r"\\.\pipe\Karma.Service.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TransportError {
    #[error("the Karma service is unavailable")]
    ServiceUnavailable,
    #[error("the IPC frame is invalid")]
    InvalidFrame,
    #[error("the IPC operation failed")]
    OperationFailed,
    #[error("the IPC operation timed out")]
    TimedOut,
    #[error("this transport is only available on Windows")]
    UnsupportedPlatform,
}

#[cfg(windows)]
mod windows_transport;

#[cfg(windows)]
pub use windows_transport::{PipeServer, send_request};

#[cfg(not(windows))]
pub fn send_request(
    _request: &RequestEnvelope,
    _timeout_ms: u32,
) -> Result<ResponseEnvelope, TransportError> {
    Err(TransportError::UnsupportedPlatform)
}
