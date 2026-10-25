//! SQLite persistence for the service state.
//!
//! The service keeps its state in memory and records every mutation as a
//! [`Change`]; [`Store::apply`] commits a batch of changes in one
//! transaction, so a crash or power loss leaves either the previous or the
//! new state on disk, never a torn file.

use std::{fs, path::Path};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use karma_policy::{RememberedTitle, StrikeRecord, TitleReason};
use rand::{RngCore, rngs::OsRng};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::Zeroizing;

const SCHEMA_VERSION: i64 = 1;
/// Audit rows kept; older rows are deleted as new ones arrive.
pub(crate) const MAX_AUDIT_ENTRIES: u64 = 50_000;
const LEGACY_SCHEMA: u32 = 1;
const MAX_LEGACY_BYTES: u64 = 2 * 1024 * 1024;
const LEGACY_MIGRATED_SUFFIX: &str = "json.migrated";
const SEAL_VERSION: u8 = 1;
const NONCE_BYTES: usize = 12;
const SEALING_KEY_CONTEXT: &[u8] = b"karma-service-state-seal-v1";

const SCHEMA_V1: &str = "
CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT;
CREATE TABLE audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at_ms INTEGER NOT NULL,
    kind TEXT NOT NULL,
    outcome TEXT NOT NULL
) STRICT;
CREATE TABLE evidence (
    id TEXT PRIMARY KEY NOT NULL,
    captured_at_ms INTEGER NOT NULL,
    monitor_name TEXT NOT NULL,
    application_name TEXT NOT NULL,
    reason_code TEXT NOT NULL,
    risk_millis INTEGER NOT NULL,
    media_type TEXT NOT NULL
) STRICT;
CREATE TABLE repeat_offenders (
    executable TEXT PRIMARY KEY NOT NULL,
    closed_at_ms TEXT NOT NULL,
    banned_until_ms INTEGER,
    last_process_id INTEGER,
    last_started_at_ms INTEGER
) STRICT;
CREATE TABLE remembered_titles (
    id TEXT PRIMARY KEY NOT NULL,
    remembered_at_ms INTEGER NOT NULL,
    sealed BLOB NOT NULL
) STRICT;
CREATE INDEX remembered_titles_by_age ON remembered_titles (remembered_at_ms);
";

/// Additions after schema 1, applied idempotently without raising
/// `user_version`: an update rolls back only program files, so the previous
/// build must still open this database. Older builds ignore the new table and
/// columns, and their title upserts leave the reason columns untouched.
const TITLE_REASON_COLUMNS: [&str; 3] = [
    "reason_code TEXT",
    "risk_millis INTEGER",
    "threshold_millis INTEGER",
];
/// Structured, title-free context for an audit row, such as the closed
/// executable and why it was closed. Older builds ignore it.
const AUDIT_DETAIL_COLUMN: &str = "detail TEXT";
const MISJUDGED_TITLES_TABLE: &str = "
CREATE TABLE IF NOT EXISTS misjudged_titles (
    id TEXT PRIMARY KEY NOT NULL,
    misjudged_at_ms INTEGER NOT NULL,
    sealed BLOB NOT NULL
) STRICT;
";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("service state database is unavailable")]
pub(crate) struct StoreError;

impl From<rusqlite::Error> for StoreError {
    fn from(_: rusqlite::Error) -> Self {
        Self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuditRecord {
    pub(crate) occurred_at_ms: i64,
    pub(crate) kind: String,
    pub(crate) outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredEvidence {
    pub(crate) id: String,
    pub(crate) captured_at_ms: i64,
    pub(crate) monitor_name: String,
    pub(crate) application_name: String,
    pub(crate) reason_code: String,
    pub(crate) risk_millis: u16,
    pub(crate) media_type: String,
}

/// An administrator-approved early end of the active network schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NetworkRestore {
    pub(crate) granted_at_ms: i64,
    pub(crate) until_ms: i64,
}

/// A requested uninstall; the administrator password may authorize it once
/// `effective_at_ms` has passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UninstallRequest {
    pub(crate) requested_at_ms: i64,
    pub(crate) effective_at_ms: i64,
    #[serde(default)]
    pub(crate) ready_audited: bool,
}

/// Everything the service loads at startup. Audit rows are write-mostly and
/// stay on disk; only their count is loaded.
#[derive(Debug, Clone)]
pub(crate) struct PersistedState {
    pub(crate) password_hash: Option<String>,
    pub(crate) super_password_hash: Option<String>,
    pub(crate) uninstall_request: Option<UninstallRequest>,
    pub(crate) policy_revision: u64,
    pub(crate) policy: Value,
    pub(crate) evidence: Vec<StoredEvidence>,
    pub(crate) repeat_offenders: Vec<StrikeRecord>,
    pub(crate) network_restore: Option<NetworkRestore>,
    pub(crate) remembered_titles: Vec<RememberedTitle>,
    /// Titles an administrator marked as misjudged; never remembered again.
    /// `remembered_at_ms` holds when they were marked.
    pub(crate) misjudged_titles: Vec<RememberedTitle>,
    pub(crate) audit_count: u64,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            password_hash: None,
            super_password_hash: None,
            uninstall_request: None,
            policy_revision: 0,
            policy: json!({}),
            evidence: Vec::new(),
            repeat_offenders: Vec::new(),
            network_restore: None,
            remembered_titles: Vec::new(),
            misjudged_titles: Vec::new(),
            audit_count: 0,
        }
    }
}

/// One mutation of the persisted state.
#[derive(Debug, Clone)]
pub(crate) enum Change {
    PasswordHash(String),
    SuperPasswordHash(String),
    UninstallRequest(Option<UninstallRequest>),
    Policy {
        revision: u64,
        policy: Value,
    },
    Audit(AuditRecord),
    InsertEvidence(StoredEvidence),
    DeleteEvidence(String),
    /// Replaces the whole (capped) repeat-offender table.
    RepeatOffenders(Vec<StrikeRecord>),
    NetworkRestore(Option<NetworkRestore>),
    UpsertTitle(RememberedTitle),
    DeleteTitle(String),
    InsertMisjudgedTitle(RememberedTitle),
    DeleteMisjudgedTitle(String),
}

/// The JSON state file written by builds before the SQLite store.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyState {
    schema_version: u32,
    password_hash: Option<String>,
    policy_revision: u64,
    policy: Value,
    audit: Vec<AuditRecord>,
    evidence: Vec<StoredEvidence>,
    #[serde(default)]
    repeat_offenders: Vec<StrikeRecord>,
    #[serde(default)]
    network_restore: Option<NetworkRestore>,
    #[serde(default)]
    remembered_titles: Vec<RememberedTitle>,
}

/// The sealed part of a remembered title: browsing history is encrypted at
/// rest and only the identifier and timestamp stay in plaintext.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TitlePayload {
    executable: String,
    title: String,
    browser_host: Option<String>,
}

pub(crate) struct Store {
    connection: Connection,
    sealing_key: Zeroizing<[u8; 32]>,
}

impl Store {
    /// Opens (creating if needed) the database at `path`. On first open, the
    /// legacy JSON state beside it (same name, `.json` extension) is imported
    /// in the same transaction that marks the database initialized, then
    /// renamed so it is never imported twice.
    pub(crate) fn open(
        path: &Path,
        master_key: &[u8; 32],
    ) -> Result<(Self, PersistedState), StoreError> {
        let connection = Connection::open(path)?;
        let journal_mode: String =
            connection.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
        if !journal_mode.eq_ignore_ascii_case("wal") {
            return Err(StoreError);
        }
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "secure_delete", "ON")?;
        let mut store = Self {
            connection,
            sealing_key: sealing_key(master_key),
        };
        store.migrate()?;
        store.initialize(&path.with_extension("json"))?;
        let state = store.load()?;
        Ok((store, state))
    }

    /// Commits `changes` atomically. On error nothing is written and the
    /// caller may retry the same batch.
    pub(crate) fn apply(&mut self, changes: &[Change]) -> Result<(), StoreError> {
        if changes.is_empty() {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        write_changes(&transaction, &self.sealing_key, changes)?;
        transaction.commit()?;
        Ok(())
    }

    /// The newest `limit` audit rows, newest first, with their row ids.
    pub(crate) fn recent_audit(&self, limit: u32) -> Result<Vec<(i64, AuditRecord)>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, occurred_at_ms, kind, outcome, detail FROM audit
             ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = statement
            .query_map([i64::from(limit)], |row| {
                Ok((
                    row.get(0)?,
                    AuditRecord {
                        occurred_at_ms: row.get(1)?,
                        kind: row.get(2)?,
                        outcome: row.get(3)?,
                        detail: row.get(4)?,
                    },
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn migrate(&mut self) -> Result<(), StoreError> {
        let version: i64 = self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            // Written by a newer build; refuse rather than misread it.
            return Err(StoreError);
        }
        if version < 1 {
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(SCHEMA_V1)?;
            transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            transaction.commit()?;
        }
        let transaction = self.connection.transaction()?;
        let columns = transaction
            .prepare("SELECT name FROM pragma_table_info('remembered_titles')")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for column in TITLE_REASON_COLUMNS {
            let name = column.split_whitespace().next().unwrap_or(column);
            if !columns.iter().any(|existing| existing == name) {
                transaction.execute_batch(&format!(
                    "ALTER TABLE remembered_titles ADD COLUMN {column};"
                ))?;
            }
        }
        let audit_columns = transaction
            .prepare("SELECT name FROM pragma_table_info('audit')")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if !audit_columns.iter().any(|existing| existing == "detail") {
            transaction.execute_batch(&format!(
                "ALTER TABLE audit ADD COLUMN {AUDIT_DETAIL_COLUMN};"
            ))?;
        }
        transaction.execute_batch(MISJUDGED_TITLES_TABLE)?;
        transaction.commit()?;
        Ok(())
    }

    fn initialize(&mut self, legacy_path: &Path) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        if read_setting(&transaction, "initialized")?.is_some() {
            return Ok(());
        }
        let legacy = if legacy_path.exists() {
            Some(load_legacy(legacy_path)?)
        } else {
            None
        };
        if let Some(legacy) = &legacy {
            write_changes(&transaction, &self.sealing_key, &legacy_changes(legacy))?;
        }
        write_setting(&transaction, "initialized", "1")?;
        transaction.commit()?;
        if legacy.is_some() {
            // The database is authoritative from here on; a failed rename
            // only leaves a stale backup behind.
            let _ = fs::rename(
                legacy_path,
                legacy_path.with_extension(LEGACY_MIGRATED_SUFFIX),
            );
        }
        Ok(())
    }

    fn load(&mut self) -> Result<PersistedState, StoreError> {
        let connection = &self.connection;
        let policy_revision = read_setting(connection, "policy_revision")?
            .map(|value| value.parse().map_err(|_| StoreError))
            .transpose()?
            .unwrap_or(0);
        let policy = read_setting(connection, "policy")?
            .map(|value| serde_json::from_str(&value).map_err(|_| StoreError))
            .transpose()?
            .unwrap_or_else(|| json!({}));
        let network_restore = read_setting(connection, "network_restore")?
            .map(|value| serde_json::from_str(&value).map_err(|_| StoreError))
            .transpose()?;
        let evidence = connection
            .prepare(
                "SELECT id, captured_at_ms, monitor_name, application_name, reason_code,
                        risk_millis, media_type
                 FROM evidence ORDER BY rowid",
            )?
            .query_map([], |row| {
                Ok(StoredEvidence {
                    id: row.get(0)?,
                    captured_at_ms: row.get(1)?,
                    monitor_name: row.get(2)?,
                    application_name: row.get(3)?,
                    reason_code: row.get(4)?,
                    risk_millis: row.get(5)?,
                    media_type: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let repeat_offenders = connection
            .prepare(
                "SELECT executable, closed_at_ms, banned_until_ms, last_process_id,
                        last_started_at_ms
                 FROM repeat_offenders ORDER BY executable",
            )?
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .map(|row| {
                let (executable, closed_at_ms, banned_until_ms, last_process_id, last_started) =
                    row?;
                Ok(StrikeRecord {
                    executable,
                    closed_at_ms: serde_json::from_str(&closed_at_ms).map_err(|_| StoreError)?,
                    banned_until_ms,
                    last_process_id,
                    last_started_at_ms: last_started,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        let sealed_titles = connection
            .prepare(
                "SELECT id, remembered_at_ms, sealed, reason_code, risk_millis, threshold_millis
                 FROM remembered_titles ORDER BY remembered_at_ms, id",
            )?
            .query_map([], |row| {
                let reason = match (row.get(3)?, row.get(4)?, row.get(5)?) {
                    (Some(reason_code), Some(risk_millis), Some(threshold_millis)) => {
                        Some(TitleReason {
                            reason_code,
                            risk_millis,
                            threshold_millis,
                        })
                    }
                    _ => None,
                };
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    reason,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut remembered_titles = Vec::with_capacity(sealed_titles.len());
        let mut unreadable = Vec::new();
        for (id, remembered_at_ms, sealed, reason) in sealed_titles {
            match unseal_title(&self.sealing_key, &id, &sealed) {
                Some(payload) => remembered_titles.push(RememberedTitle {
                    id,
                    executable: payload.executable,
                    title: payload.title,
                    browser_host: payload.browser_host,
                    remembered_at_ms,
                    reason,
                }),
                // A replaced or lost key makes old rows unreadable forever;
                // drop them instead of refusing to start protection.
                None => unreadable.push(Change::DeleteTitle(id)),
            }
        }
        let sealed_misjudged = connection
            .prepare(
                "SELECT id, misjudged_at_ms, sealed FROM misjudged_titles
                 ORDER BY misjudged_at_ms, id",
            )?
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut misjudged_titles = Vec::with_capacity(sealed_misjudged.len());
        for (id, misjudged_at_ms, sealed) in sealed_misjudged {
            match unseal_title(&self.sealing_key, &id, &sealed) {
                Some(payload) => misjudged_titles.push(RememberedTitle {
                    id,
                    executable: payload.executable,
                    title: payload.title,
                    browser_host: payload.browser_host,
                    remembered_at_ms: misjudged_at_ms,
                    reason: None,
                }),
                None => unreadable.push(Change::DeleteMisjudgedTitle(id)),
            }
        }
        let audit_count: i64 =
            connection.query_row("SELECT COUNT(*) FROM audit", [], |row| row.get(0))?;
        let password_hash = read_setting(connection, "password_hash")?;
        let super_password_hash = read_setting(connection, "super_password_hash")?;
        let uninstall_request = read_setting(connection, "uninstall_request")?
            .map(|value| serde_json::from_str(&value).map_err(|_| StoreError))
            .transpose()?;
        if !unreadable.is_empty() {
            self.apply(&unreadable)?;
        }
        Ok(PersistedState {
            password_hash,
            super_password_hash,
            uninstall_request,
            policy_revision,
            policy,
            evidence,
            repeat_offenders,
            network_restore,
            remembered_titles,
            misjudged_titles,
            audit_count: u64::try_from(audit_count).unwrap_or(0),
        })
    }
}

fn sealing_key(master_key: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    let mut hasher = Sha256::new();
    hasher.update(SEALING_KEY_CONTEXT);
    hasher.update(master_key);
    Zeroizing::new(hasher.finalize().into())
}

fn seal_title(key: &[u8; 32], title: &RememberedTitle) -> Result<Vec<u8>, StoreError> {
    let plaintext = Zeroizing::new(
        serde_json::to_vec(&TitlePayload {
            executable: title.executable.clone(),
            title: title.title.clone(),
            browser_host: title.browser_host.clone(),
        })
        .map_err(|_| StoreError)?,
    );
    let mut nonce = [0_u8; NONCE_BYTES];
    OsRng.fill_bytes(&mut nonce);
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| StoreError)?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: title.id.as_bytes(),
            },
        )
        .map_err(|_| StoreError)?;
    let mut sealed = Vec::with_capacity(1 + NONCE_BYTES + ciphertext.len());
    sealed.push(SEAL_VERSION);
    sealed.extend_from_slice(&nonce);
    sealed.extend_from_slice(&ciphertext);
    Ok(sealed)
}

fn unseal_title(key: &[u8; 32], id: &str, sealed: &[u8]) -> Option<TitlePayload> {
    let (&version, rest) = sealed.split_first()?;
    if version != SEAL_VERSION || rest.len() < NONCE_BYTES {
        return None;
    }
    let (nonce, ciphertext) = rest.split_at(NONCE_BYTES);
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: id.as_bytes(),
                },
            )
            .ok()?,
    );
    serde_json::from_slice(&plaintext).ok()
}

fn read_setting(connection: &Connection, key: &str) -> Result<Option<String>, StoreError> {
    Ok(connection
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()?)
}

fn write_setting(transaction: &Transaction<'_>, key: &str, value: &str) -> Result<(), StoreError> {
    transaction.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

fn write_changes(
    transaction: &Transaction<'_>,
    sealing_key: &[u8; 32],
    changes: &[Change],
) -> Result<(), StoreError> {
    let mut audited = false;
    for change in changes {
        match change {
            Change::PasswordHash(hash) => write_setting(transaction, "password_hash", hash)?,
            Change::SuperPasswordHash(hash) => {
                write_setting(transaction, "super_password_hash", hash)?
            }
            Change::UninstallRequest(Some(request)) => write_setting(
                transaction,
                "uninstall_request",
                &serde_json::to_string(request).map_err(|_| StoreError)?,
            )?,
            Change::UninstallRequest(None) => {
                transaction.execute("DELETE FROM settings WHERE key = 'uninstall_request'", [])?;
            }
            Change::Policy { revision, policy } => {
                write_setting(transaction, "policy_revision", &revision.to_string())?;
                write_setting(
                    transaction,
                    "policy",
                    &serde_json::to_string(policy).map_err(|_| StoreError)?,
                )?;
            }
            Change::Audit(record) => {
                transaction.execute(
                    "INSERT INTO audit (occurred_at_ms, kind, outcome, detail)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        record.occurred_at_ms,
                        record.kind,
                        record.outcome,
                        record.detail
                    ],
                )?;
                audited = true;
            }
            Change::InsertEvidence(item) => {
                transaction.execute(
                    "INSERT INTO evidence (id, captured_at_ms, monitor_name, application_name,
                                           reason_code, risk_millis, media_type)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        item.id,
                        item.captured_at_ms,
                        item.monitor_name,
                        item.application_name,
                        item.reason_code,
                        item.risk_millis,
                        item.media_type
                    ],
                )?;
            }
            Change::DeleteEvidence(id) => {
                transaction.execute("DELETE FROM evidence WHERE id = ?1", [id])?;
            }
            Change::RepeatOffenders(records) => {
                transaction.execute("DELETE FROM repeat_offenders", [])?;
                for record in records {
                    transaction.execute(
                        "INSERT INTO repeat_offenders (executable, closed_at_ms, banned_until_ms,
                                                       last_process_id, last_started_at_ms)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            record.executable,
                            serde_json::to_string(&record.closed_at_ms).map_err(|_| StoreError)?,
                            record.banned_until_ms,
                            record.last_process_id,
                            record.last_started_at_ms
                        ],
                    )?;
                }
            }
            Change::NetworkRestore(Some(restore)) => write_setting(
                transaction,
                "network_restore",
                &serde_json::to_string(restore).map_err(|_| StoreError)?,
            )?,
            Change::NetworkRestore(None) => {
                transaction.execute("DELETE FROM settings WHERE key = 'network_restore'", [])?;
            }
            Change::UpsertTitle(title) => {
                let reason = title.reason.as_ref();
                transaction.execute(
                    "INSERT INTO remembered_titles
                         (id, remembered_at_ms, sealed, reason_code, risk_millis, threshold_millis)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT (id) DO UPDATE SET
                         remembered_at_ms = excluded.remembered_at_ms,
                         sealed = excluded.sealed,
                         reason_code = excluded.reason_code,
                         risk_millis = excluded.risk_millis,
                         threshold_millis = excluded.threshold_millis",
                    params![
                        title.id,
                        title.remembered_at_ms,
                        seal_title(sealing_key, title)?,
                        reason.map(|reason| reason.reason_code.as_str()),
                        reason.map(|reason| reason.risk_millis),
                        reason.map(|reason| reason.threshold_millis)
                    ],
                )?;
            }
            Change::DeleteTitle(id) => {
                transaction.execute("DELETE FROM remembered_titles WHERE id = ?1", [id])?;
            }
            Change::InsertMisjudgedTitle(title) => {
                transaction.execute(
                    "INSERT INTO misjudged_titles (id, misjudged_at_ms, sealed)
                     VALUES (?1, ?2, ?3)",
                    params![
                        title.id,
                        title.remembered_at_ms,
                        seal_title(sealing_key, title)?
                    ],
                )?;
            }
            Change::DeleteMisjudgedTitle(id) => {
                transaction.execute("DELETE FROM misjudged_titles WHERE id = ?1", [id])?;
            }
        }
    }
    if audited {
        transaction.execute(
            "DELETE FROM audit WHERE id <= (SELECT MAX(id) FROM audit) - ?1",
            [MAX_AUDIT_ENTRIES as i64],
        )?;
    }
    Ok(())
}

fn load_legacy(path: &Path) -> Result<LegacyState, StoreError> {
    let metadata = fs::metadata(path).map_err(|_| StoreError)?;
    if metadata.len() > MAX_LEGACY_BYTES {
        return Err(StoreError);
    }
    let bytes = fs::read(path).map_err(|_| StoreError)?;
    let state: LegacyState = serde_json::from_slice(&bytes).map_err(|_| StoreError)?;
    if state.schema_version != LEGACY_SCHEMA {
        return Err(StoreError);
    }
    Ok(state)
}

fn legacy_changes(legacy: &LegacyState) -> Vec<Change> {
    let mut changes = Vec::new();
    if let Some(hash) = &legacy.password_hash {
        changes.push(Change::PasswordHash(hash.clone()));
    }
    changes.push(Change::Policy {
        revision: legacy.policy_revision,
        policy: legacy.policy.clone(),
    });
    changes.extend(legacy.audit.iter().cloned().map(Change::Audit));
    changes.extend(legacy.evidence.iter().cloned().map(Change::InsertEvidence));
    changes.push(Change::RepeatOffenders(legacy.repeat_offenders.clone()));
    changes.push(Change::NetworkRestore(legacy.network_restore));
    changes.extend(
        legacy
            .remembered_titles
            .iter()
            .cloned()
            .map(Change::UpsertTitle),
    );
    changes
}

/// Audit rows on disk, oldest first, for tests.
#[cfg(test)]
pub(crate) fn audit_records(path: &Path) -> Vec<AuditRecord> {
    Connection::open(path)
        .unwrap()
        .prepare("SELECT occurred_at_ms, kind, outcome, detail FROM audit ORDER BY id")
        .unwrap()
        .query_map([], |row| {
            Ok(AuditRecord {
                occurred_at_ms: row.get(0)?,
                kind: row.get(1)?,
                outcome: row.get(2)?,
                detail: row.get(3)?,
            })
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7; 32];

    fn title(id: &str, text: &str, at: i64) -> RememberedTitle {
        RememberedTitle {
            id: id.into(),
            executable: r"c:\browser\chrome.exe".into(),
            title: text.into(),
            browser_host: Some("video.example".into()),
            remembered_at_ms: at,
            reason: None,
        }
    }

    #[test]
    fn title_reasons_and_misjudged_titles_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.db");
        let reasoned = RememberedTitle {
            reason: Some(TitleReason {
                reason_code: "image_immediate".into(),
                risk_millis: 910,
                threshold_millis: 820,
            }),
            ..title("title-1", "Hot Video", 7)
        };
        {
            let (mut store, _) = Store::open(&path, &KEY).unwrap();
            store
                .apply(&[
                    Change::UpsertTitle(reasoned.clone()),
                    Change::InsertMisjudgedTitle(title("misjudged-1", "Homework", 8)),
                    Change::InsertMisjudgedTitle(title("misjudged-2", "News", 9)),
                    Change::DeleteMisjudgedTitle("misjudged-2".into()),
                ])
                .unwrap();
        }
        let (_, state) = Store::open(&path, &KEY).unwrap();
        assert_eq!(state.remembered_titles, vec![reasoned]);
        assert_eq!(
            state.misjudged_titles,
            vec![title("misjudged-1", "Homework", 8)]
        );
    }

    #[test]
    fn schema_one_databases_gain_reasons_and_stay_readable_by_older_builds() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.db");
        let sealed = seal_title(&sealing_key(&KEY), &title("title-1", "Old", 3)).unwrap();
        {
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch(SCHEMA_V1).unwrap();
            connection
                .pragma_update(None, "user_version", SCHEMA_VERSION)
                .unwrap();
            connection
                .execute(
                    "INSERT INTO remembered_titles (id, remembered_at_ms, sealed)
                     VALUES ('title-1', 3, ?1)",
                    [&sealed],
                )
                .unwrap();
        }
        let (_, state) = Store::open(&path, &KEY).unwrap();
        assert_eq!(state.remembered_titles, vec![title("title-1", "Old", 3)]);
        // Reopening is idempotent, and the version an older build checks
        // is unchanged, so a rolled-back update still opens this database.
        Store::open(&path, &KEY).unwrap();
        let version: i64 = Connection::open(&path)
            .unwrap()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn changes_round_trip_through_the_database() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.db");
        {
            let (mut store, state) = Store::open(&path, &KEY).unwrap();
            assert_eq!(state.policy, json!({}));
            store
                .apply(&[
                    Change::PasswordHash("$argon2id$hash".into()),
                    Change::SuperPasswordHash("$argon2id$super".into()),
                    Change::UninstallRequest(Some(UninstallRequest {
                        requested_at_ms: 10,
                        effective_at_ms: 20,
                        ready_audited: false,
                    })),
                    Change::Policy {
                        revision: 3,
                        policy: json!({"protectionEnabled": false}),
                    },
                    Change::Audit(AuditRecord {
                        occurred_at_ms: 1,
                        kind: "policy_updated".into(),
                        outcome: "success".into(),
                        detail: None,
                    }),
                    Change::InsertEvidence(StoredEvidence {
                        id: "evidence-1".into(),
                        captured_at_ms: 2,
                        monitor_name: "显示器 1".into(),
                        application_name: "chrome.exe".into(),
                        reason_code: "image_immediate".into(),
                        risk_millis: 990,
                        media_type: "image/jpeg".into(),
                    }),
                    Change::RepeatOffenders(vec![StrikeRecord {
                        executable: r"d:\game.exe".into(),
                        closed_at_ms: vec![5, 6],
                        banned_until_ms: Some(9),
                        last_process_id: Some(10),
                        last_started_at_ms: Some(11),
                    }]),
                    Change::NetworkRestore(Some(NetworkRestore {
                        granted_at_ms: 1,
                        until_ms: 2,
                    })),
                    Change::UpsertTitle(title("title-1", "Hot Video", 7)),
                    Change::UpsertTitle(title("title-2", "Other", 8)),
                    Change::DeleteTitle("title-2".into()),
                ])
                .unwrap();
        }
        let (_, state) = Store::open(&path, &KEY).unwrap();
        assert_eq!(state.password_hash.as_deref(), Some("$argon2id$hash"));
        assert_eq!(
            state.super_password_hash.as_deref(),
            Some("$argon2id$super")
        );
        assert_eq!(
            state.uninstall_request,
            Some(UninstallRequest {
                requested_at_ms: 10,
                effective_at_ms: 20,
                ready_audited: false,
            })
        );
        assert_eq!(state.policy_revision, 3);
        assert_eq!(state.policy, json!({"protectionEnabled": false}));
        assert_eq!(state.evidence.len(), 1);
        assert_eq!(state.repeat_offenders[0].closed_at_ms, vec![5, 6]);
        assert_eq!(state.repeat_offenders[0].banned_until_ms, Some(9));
        assert_eq!(
            state.network_restore,
            Some(NetworkRestore {
                granted_at_ms: 1,
                until_ms: 2
            })
        );
        assert_eq!(
            state.remembered_titles,
            vec![title("title-1", "Hot Video", 7)]
        );
        assert_eq!(state.audit_count, 1);
    }

    #[test]
    fn remembered_titles_are_encrypted_at_rest() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.db");
        let (mut store, _) = Store::open(&path, &KEY).unwrap();
        store
            .apply(&[Change::UpsertTitle(title(
                "title-1",
                "UniquePlaintextTitle",
                1,
            ))])
            .unwrap();
        // Inspect the write-ahead log before the last connection folds it in.
        let mut bytes = fs::read(path.with_extension("db-wal")).unwrap_or_default();
        drop(store);
        bytes.extend(fs::read(&path).unwrap());
        for secret in [
            b"UniquePlaintextTitle".as_slice(),
            b"video.example",
            b"chrome.exe",
        ] {
            assert!(!bytes.windows(secret.len()).any(|window| window == secret));
        }
        // A different key cannot read the rows; they are dropped on open.
        let (_, state) = Store::open(&path, &[8; 32]).unwrap();
        assert!(state.remembered_titles.is_empty());
        let (_, state) = Store::open(&path, &KEY).unwrap();
        assert!(state.remembered_titles.is_empty());
    }

    #[test]
    fn audit_detail_column_is_added_to_existing_databases() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.db");
        {
            // A database written before the detail column existed.
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch(SCHEMA_V1).unwrap();
            connection
                .execute(
                    "INSERT INTO audit (occurred_at_ms, kind, outcome) VALUES (1, 'old', 'x')",
                    [],
                )
                .unwrap();
            connection
                .pragma_update(None, "user_version", SCHEMA_VERSION)
                .unwrap();
        }
        let (mut store, _) = Store::open(&path, &KEY).unwrap();
        store
            .apply(&[Change::Audit(AuditRecord {
                occurred_at_ms: 2,
                kind: "application_disposition".into(),
                outcome: "terminated".into(),
                detail: Some(r#"{"reason":"title_keyword"}"#.into()),
            })])
            .unwrap();

        let recent = store.recent_audit(10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].1.kind, "application_disposition");
        assert_eq!(
            recent[0].1.detail.as_deref(),
            Some(r#"{"reason":"title_keyword"}"#)
        );
        assert_eq!(recent[1].1.detail, None);
        assert_eq!(store.recent_audit(1).unwrap().len(), 1);
        drop(store);
        // Reopening does not try to add the column twice.
        Store::open(&path, &KEY).unwrap();
    }

    #[test]
    fn audit_log_is_capped_to_the_newest_rows() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.db");
        let (mut store, _) = Store::open(&path, &KEY).unwrap();
        let batch: Vec<_> = (0..MAX_AUDIT_ENTRIES as i64 + 3)
            .map(|index| {
                Change::Audit(AuditRecord {
                    occurred_at_ms: index,
                    kind: "event".into(),
                    outcome: "success".into(),
                    detail: None,
                })
            })
            .collect();
        store.apply(&batch).unwrap();
        let records = audit_records(&path);
        assert_eq!(records.len() as u64, MAX_AUDIT_ENTRIES);
        assert_eq!(records[0].occurred_at_ms, 3);
    }

    #[test]
    fn legacy_json_state_is_imported_once_and_kept_as_a_backup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.db");
        let legacy = directory.path().join("state.json");
        fs::write(
            &legacy,
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "password_hash": "$argon2id$legacy",
                "policy_revision": 4,
                "policy": {"protectionEnabled": true},
                "audit": [{"occurred_at_ms": 1, "kind": "policy_updated", "outcome": "success"}],
                "evidence": [],
                "repeat_offenders": [{
                    "executable": "d:\\game.exe",
                    "closed_at_ms": [],
                    "banned_until_ms": 99,
                    "last_process_id": 1,
                    "last_started_at_ms": 2
                }],
                "network_restore": null,
                "remembered_titles": [{
                    "id": "title-1",
                    "executable": "c:\\browser\\chrome.exe",
                    "title": "Hot Video",
                    "browser_host": "video.example",
                    "remembered_at_ms": 7
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        let (_, state) = Store::open(&path, &KEY).unwrap();
        assert_eq!(state.password_hash.as_deref(), Some("$argon2id$legacy"));
        assert_eq!(state.policy_revision, 4);
        assert_eq!(state.repeat_offenders[0].banned_until_ms, Some(99));
        assert_eq!(
            state.remembered_titles,
            vec![title("title-1", "Hot Video", 7)]
        );
        assert_eq!(state.audit_count, 1);
        assert!(!legacy.exists());
        assert!(directory.path().join("state.json.migrated").exists());

        // A restored legacy file is ignored once the database is initialized.
        fs::write(&legacy, b"{\"schema_version\": 1}").unwrap();
        let (_, state) = Store::open(&path, &KEY).unwrap();
        assert_eq!(state.policy_revision, 4);
    }

    #[test]
    fn unreadable_legacy_state_refuses_to_start_instead_of_resetting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.db");
        fs::write(directory.path().join("state.json"), b"not json").unwrap();
        assert!(Store::open(&path, &KEY).is_err());
        // Nothing was committed, so a repaired file is still imported.
        fs::write(
            directory.path().join("state.json"),
            br#"{"schema_version":1,"password_hash":"$argon2id$x","policy_revision":1,
                "policy":{},"audit":[],"evidence":[]}"#,
        )
        .unwrap();
        let (_, state) = Store::open(&path, &KEY).unwrap();
        assert_eq!(state.password_hash.as_deref(), Some("$argon2id$x"));
    }

    #[test]
    fn databases_from_newer_builds_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.db");
        drop(Store::open(&path, &KEY).unwrap());
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        assert!(Store::open(&path, &KEY).is_err());
    }
}
