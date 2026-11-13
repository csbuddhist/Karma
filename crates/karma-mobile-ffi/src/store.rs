//! The persisted core state: one AES-256-GCM sealed JSON document, replaced
//! atomically. The platform supplies the 32-byte key (wrapped by Android
//! Keystore); without it the file reveals nothing and cannot be altered
//! undetected.

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::engine::Cooldown;
use crate::settings_file::ExportKey;
use crate::types::{AuditEventRecord, CoreError};

const STATE_FILE: &str = "karma-core.state";
const FORMAT_VERSION: u32 = 1;
const MAGIC: &[u8; 8] = b"KARMAST1";
const AAD: &[u8] = b"karma-mobile-state-v1";
const NONCE_BYTES: usize = 12;
/// Oldest events are dropped beyond this many.
pub const MAX_AUDIT_EVENTS: usize = 2_000;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthLimit {
    pub failures: u32,
    pub blocked_until_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedState {
    pub format_version: u32,
    pub password_hash: Option<String>,
    pub recovery_code_hashes: Vec<String>,
    pub auth_limit: AuthLimit,
    pub policy: Value,
    pub policy_revision: u64,
    pub cooldowns: BTreeMap<String, Cooldown>,
    pub audit: VecDeque<AuditEventRecord>,
    /// Settings-file key of the current password; states written before
    /// settings files existed have none until the password is entered.
    #[serde(default)]
    pub export_key: Option<ExportKey>,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            password_hash: None,
            recovery_code_hashes: Vec::new(),
            auth_limit: AuthLimit::default(),
            policy: Value::Object(Default::default()),
            policy_revision: 0,
            cooldowns: BTreeMap::new(),
            audit: VecDeque::new(),
            export_key: None,
        }
    }
}

impl PersistedState {
    pub fn push_audit(&mut self, event: AuditEventRecord) {
        while self.audit.len() >= MAX_AUDIT_EVENTS {
            self.audit.pop_front();
        }
        self.audit.push_back(event);
    }
}

pub struct SealedStore {
    path: PathBuf,
    key: Zeroizing<[u8; 32]>,
}

impl SealedStore {
    pub fn new(data_dir: &Path, key: &[u8]) -> Result<Self, CoreError> {
        let key: [u8; 32] = key.try_into().map_err(|_| CoreError::InvalidKey)?;
        fs::create_dir_all(data_dir).map_err(|_| CoreError::Storage)?;
        Ok(Self {
            path: data_dir.join(STATE_FILE),
            key: Zeroizing::new(key),
        })
    }

    /// The stored state, or a fresh one when nothing was stored yet.
    pub fn load(&self) -> Result<PersistedState, CoreError> {
        let sealed = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PersistedState::default());
            }
            Err(_) => return Err(CoreError::Storage),
        };
        if sealed.len() < MAGIC.len() + NONCE_BYTES || &sealed[..MAGIC.len()] != MAGIC {
            return Err(CoreError::Storage);
        }
        let (nonce, ciphertext) = sealed[MAGIC.len()..].split_at(NONCE_BYTES);
        let plaintext = Zeroizing::new(
            self.cipher()
                .decrypt(
                    Nonce::from_slice(nonce),
                    Payload {
                        msg: ciphertext,
                        aad: AAD,
                    },
                )
                .map_err(|_| CoreError::InvalidKey)?,
        );
        let state: PersistedState =
            serde_json::from_slice(&plaintext).map_err(|_| CoreError::Storage)?;
        if state.format_version != FORMAT_VERSION {
            return Err(CoreError::Storage);
        }
        Ok(state)
    }

    pub fn save(&self, state: &PersistedState) -> Result<(), CoreError> {
        let plaintext = Zeroizing::new(serde_json::to_vec(state).map_err(|_| CoreError::Storage)?);
        let mut nonce = [0u8; NONCE_BYTES];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = self
            .cipher()
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &plaintext,
                    aad: AAD,
                },
            )
            .map_err(|_| CoreError::Storage)?;
        let temporary = self.path.with_extension("tmp");
        let mut file = fs::File::create(&temporary).map_err(|_| CoreError::Storage)?;
        file.write_all(MAGIC)
            .and_then(|()| file.write_all(&nonce))
            .and_then(|()| file.write_all(&ciphertext))
            .and_then(|()| file.sync_all())
            .map_err(|_| CoreError::Storage)?;
        drop(file);
        fs::rename(&temporary, &self.path).map_err(|_| CoreError::Storage)
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(self.key.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips_and_is_not_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let store = SealedStore::new(dir.path(), &[7u8; 32]).unwrap();
        let mut state = PersistedState {
            password_hash: Some("secret-hash-marker".into()),
            ..PersistedState::default()
        };
        state.policy_revision = 3;
        store.save(&state).unwrap();

        assert_eq!(store.load().unwrap(), state);
        let raw = fs::read(dir.path().join(STATE_FILE)).unwrap();
        assert!(!raw.windows(18).any(|w| w == b"secret-hash-marker"));
    }

    #[test]
    fn states_from_before_settings_files_load_without_an_export_key() {
        let mut legacy = serde_json::to_value(PersistedState::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("export_key");
        let state: PersistedState = serde_json::from_value(legacy).unwrap();
        assert!(state.export_key.is_none());
    }

    #[test]
    fn wrong_key_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        SealedStore::new(dir.path(), &[7u8; 32])
            .unwrap()
            .save(&PersistedState::default())
            .unwrap();
        let other = SealedStore::new(dir.path(), &[8u8; 32]).unwrap();
        assert_eq!(other.load(), Err(CoreError::InvalidKey));
    }

    #[test]
    fn short_keys_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            SealedStore::new(dir.path(), &[1u8; 16]),
            Err(CoreError::InvalidKey)
        ));
    }

    #[test]
    fn audit_is_bounded() {
        let mut state = PersistedState::default();
        for index in 0..(MAX_AUDIT_EVENTS + 5) {
            state.push_audit(AuditEventRecord {
                occurred_at_ms: index as i64,
                kind: "k".into(),
                outcome: "o".into(),
                package_name: None,
                action: None,
                reason: None,
            });
        }
        assert_eq!(state.audit.len(), MAX_AUDIT_EVENTS);
        assert_eq!(state.audit.front().unwrap().occurred_at_ms, 5);
    }
}
