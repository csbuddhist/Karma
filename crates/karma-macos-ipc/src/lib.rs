//! The local IPC transport between KarmaService and its macOS clients: one
//! length-prefixed request and response per connection over a Unix domain
//! socket, with the kernel-recorded identity of the other end.

use std::path::PathBuf;

use thiserror::Error;

#[cfg(not(target_os = "macos"))]
use karma_ipc::{RequestEnvelope, ResponseEnvelope};

/// Where the LaunchDaemon listens. The directory is created by root with mode
/// 0755, so no other user can replace the socket.
pub const SERVICE_SOCKET_DIRECTORY: &str = "/var/run/karma";
pub const SERVICE_SOCKET_PATH: &str = "/var/run/karma/service.sock";
/// Debug builds only: connect to a development service at this path instead.
/// Release builds ignore it, so nobody can point an installed console at a
/// fake service that collects the administrator password.
pub const SERVICE_SOCKET_ENV: &str = "KARMA_SERVICE_SOCKET";

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
    #[error("the endpoint is not owned by the Karma service")]
    UntrustedEndpoint,
    #[error("this transport is only available on macOS")]
    UnsupportedPlatform,
}

/// One service endpoint and whether its listener must run as root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    path: PathBuf,
    require_root_server: bool,
}

impl Endpoint {
    /// The installed service. Debug builds honor [`SERVICE_SOCKET_ENV`] and
    /// then accept a development service that is not running as root.
    pub fn service() -> Self {
        #[cfg(debug_assertions)]
        if let Some(path) = std::env::var_os(SERVICE_SOCKET_ENV).filter(|path| !path.is_empty()) {
            return Self::at(path);
        }
        Self {
            path: PathBuf::from(SERVICE_SOCKET_PATH),
            require_root_server: true,
        }
    }

    /// A service at `path` run by any user, for tests and development.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            require_root_server: false,
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn requires_root_server(&self) -> bool {
        self.require_root_server
    }
}

#[cfg(target_os = "macos")]
mod socket_transport;

#[cfg(target_os = "macos")]
pub use socket_transport::{AuditToken, Connection, PeerCredentials, SocketServer};

/// Sends one request to the installed service and waits for its response.
#[cfg(target_os = "macos")]
pub fn send_request(
    request: &karma_ipc::RequestEnvelope,
    timeout_ms: u32,
) -> Result<karma_ipc::ResponseEnvelope, TransportError> {
    Endpoint::service().send(request, timeout_ms)
}

#[cfg(not(target_os = "macos"))]
pub fn send_request(
    _request: &RequestEnvelope,
    _timeout_ms: u32,
) -> Result<ResponseEnvelope, TransportError> {
    Err(TransportError::UnsupportedPlatform)
}
