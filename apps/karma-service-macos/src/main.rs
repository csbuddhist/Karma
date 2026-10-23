//! KarmaService for macOS: the root LaunchDaemon that owns policy, audit,
//! evidence, and enforcement, serving the console and session agent over a
//! local socket.
//!
//! `KarmaService` runs as the installed daemon. `KarmaService --console
//! [--data-dir DIR] [--socket PATH] [--agent PATH]` runs a development
//! service as the current user, with its own data directory and socket.

#[cfg(target_os = "macos")]
mod disposition;
#[cfg(target_os = "macos")]
mod host;
#[cfg(target_os = "macos")]
mod maintenance;
#[cfg(target_os = "macos")]
mod peer;
#[cfg(target_os = "macos")]
mod storage;
#[cfg(target_os = "macos")]
mod watchdog;

#[cfg(target_os = "macos")]
mod daemon {
    use std::{
        ffi::OsString,
        path::{Path, PathBuf},
    };

    use crate::{
        host::{self, HostConfig},
        peer::PeerPolicy,
        storage,
    };

    const DATA_DIRECTORY: &str = "/Library/Application Support/Karma";
    const LAUNCH_AGENT_PLIST: &str = "/Library/LaunchAgents/com.karma.agent.plist";
    const DEVELOPMENT_DIRECTORY: &str = "Library/Application Support/Karma Development";
    /// The agent inside its application bundle, next to this executable.
    const AGENT_IN_INSTALL: &str = "KarmaAgent.app/Contents/MacOS/karma-agent-macos";
    /// A development agent built into the same target directory.
    const AGENT_IN_BUILD: &str = "karma-agent-macos";

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let mut arguments = std::env::args_os().skip(1);
        let config = match arguments.next() {
            None => installed_config()?,
            Some(argument) if argument == "--console" => console_config(arguments)?,
            Some(argument) if argument == "version" => {
                println!("{}", karma_ipc::PRODUCT_VERSION);
                return Ok(());
            }
            Some(_) => {
                eprintln!(
                    "usage: KarmaService [version | --console [--data-dir DIR] [--socket PATH] [--agent PATH]]"
                );
                std::process::exit(2);
            }
        };
        host::serve(config)?;
        Ok(())
    }

    fn install_directory() -> Result<PathBuf, std::io::Error> {
        std::env::current_exe()?
            .canonicalize()?
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| std::io::Error::other("the service has no install directory"))
    }

    fn installed_config() -> Result<HostConfig, Box<dyn std::error::Error>> {
        // SAFETY: geteuid has no preconditions.
        if unsafe { libc::geteuid() } != 0 {
            return Err("KarmaService must run as root; use --console for development".into());
        }
        let agent = install_directory()?.join(AGENT_IN_INSTALL);
        let agent = agent.canonicalize().ok().filter(|path| {
            let protected = storage::is_root_protected(path);
            if !protected {
                eprintln!("status=degraded component=agent_identity error=agent_not_protected");
            }
            protected
        });
        Ok(HostConfig {
            data_directory: PathBuf::from(DATA_DIRECTORY),
            socket_path: PathBuf::from(karma_macos_ipc::SERVICE_SOCKET_PATH),
            own_socket_directory: true,
            peers: PeerPolicy {
                agent_executable: agent,
                installer_uid: 0,
            },
            launch_agent_plist: Some(PathBuf::from(LAUNCH_AGENT_PLIST)),
        })
    }

    fn console_config(
        mut arguments: impl Iterator<Item = OsString>,
    ) -> Result<HostConfig, Box<dyn std::error::Error>> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is not set")?;
        let mut data_directory = home.join(DEVELOPMENT_DIRECTORY);
        let mut socket_path = None;
        let mut agent = install_directory()?.join(AGENT_IN_BUILD);
        while let Some(argument) = arguments.next() {
            let value = arguments
                .next()
                .map(PathBuf::from)
                .ok_or("an option is missing its value")?;
            match argument.to_str() {
                Some("--data-dir") => data_directory = value,
                Some("--socket") => socket_path = Some(value),
                Some("--agent") => agent = value,
                _ => return Err("unknown option".into()),
            }
        }
        let socket_path = socket_path.unwrap_or_else(|| data_directory.join("service.sock"));
        eprintln!(
            "status=development component=service socket={}",
            socket_path.display()
        );
        Ok(HostConfig {
            data_directory,
            socket_path,
            own_socket_directory: false,
            peers: PeerPolicy {
                agent_executable: agent.canonicalize().ok(),
                // SAFETY: getuid has no preconditions.
                installer_uid: unsafe { libc::getuid() },
            },
            launch_agent_plist: None,
        })
    }
}

#[cfg(target_os = "macos")]
fn main() {
    if let Err(error) = daemon::run() {
        eprintln!("KarmaService failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("this KarmaService can only run on macOS");
}
