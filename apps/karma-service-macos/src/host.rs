//! Serves `ServiceCore` over the local socket.

use std::{
    fs,
    os::unix::{fs::MetadataExt, fs::PermissionsExt, net::UnixStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use karma_ipc::{
    ClientKind, ResponseEnvelope, ServiceErrorCode, ServiceFailure, ServiceRequest, ServiceResult,
};
use karma_macos::{process, session};
use karma_macos_ipc::{Connection, SocketServer};
use karma_service_core::ServiceCore;
use thiserror::Error;
use zeroize::Zeroizing;

use crate::{
    disposition::{self, DispositionScope},
    maintenance,
    peer::{Peer, PeerPolicy},
    storage, watchdog,
};

const MAX_CONCURRENT_CONNECTIONS: usize = 8;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);
const SOCKET_DIRECTORY_MODE: u32 = 0o755;

#[derive(Debug, Error)]
pub enum HostError {
    #[error("service data storage is unavailable")]
    Storage(#[from] storage::StorageError),
    #[error("the service state could not be opened")]
    Core,
    #[error("the service socket could not be created")]
    Socket,
}

/// Where the service keeps its state and whom it trusts.
#[derive(Debug, Clone)]
pub struct HostConfig {
    pub data_directory: PathBuf,
    pub socket_path: PathBuf,
    /// Create the socket's directory as root-owned 0755 (LaunchDaemon only).
    pub own_socket_directory: bool,
    pub peers: PeerPolicy,
    /// The LaunchAgent the watchdog restarts; `None` disables the watchdog.
    pub launch_agent_plist: Option<PathBuf>,
}

struct ConnectionSlot(Arc<AtomicUsize>);

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// State shared by every connection worker.
struct Shared {
    core: Arc<ServiceCore>,
    peers: PeerPolicy,
    agent_token: Zeroizing<String>,
    protected_executables: Vec<PathBuf>,
    last_heartbeat_ms: Arc<AtomicI64>,
    shutdown: Arc<AtomicBool>,
    socket_path: PathBuf,
}

pub fn serve(config: HostConfig) -> Result<(), HostError> {
    let files = storage::open(&config.data_directory)?;
    let core = Arc::new(
        ServiceCore::open(
            &files.state,
            files.agent_secret.to_string(),
            files.evidence_directory.clone(),
            *files.evidence_key,
            unix_time_ms(),
        )
        .map_err(|_| HostError::Core)?,
    );
    storage::restrict_state_files(&files.state)?;
    if config.own_socket_directory {
        if let Some(directory) = config.socket_path.parent() {
            prepare_socket_directory(directory)?;
        }
    }
    let server = SocketServer::bind(&config.socket_path).map_err(|_| HostError::Socket)?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let last_heartbeat_ms = Arc::new(AtomicI64::new(0));
    let protected_executables = config
        .peers
        .agent_executable
        .iter()
        .cloned()
        .chain(std::env::current_exe().ok())
        .collect();
    let maintenance = maintenance::start(Arc::clone(&core), Arc::clone(&shutdown));
    let shared = Arc::new(Shared {
        core,
        peers: config.peers,
        agent_token: files.agent_secret,
        protected_executables,
        last_heartbeat_ms: Arc::clone(&last_heartbeat_ms),
        shutdown: Arc::clone(&shutdown),
        socket_path: config.socket_path.clone(),
    });
    let watchdog = config
        .launch_agent_plist
        .map(|plist| watchdog::start(plist, Arc::clone(&last_heartbeat_ms), Arc::clone(&shutdown)));
    eprintln!("status=running component=service");

    let active_connections = Arc::new(AtomicUsize::new(0));
    let mut consecutive_failures = 0_u32;
    while !shutdown.load(Ordering::Acquire) {
        let connection = match server.accept() {
            Ok(connection) => {
                consecutive_failures = 0;
                connection
            }
            Err(_) => {
                // A client that hung up before its identity could be read;
                // back off briefly in case the listener itself is failing.
                consecutive_failures = consecutive_failures.saturating_add(1);
                thread::sleep(Duration::from_millis(
                    u64::from(consecutive_failures.min(100)) * 10,
                ));
                continue;
            }
        };
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        if active_connections
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_CONCURRENT_CONNECTIONS).then_some(count + 1)
            })
            .is_err()
        {
            continue;
        }
        let slot = ConnectionSlot(Arc::clone(&active_connections));
        let worker_shared = Arc::clone(&shared);
        let spawned = thread::Builder::new()
            .name("karma-socket-connection".into())
            .spawn(move || {
                let _slot = slot;
                if handle_connection(connection, &worker_shared).is_err() {
                    eprintln!("status=degraded component=service_ipc error=connection_failed");
                }
            });
        if spawned.is_err() {
            eprintln!("status=degraded component=service_ipc error=worker_unavailable");
        }
    }
    let _ = maintenance.join();
    if let Some(watchdog) = watchdog {
        let _ = watchdog.join();
    }
    let _ = fs::remove_file(&config.socket_path);
    Ok(())
}

fn handle_connection(
    mut connection: Connection,
    shared: &Shared,
) -> Result<(), karma_macos_ipc::TransportError> {
    connection.set_timeout(CONNECTION_TIMEOUT)?;
    let mut request = connection.read_request()?;
    let credentials = *connection.peer();
    let peer = Peer {
        uid: credentials.uid,
        executable: (request.client == ClientKind::Agent)
            .then(|| process::executable_path_for(&credentials.audit_token))
            .flatten(),
    };
    let console_user = session::console_user();
    if let Err(code) = shared.peers.admit(
        &mut request,
        &peer,
        console_user,
        shared.agent_token.as_str(),
    ) {
        eprintln!(
            "status=denied component=service_ipc client={:?} uid={}",
            request.client, peer.uid
        );
        return connection.send_response(&ResponseEnvelope::failure(
            request.request_id,
            ServiceFailure::new(code),
        ));
    }
    let heartbeat = matches!(request.request, ServiceRequest::AgentHeartbeat { .. });
    let shutdown_requested = matches!(request.request, ServiceRequest::RequestShutdown { .. });
    let mut response = shared.core.handle(request, unix_time_ms());
    if heartbeat && response.result.is_ok() {
        shared
            .last_heartbeat_ms
            .store(unix_time_ms(), Ordering::Release);
    }
    if let Ok(ServiceResult::DispositionRequired {
        event_id,
        target,
        grace_period_ms,
    }) = &response.result
    {
        let report = disposition::execute(
            event_id.clone(),
            target,
            Duration::from_millis(u64::from(*grace_period_ms)),
            DispositionScope {
                console_user,
                protected_executables: shared.protected_executables.clone(),
            },
        );
        response = if shared
            .core
            .record_disposition(&report, unix_time_ms())
            .is_ok()
        {
            ResponseEnvelope::success(
                response.request_id,
                ServiceResult::DispositionCompleted { report },
            )
        } else {
            ResponseEnvelope::failure(
                response.request_id,
                ServiceFailure::new(ServiceErrorCode::Internal),
            )
        };
    }
    if shutdown_requested && response.result.is_ok() {
        // Authorization is terminal in the core; honor it even if the
        // client disconnects before reading the response.
        shared.shutdown.store(true, Ordering::Release);
        let _ = UnixStream::connect(&shared.socket_path);
    }
    connection.send_response(&response)
}

/// Creates the socket directory as root-owned 0755, repairing an existing one
/// so no other account can replace the socket.
fn prepare_socket_directory(directory: &Path) -> Result<(), HostError> {
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(HostError::Socket);
        }
        Ok(metadata) if metadata.uid() != 0 || metadata.gid() != 0 => {
            std::os::unix::fs::chown(directory, Some(0), Some(0)).map_err(|_| HostError::Socket)?;
        }
        Ok(_) => {}
        Err(_) => fs::create_dir_all(directory).map_err(|_| HostError::Socket)?,
    }
    fs::set_permissions(directory, fs::Permissions::from_mode(SOCKET_DIRECTORY_MODE))
        .map_err(|_| HostError::Socket)
}

pub fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
