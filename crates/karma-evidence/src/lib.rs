#![forbid(unsafe_code)]

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use rand::{RngCore, rngs::OsRng};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

const FILE_VERSION: u8 = 1;
const NONCE_BYTES: usize = 12;
const HEADER_BYTES: usize = 1 + NONCE_BYTES;
const MAX_PLAINTEXT_BYTES: usize = 768 * 1024;
const MAX_CIPHERTEXT_BYTES: usize = MAX_PLAINTEXT_BYTES + HEADER_BYTES + 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum EvidenceError {
    #[error("evidence identifier is invalid")]
    InvalidIdentifier,
    #[error("evidence is too large")]
    TooLarge,
    #[error("evidence already exists")]
    AlreadyExists,
    #[error("evidence is unavailable")]
    Unavailable,
    #[error("evidence authentication failed")]
    AuthenticationFailed,
}

pub struct EvidenceVault {
    directory: PathBuf,
    key: Zeroizing<[u8; 32]>,
}

impl EvidenceVault {
    pub fn new(directory: impl Into<PathBuf>, key: [u8; 32]) -> Self {
        Self {
            directory: directory.into(),
            key: Zeroizing::new(key),
        }
    }

    pub fn store(&self, evidence_id: &str, plaintext: &mut Vec<u8>) -> Result<(), EvidenceError> {
        validate_id(evidence_id)?;
        if plaintext.len() > MAX_PLAINTEXT_BYTES {
            plaintext.zeroize();
            return Err(EvidenceError::TooLarge);
        }
        fs::create_dir_all(&self.directory).map_err(|_| EvidenceError::Unavailable)?;
        let mut nonce_bytes = [0_u8; NONCE_BYTES];
        OsRng.fill_bytes(&mut nonce_bytes);
        let cipher =
            Aes256Gcm::new_from_slice(self.key.as_ref()).map_err(|_| EvidenceError::Unavailable)?;
        let encrypted = cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: plaintext,
                    aad: evidence_id.as_bytes(),
                },
            )
            .map_err(|_| EvidenceError::Unavailable);
        plaintext.zeroize();
        let mut encrypted = encrypted?;
        let path = self.path(evidence_id);
        let write_result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        EvidenceError::AlreadyExists
                    } else {
                        EvidenceError::Unavailable
                    }
                })?;
            file.write_all(&[FILE_VERSION])
                .and_then(|()| file.write_all(&nonce_bytes))
                .and_then(|()| file.write_all(&encrypted))
                .and_then(|()| file.sync_all())
                .map_err(|_| EvidenceError::Unavailable)
        })();
        encrypted.zeroize();
        write_result
    }

    pub fn reveal(&self, evidence_id: &str) -> Result<Zeroizing<Vec<u8>>, EvidenceError> {
        validate_id(evidence_id)?;
        let path = self.path(evidence_id);
        let metadata = fs::metadata(&path).map_err(|_| EvidenceError::Unavailable)?;
        if metadata.len() < HEADER_BYTES as u64 || metadata.len() > MAX_CIPHERTEXT_BYTES as u64 {
            return Err(EvidenceError::Unavailable);
        }
        let mut bytes = fs::read(path).map_err(|_| EvidenceError::Unavailable)?;
        if bytes[0] != FILE_VERSION {
            bytes.zeroize();
            return Err(EvidenceError::Unavailable);
        }
        let mut nonce_bytes = [0_u8; NONCE_BYTES];
        nonce_bytes.copy_from_slice(&bytes[1..HEADER_BYTES]);
        let cipher =
            Aes256Gcm::new_from_slice(self.key.as_ref()).map_err(|_| EvidenceError::Unavailable)?;
        let decrypted = cipher
            .decrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: &bytes[HEADER_BYTES..],
                    aad: evidence_id.as_bytes(),
                },
            )
            .map_err(|_| EvidenceError::AuthenticationFailed);
        bytes.zeroize();
        decrypted.map(Zeroizing::new)
    }

    pub fn delete(&self, evidence_id: &str) -> Result<(), EvidenceError> {
        validate_id(evidence_id)?;
        fs::remove_file(self.path(evidence_id)).map_err(|_| EvidenceError::Unavailable)
    }

    pub fn exists(&self, evidence_id: &str) -> bool {
        validate_id(evidence_id).is_ok() && self.path(evidence_id).is_file()
    }

    fn path(&self, evidence_id: &str) -> PathBuf {
        self.directory.join(format!("{evidence_id}.kme"))
    }
}

fn validate_id(value: &str) -> Result<(), EvidenceError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(EvidenceError::InvalidIdentifier);
    }
    Ok(())
}

pub fn evidence_path(directory: &Path, evidence_id: &str) -> Result<PathBuf, EvidenceError> {
    validate_id(evidence_id)?;
    Ok(directory.join(format!("{evidence_id}.kme")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ciphertext_contains_no_plaintext_and_round_trips() {
        let directory = tempfile::tempdir().unwrap();
        let vault = EvidenceVault::new(directory.path(), [7; 32]);
        let original = b"private-image-pixels".to_vec();
        let mut plaintext = original.clone();
        vault.store("event-1", &mut plaintext).unwrap();
        assert!(plaintext.iter().all(|byte| *byte == 0));
        let encrypted = fs::read(directory.path().join("event-1.kme")).unwrap();
        assert!(
            !encrypted
                .windows(original.len())
                .any(|window| window == original)
        );
        assert_eq!(&*vault.reveal("event-1").unwrap(), &original);
    }

    #[test]
    fn identifier_traversal_and_tampering_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let vault = EvidenceVault::new(directory.path(), [9; 32]);
        assert_eq!(
            vault.reveal("../secret"),
            Err(EvidenceError::InvalidIdentifier)
        );
        let mut plaintext = vec![1, 2, 3];
        vault.store("event-2", &mut plaintext).unwrap();
        let path = directory.path().join("event-2.kme");
        let mut encrypted = fs::read(&path).unwrap();
        *encrypted.last_mut().unwrap() ^= 1;
        fs::write(path, encrypted).unwrap();
        assert_eq!(
            vault.reveal("event-2"),
            Err(EvidenceError::AuthenticationFailed)
        );
    }
}
