//! The service's private files under `/Library/Application Support/Karma`.
//!
//! The directory belongs to the service's user with mode 0700 and every file
//! with mode 0600, the counterpart of the SYSTEM/Administrators-only ACL on
//! Windows. The evidence key is a root-only file; FileVault protects it at
//! rest until it is wrapped by the system keychain.

use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use rand::{RngCore, rngs::OsRng};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

const STATE_FILE: &str = "service-state.db";
const AGENT_SECRET_FILE: &str = "agent.secret";
const EVIDENCE_KEY_FILE: &str = "evidence.key";
const EVIDENCE_DIRECTORY: &str = "evidence";
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_MODE: u32 = 0o600;
const AGENT_SECRET_CHARS: usize = 64;
const EVIDENCE_KEY_BYTES: usize = 32;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("service data storage is unavailable")]
    Unavailable,
    #[error("service data storage belongs to another user")]
    WrongOwner,
}

pub struct ServiceFiles {
    pub state: PathBuf,
    pub evidence_directory: PathBuf,
    pub agent_secret: Zeroizing<String>,
    pub evidence_key: Zeroizing<[u8; EVIDENCE_KEY_BYTES]>,
}

/// Creates or opens the private data directory and its secrets.
pub fn open(directory: &Path) -> Result<ServiceFiles, StorageError> {
    private_directory(directory)?;
    let evidence_directory = directory.join(EVIDENCE_DIRECTORY);
    private_directory(&evidence_directory)?;
    let agent_secret = load_or_create_agent_secret(&directory.join(AGENT_SECRET_FILE))?;
    let evidence_key = load_or_create_evidence_key(&directory.join(EVIDENCE_KEY_FILE))?;
    let state = directory.join(STATE_FILE);
    if state.exists() {
        restrict(&state, PRIVATE_FILE_MODE)?;
    }
    Ok(ServiceFiles {
        state,
        evidence_directory,
        agent_secret,
        evidence_key,
    })
}

/// Restricts the database and its WAL side files once SQLite created them.
pub fn restrict_state_files(state: &Path) -> Result<(), StorageError> {
    for suffix in ["", "-wal", "-shm"] {
        let mut path = state.as_os_str().to_owned();
        path.push(suffix);
        let path = PathBuf::from(path);
        if path.exists() {
            restrict(&path, PRIVATE_FILE_MODE)?;
        }
    }
    Ok(())
}

fn private_directory(path: &Path) -> Result<(), StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(StorageError::Unavailable);
        }
        Ok(_) => {}
        Err(_) => fs::create_dir_all(path).map_err(|_| StorageError::Unavailable)?,
    }
    restrict(path, PRIVATE_DIRECTORY_MODE)
}

fn restrict(path: &Path, mode: u32) -> Result<(), StorageError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| StorageError::Unavailable)?;
    if metadata.file_type().is_symlink() {
        return Err(StorageError::Unavailable);
    }
    // SAFETY: geteuid has no preconditions.
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(StorageError::WrongOwner);
    }
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|_| StorageError::Unavailable)
}

fn create_private_file(path: &Path, contents: &[u8]) -> Result<(), StorageError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(PRIVATE_FILE_MODE)
        .open(path)
        .map_err(|_| StorageError::Unavailable)?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|_| StorageError::Unavailable)
}

fn read_private_file(
    path: &Path,
    maximum_bytes: usize,
) -> Result<Zeroizing<Vec<u8>>, StorageError> {
    restrict(path, PRIVATE_FILE_MODE)?;
    let mut contents = Zeroizing::new(Vec::new());
    fs::File::open(path)
        .and_then(|file| {
            file.take(maximum_bytes as u64 + 1)
                .read_to_end(&mut contents)
        })
        .map_err(|_| StorageError::Unavailable)?;
    Ok(contents)
}

fn load_or_create_agent_secret(path: &Path) -> Result<Zeroizing<String>, StorageError> {
    if path.exists() {
        let contents = read_private_file(path, AGENT_SECRET_CHARS)?;
        if contents.len() != AGENT_SECRET_CHARS || !contents.iter().all(u8::is_ascii_hexdigit) {
            return Err(StorageError::Unavailable);
        }
        let secret = std::str::from_utf8(&contents).map_err(|_| StorageError::Unavailable)?;
        return Ok(Zeroizing::new(secret.to_owned()));
    }
    let mut bytes = Zeroizing::new([0_u8; AGENT_SECRET_CHARS / 2]);
    OsRng.fill_bytes(bytes.as_mut());
    let mut secret = Zeroizing::new(String::with_capacity(AGENT_SECRET_CHARS));
    for byte in bytes.iter() {
        use std::fmt::Write as _;
        write!(&mut *secret, "{byte:02x}").expect("writing to a String cannot fail");
    }
    create_private_file(path, secret.as_bytes())?;
    Ok(secret)
}

fn load_or_create_evidence_key(
    path: &Path,
) -> Result<Zeroizing<[u8; EVIDENCE_KEY_BYTES]>, StorageError> {
    let mut key = Zeroizing::new([0_u8; EVIDENCE_KEY_BYTES]);
    if path.exists() {
        let mut contents = read_private_file(path, EVIDENCE_KEY_BYTES)?;
        if contents.len() != EVIDENCE_KEY_BYTES {
            contents.zeroize();
            return Err(StorageError::Unavailable);
        }
        key.copy_from_slice(&contents);
        return Ok(key);
    }
    OsRng.fill_bytes(key.as_mut());
    create_private_file(path, key.as_ref())?;
    Ok(key)
}

/// Whether `path` and every directory above it belong to root and are not
/// writable by group or others, so no other user can replace the file.
pub fn is_root_protected(path: &Path) -> bool {
    path.ancestors().all(|component| {
        fs::symlink_metadata(component).is_ok_and(|metadata| {
            metadata.uid() == 0
                && metadata.mode() & 0o022 == 0
                && !metadata.file_type().is_symlink()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn secrets_are_created_privately_and_reloaded_unchanged() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("Karma");
        let first = open(&directory).unwrap();

        assert_eq!(mode(&directory), PRIVATE_DIRECTORY_MODE);
        assert_eq!(mode(&first.evidence_directory), PRIVATE_DIRECTORY_MODE);
        assert_eq!(mode(&directory.join(AGENT_SECRET_FILE)), PRIVATE_FILE_MODE);
        assert_eq!(mode(&directory.join(EVIDENCE_KEY_FILE)), PRIVATE_FILE_MODE);
        assert_eq!(first.agent_secret.len(), AGENT_SECRET_CHARS);

        fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
        let second = open(&directory).unwrap();
        assert_eq!(mode(&directory), PRIVATE_DIRECTORY_MODE);
        assert_eq!(*first.agent_secret, *second.agent_secret);
        assert_eq!(*first.evidence_key, *second.evidence_key);
    }

    #[test]
    fn damaged_secrets_are_refused_instead_of_replaced() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("Karma");
        open(&directory).unwrap();
        fs::write(directory.join(EVIDENCE_KEY_FILE), [0_u8; 31]).unwrap();
        assert!(matches!(open(&directory), Err(StorageError::Unavailable)));

        let other = root.path().join("Other");
        open(&other).unwrap();
        fs::write(other.join(AGENT_SECRET_FILE), "not-hex").unwrap();
        assert!(matches!(open(&other), Err(StorageError::Unavailable)));
    }

    #[test]
    fn a_symlinked_data_directory_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let link = root.path().join("Karma");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(matches!(open(&link), Err(StorageError::Unavailable)));
    }

    #[test]
    fn only_root_owned_unwritable_paths_are_protected() {
        assert!(is_root_protected(Path::new("/usr/bin/true")));
        let root = tempfile::tempdir().unwrap();
        assert!(!is_root_protected(root.path()));
    }
}
