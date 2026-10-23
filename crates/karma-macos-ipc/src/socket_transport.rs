use std::{
    fs,
    io::{self, Read, Write},
    mem::{MaybeUninit, size_of},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{FileTypeExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::Path,
    time::Duration,
};

use karma_ipc::{MAX_FRAME_BYTES, RequestEnvelope, ResponseEnvelope, decode_frame, encode_frame};

use crate::{Endpoint, TransportError};

/// Anyone may connect, as the Windows pipe grants interactive users; every
/// request is still authorized by the service.
const SOCKET_MODE: u32 = 0o666;

/// The kernel audit token of a process instance. It names the process ID
/// together with a version that changes on `exec`, so it cannot be confused
/// with a later process that reuses the ID.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct AuditToken(pub [u32; 8]);

impl AuditToken {
    pub fn euid(&self) -> u32 {
        self.0[1]
    }

    pub fn pid(&self) -> i32 {
        self.0[5] as i32
    }

    pub fn pid_version(&self) -> u32 {
        self.0[7]
    }
}

impl std::fmt::Debug for AuditToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuditToken")
            .field("euid", &self.euid())
            .field("pid", &self.pid())
            .field("pid_version", &self.pid_version())
            .finish()
    }
}

/// The identity the kernel recorded for the other end of a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerCredentials {
    pub uid: u32,
    pub gid: u32,
    pub pid: i32,
    pub audit_token: AuditToken,
}

impl PeerCredentials {
    fn of(stream: &UnixStream) -> Result<Self, TransportError> {
        let descriptor = stream.as_raw_fd();
        let credentials: libc::xucred = socket_option(descriptor, libc::LOCAL_PEERCRED)?;
        if credentials.cr_version != libc::XUCRED_VERSION {
            return Err(TransportError::OperationFailed);
        }
        let pid: libc::pid_t = socket_option(descriptor, libc::LOCAL_PEERPID)?;
        let audit_token = AuditToken(socket_option(descriptor, libc::LOCAL_PEERTOKEN)?);
        if audit_token.pid() != pid || audit_token.euid() != credentials.cr_uid {
            return Err(TransportError::OperationFailed);
        }
        Ok(Self {
            uid: credentials.cr_uid,
            gid: if credentials.cr_ngroups > 0 {
                credentials.cr_groups[0]
            } else {
                u32::MAX
            },
            pid,
            audit_token,
        })
    }
}

/// Reads a fixed-size `SOL_LOCAL` option that the kernel fills completely.
fn socket_option<T: Copy>(descriptor: i32, option: i32) -> Result<T, TransportError> {
    let mut value = MaybeUninit::<T>::zeroed();
    let mut length = size_of::<T>() as libc::socklen_t;
    // SAFETY: `value` is valid for `length` writable bytes, and the descriptor
    // belongs to a socket the caller keeps open for the duration of the call.
    let result = unsafe {
        libc::getsockopt(
            descriptor,
            libc::SOL_LOCAL,
            option,
            value.as_mut_ptr().cast(),
            &mut length,
        )
    };
    if result != 0 || length as usize != size_of::<T>() {
        return Err(TransportError::OperationFailed);
    }
    // SAFETY: the kernel reported writing exactly `size_of::<T>()` bytes into
    // zero-initialized storage, and every `T` used here is plain old data.
    Ok(unsafe { value.assume_init() })
}

pub struct SocketServer {
    listener: UnixListener,
}

impl SocketServer {
    /// Listens at `path`, replacing a socket left by an earlier run. Any other
    /// kind of file at `path` is left alone and binding fails. The caller is
    /// responsible for the parent directory's ownership and mode.
    pub fn bind(path: &Path) -> Result<Self, TransportError> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_socket() => {
                fs::remove_file(path).map_err(|_| TransportError::OperationFailed)?;
            }
            Ok(_) => return Err(TransportError::OperationFailed),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(TransportError::OperationFailed),
        }
        let listener = UnixListener::bind(path).map_err(|_| TransportError::OperationFailed)?;
        fs::set_permissions(path, fs::Permissions::from_mode(SOCKET_MODE))
            .map_err(|_| TransportError::OperationFailed)?;
        Ok(Self { listener })
    }

    /// Waits for the next client and records its kernel identity.
    pub fn accept(&self) -> Result<Connection, TransportError> {
        let (stream, _) = self
            .listener
            .accept()
            .map_err(|_| TransportError::OperationFailed)?;
        let peer = PeerCredentials::of(&stream)?;
        Ok(Connection { stream, peer })
    }
}

/// One accepted client connection carrying a single request.
pub struct Connection {
    stream: UnixStream,
    peer: PeerCredentials,
}

impl Connection {
    pub fn peer(&self) -> &PeerCredentials {
        &self.peer
    }

    /// Bounds how long a stalled client may hold this connection.
    pub fn set_timeout(&self, timeout: Duration) -> Result<(), TransportError> {
        let timeout = Some(timeout.max(Duration::from_millis(1)));
        self.stream
            .set_read_timeout(timeout)
            .and_then(|()| self.stream.set_write_timeout(timeout))
            .map_err(|_| TransportError::OperationFailed)
    }

    pub fn read_request(&mut self) -> Result<RequestEnvelope, TransportError> {
        read_message(&mut self.stream)
    }

    pub fn send_response(mut self, response: &ResponseEnvelope) -> Result<(), TransportError> {
        write_message(&mut self.stream, response)?;
        self.stream
            .flush()
            .map_err(|_| TransportError::OperationFailed)
    }
}

impl Endpoint {
    /// Sends one request and waits up to `timeout_ms` for each I/O step.
    pub fn send(
        &self,
        request: &RequestEnvelope,
        timeout_ms: u32,
    ) -> Result<ResponseEnvelope, TransportError> {
        let mut stream =
            UnixStream::connect(&self.path).map_err(|_| TransportError::ServiceUnavailable)?;
        if self.require_root_server && PeerCredentials::of(&stream)?.uid != 0 {
            return Err(TransportError::UntrustedEndpoint);
        }
        let timeout = Some(Duration::from_millis(u64::from(timeout_ms.max(1))));
        stream
            .set_read_timeout(timeout)
            .and_then(|()| stream.set_write_timeout(timeout))
            .map_err(|_| TransportError::OperationFailed)?;
        write_message(&mut stream, request)?;
        read_message(&mut stream)
    }
}

fn write_message<T: serde::Serialize>(
    stream: &mut UnixStream,
    message: &T,
) -> Result<(), TransportError> {
    let bytes = encode_frame(message).map_err(|_| TransportError::InvalidFrame)?;
    stream.write_all(&bytes).map_err(io_error)
}

fn read_message<T: serde::de::DeserializeOwned>(
    stream: &mut UnixStream,
) -> Result<T, TransportError> {
    let mut prefix = [0_u8; 4];
    stream.read_exact(&mut prefix).map_err(io_error)?;
    let body_len = u32::from_le_bytes(prefix) as usize;
    if body_len > MAX_FRAME_BYTES {
        return Err(TransportError::InvalidFrame);
    }
    let mut framed = Vec::with_capacity(body_len + 4);
    framed.extend_from_slice(&prefix);
    framed.resize(body_len + 4, 0);
    stream.read_exact(&mut framed[4..]).map_err(io_error)?;
    decode_frame(&framed).map_err(|_| TransportError::InvalidFrame)
}

fn io_error(error: io::Error) -> TransportError {
    match error.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => TransportError::TimedOut,
        io::ErrorKind::UnexpectedEof => TransportError::InvalidFrame,
        _ => TransportError::OperationFailed,
    }
}

#[cfg(test)]
mod tests {
    use std::{os::unix::fs::PermissionsExt, thread};

    use karma_ipc::{ClientKind, ServiceRequest, ServiceResult};

    use super::*;

    fn bootstrap_request() -> RequestEnvelope {
        RequestEnvelope::new(
            "request-1",
            "nonce-1",
            ClientKind::Ui,
            ServiceRequest::GetBootstrap,
        )
    }

    #[test]
    fn request_and_response_round_trip_with_the_callers_identity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("service.sock");
        let server = SocketServer::bind(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, SOCKET_MODE);

        let worker = thread::spawn(move || {
            let mut connection = server.accept().unwrap();
            let peer = *connection.peer();
            let request = connection.read_request().unwrap();
            connection
                .send_response(&ResponseEnvelope::success(
                    request.request_id,
                    ServiceResult::Acknowledged,
                ))
                .unwrap();
            peer
        });
        let response = Endpoint::at(&path)
            .send(&bootstrap_request(), 2000)
            .unwrap();
        let peer = worker.join().unwrap();

        assert_eq!(response.request_id, "request-1");
        assert!(matches!(response.result, Ok(ServiceResult::Acknowledged)));
        // SAFETY: getuid and getpid have no preconditions.
        assert_eq!(peer.uid, unsafe { libc::getuid() });
        assert_eq!(peer.pid, unsafe { libc::getpid() });
        assert_eq!(peer.audit_token.pid(), peer.pid);
    }

    #[test]
    fn a_service_that_is_not_root_is_rejected_when_root_is_required() {
        // SAFETY: getuid has no preconditions.
        if unsafe { libc::getuid() } == 0 {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("service.sock");
        let _server = SocketServer::bind(&path).unwrap();
        let endpoint = Endpoint {
            path,
            require_root_server: true,
        };

        assert_eq!(
            endpoint.send(&bootstrap_request(), 500).err(),
            Some(TransportError::UntrustedEndpoint)
        );
    }

    #[test]
    fn binding_replaces_a_stale_socket_but_never_another_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("service.sock");
        drop(SocketServer::bind(&path).unwrap());
        assert!(SocketServer::bind(&path).is_ok());

        let regular = directory.path().join("regular");
        fs::write(&regular, b"keep").unwrap();
        assert_eq!(
            SocketServer::bind(&regular).err(),
            Some(TransportError::OperationFailed)
        );
        assert_eq!(fs::read(&regular).unwrap(), b"keep");
    }

    #[test]
    fn oversized_frames_are_rejected_before_the_body_is_read() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("service.sock");
        let server = SocketServer::bind(&path).unwrap();
        let mut client = UnixStream::connect(&path).unwrap();
        client
            .write_all(&((MAX_FRAME_BYTES as u32) + 1).to_le_bytes())
            .unwrap();
        let mut connection = server.accept().unwrap();

        assert_eq!(
            connection.read_request().err(),
            Some(TransportError::InvalidFrame)
        );
        drop(client);
    }

    #[test]
    fn a_silent_client_times_out_instead_of_holding_the_connection() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("service.sock");
        let server = SocketServer::bind(&path).unwrap();
        let _client = UnixStream::connect(&path).unwrap();
        let mut connection = server.accept().unwrap();
        connection.set_timeout(Duration::from_millis(50)).unwrap();

        assert_eq!(
            connection.read_request().err(),
            Some(TransportError::TimedOut)
        );
    }

    #[test]
    fn a_missing_service_is_reported_as_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            Endpoint::at(directory.path().join("absent.sock"))
                .send(&bootstrap_request(), 100)
                .err(),
            Some(TransportError::ServiceUnavailable)
        );
    }
}
