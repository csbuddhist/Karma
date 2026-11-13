//! Settings files: the policy document sealed with a key derived from the
//! administrator password, so they can be exported and imported without the
//! password while nobody can read or change them.
//!
//! Layout: `KARMASET` | version (1 byte) | Argon2id salt (16) | nonce (12) |
//! AES-256-GCM ciphertext. The header is authenticated as associated data.
//! The core keeps the key of the current password (derived when the password
//! is set or entered), never the password itself.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::Argon2;
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::types::SettingsError;

const MAGIC: &[u8; 8] = b"KARMASET";
const VERSION: u8 = 1;
pub const SALT_BYTES: usize = 16;
const NONCE_BYTES: usize = 12;
const HEADER_BYTES: usize = MAGIC.len() + 1 + SALT_BYTES;
const KEY_BYTES: usize = 32;
const PAYLOAD_FORMAT: &str = "karma-android-settings";
/// The policy limit plus room for the envelope.
pub const MAX_FILE_BYTES: usize = crate::MAX_POLICY_BYTES + 4 * 1024;

/// The settings-file key of the current administrator password.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportKey {
    salt: Vec<u8>,
    key: Vec<u8>,
}

impl std::fmt::Debug for ExportKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ExportKey(..)")
    }
}

impl ExportKey {
    /// Derives the key of `password` with a new random salt.
    pub fn derive(password: &str) -> Result<Self, SettingsError> {
        let mut salt = [0u8; SALT_BYTES];
        OsRng.fill_bytes(&mut salt);
        let key = derive_key(password, &salt)?;
        Ok(Self {
            salt: salt.to_vec(),
            key: key.to_vec(),
        })
    }

    fn is_valid(&self) -> bool {
        self.salt.len() == SALT_BYTES && self.key.len() == KEY_BYTES
    }
}

impl Drop for ExportKey {
    fn drop(&mut self) {
        use zeroize::Zeroize as _;
        self.key.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsPayload {
    format: String,
    version: u32,
    exported_at_ms: i64,
    policy: Value,
}

/// Seals `policy` with `export_key`.
pub fn seal(export_key: &ExportKey, policy: &Value, now_ms: i64) -> Result<Vec<u8>, SettingsError> {
    if !export_key.is_valid() {
        return Err(SettingsError::KeyNotReady);
    }
    let payload = SettingsPayload {
        format: PAYLOAD_FORMAT.into(),
        version: u32::from(VERSION),
        exported_at_ms: now_ms,
        policy: policy.clone(),
    };
    let plaintext =
        Zeroizing::new(serde_json::to_vec(&payload).map_err(|_| SettingsError::Storage)?);
    let mut header = Vec::with_capacity(HEADER_BYTES);
    header.extend_from_slice(MAGIC);
    header.push(VERSION);
    header.extend_from_slice(&export_key.salt);
    let mut nonce = [0u8; NONCE_BYTES];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher(&export_key.key)
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: &header,
            },
        )
        .map_err(|_| SettingsError::Storage)?;
    let mut file = header;
    file.extend_from_slice(&nonce);
    file.extend_from_slice(&ciphertext);
    Ok(file)
}

/// A parsed but still sealed settings file.
pub struct SealedFile<'a> {
    header: &'a [u8],
    salt: &'a [u8],
    nonce: &'a [u8],
    ciphertext: &'a [u8],
}

impl<'a> SealedFile<'a> {
    pub fn parse(file: &'a [u8]) -> Result<Self, SettingsError> {
        if file.len() > MAX_FILE_BYTES
            || file.len() < HEADER_BYTES + NONCE_BYTES + 16
            || &file[..MAGIC.len()] != MAGIC
            || file[MAGIC.len()] != VERSION
        {
            return Err(SettingsError::InvalidFile);
        }
        let (header, rest) = file.split_at(HEADER_BYTES);
        let (nonce, ciphertext) = rest.split_at(NONCE_BYTES);
        Ok(Self {
            header,
            salt: &header[MAGIC.len() + 1..],
            nonce,
            ciphertext,
        })
    }

    /// Whether the file was sealed with `export_key` (same password and salt).
    pub fn uses(&self, export_key: &ExportKey) -> bool {
        export_key.is_valid() && export_key.salt == self.salt
    }

    /// Opens the file with the stored key of the current password.
    pub fn open_with_key(&self, export_key: &ExportKey) -> Result<Value, SettingsError> {
        if !self.uses(export_key) {
            return Err(SettingsError::NeedsAdministrator);
        }
        self.open(&export_key.key)
    }

    /// Opens the file with the password it was sealed with.
    pub fn open_with_password(&self, password: &str) -> Result<Value, SettingsError> {
        let key = derive_key(password, self.salt)?;
        self.open(key.as_ref())
    }

    fn open(&self, key: &[u8]) -> Result<Value, SettingsError> {
        let plaintext = Zeroizing::new(
            cipher(key)
                .decrypt(
                    Nonce::from_slice(self.nonce),
                    Payload {
                        msg: self.ciphertext,
                        aad: self.header,
                    },
                )
                .map_err(|_| SettingsError::Rejected)?,
        );
        let payload: SettingsPayload =
            serde_json::from_slice(&plaintext).map_err(|_| SettingsError::InvalidFile)?;
        if payload.format != PAYLOAD_FORMAT
            || payload.version != u32::from(VERSION)
            || !payload.policy.is_object()
        {
            return Err(SettingsError::InvalidFile);
        }
        Ok(payload.policy)
    }
}

fn derive_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; KEY_BYTES]>, SettingsError> {
    let mut key = Zeroizing::new([0u8; KEY_BYTES]);
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| SettingsError::Storage)?;
    Ok(key)
}

fn cipher(key: &[u8]) -> Aes256Gcm {
    Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PASSWORD: &str = "correct horse battery";

    #[test]
    fn sealed_settings_round_trip_with_the_key_or_the_password() {
        let key = ExportKey::derive(PASSWORD).unwrap();
        let policy = json!({ "protectionEnabled": true, "websites": [] });
        let file = seal(&key, &policy, 7).unwrap();
        assert!(!file.windows(17).any(|w| w == b"protectionEnabled"));

        let sealed = SealedFile::parse(&file).unwrap();
        assert_eq!(sealed.open_with_key(&key).unwrap(), policy);
        assert_eq!(sealed.open_with_password(PASSWORD).unwrap(), policy);
        assert_eq!(
            sealed.open_with_password("another password"),
            Err(SettingsError::Rejected)
        );
    }

    #[test]
    fn any_changed_byte_is_rejected() {
        let key = ExportKey::derive(PASSWORD).unwrap();
        let file = seal(&key, &json!({ "protectionEnabled": true }), 7).unwrap();
        for index in [HEADER_BYTES - 1, HEADER_BYTES + 1, file.len() - 1] {
            let mut tampered = file.clone();
            tampered[index] ^= 1;
            let result = SealedFile::parse(&tampered).and_then(|sealed| {
                sealed
                    .open_with_password(PASSWORD)
                    .and_then(|_| sealed.open_with_key(&key))
            });
            assert!(result.is_err(), "byte {index} was not authenticated");
        }
    }

    #[test]
    fn a_file_from_another_password_needs_the_administrator() {
        let old = ExportKey::derive(PASSWORD).unwrap();
        let current = ExportKey::derive("a newer admin password").unwrap();
        let file = seal(&old, &json!({}), 7).unwrap();
        let sealed = SealedFile::parse(&file).unwrap();
        assert_eq!(
            sealed.open_with_key(&current),
            Err(SettingsError::NeedsAdministrator)
        );
    }

    #[test]
    fn foreign_and_truncated_files_are_invalid() {
        assert!(matches!(
            SealedFile::parse(b"{\"protectionEnabled\":false}"),
            Err(SettingsError::InvalidFile)
        ));
        let key = ExportKey::derive(PASSWORD).unwrap();
        let file = seal(&key, &json!({}), 7).unwrap();
        assert!(matches!(
            SealedFile::parse(&file[..HEADER_BYTES + 4]),
            Err(SettingsError::InvalidFile)
        ));
    }
}
