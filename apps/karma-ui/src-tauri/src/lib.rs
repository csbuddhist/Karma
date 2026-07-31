use std::{
    collections::HashMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng as PasswordOsRng},
};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, State};
use thiserror::Error;
use zeroize::Zeroizing;

const CONFIG_FILE: &str = "console.json";
const MAX_CONFIG_BYTES: usize = 1_048_576;
const SESSION_LIFETIME: Duration = Duration::from_secs(15 * 60);
const MAX_FAILURES: u32 = 5;
const FAILURE_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
enum ConsoleError {
    #[error("管理员密码至少需要 10 个字符")]
    PasswordTooShort,
    #[error("管理员密码不正确")]
    InvalidPassword,
    #[error("验证失败次数过多，请稍后重试")]
    RateLimited,
    #[error("管理会话已失效，请重新解锁")]
    SessionExpired,
    #[error("控制台已经完成初始化")]
    AlreadyEnrolled,
    #[error("控制台尚未设置管理员密码")]
    NotEnrolled,
    #[error("设置数据超出大小限制")]
    StateTooLarge,
    #[error("该事件没有可用的加密原图")]
    EvidenceUnavailable,
    #[error("本地安全存储不可用")]
    StorageUnavailable,
}

impl serde::Serialize for ConsoleError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredConsole {
    schema_version: u32,
    password_hash: String,
    state: Value,
}

#[derive(Debug, Default)]
struct AuthRuntime {
    sessions: HashMap<String, Instant>,
    failures: u32,
    blocked_until: Option<Instant>,
}

#[derive(Default)]
struct ConsoleRuntime(Mutex<AuthRuntime>);

fn default_console_state() -> Value {
    json!({
        "protectionEnabled": true,
        "serviceConnected": false,
        "agentConnected": false,
        "monitors": [],
        "recognition": {
            "imageEnabled": true,
            "ocrEnabled": true,
            "sensitivity": 82,
            "immediateThreshold": 95,
            "evidenceEnabled": false,
            "evidenceRetentionDays": 7
        },
        "keywords": [],
        "applications": [
            {
                "id": "browser",
                "name": "浏览器",
                "executable": "受支持浏览器",
                "category": "browser",
                "action": "content_only",
                "enabled": true
            },
            {
                "id": "player",
                "name": "播放器",
                "executable": "受支持播放器",
                "category": "player",
                "action": "content_only",
                "enabled": true
            }
        ],
        "schedules": [],
        "evidence": [],
        "audit": []
    })
}

fn config_path(app: &AppHandle) -> Result<PathBuf, ConsoleError> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join(CONFIG_FILE))
        .map_err(|_| ConsoleError::StorageUnavailable)
}

fn load_stored(path: &Path) -> Result<StoredConsole, ConsoleError> {
    let metadata = fs::metadata(path).map_err(|_| ConsoleError::NotEnrolled)?;
    if metadata.len() > MAX_CONFIG_BYTES as u64 {
        return Err(ConsoleError::StorageUnavailable);
    }
    let bytes = fs::read(path).map_err(|_| ConsoleError::StorageUnavailable)?;
    serde_json::from_slice(&bytes).map_err(|_| ConsoleError::StorageUnavailable)
}

fn save_stored(path: &Path, stored: &StoredConsole) -> Result<(), ConsoleError> {
    let parent = path.parent().ok_or(ConsoleError::StorageUnavailable)?;
    fs::create_dir_all(parent).map_err(|_| ConsoleError::StorageUnavailable)?;
    let encoded = serde_json::to_vec(stored).map_err(|_| ConsoleError::StorageUnavailable)?;
    if encoded.len() > MAX_CONFIG_BYTES {
        return Err(ConsoleError::StateTooLarge);
    }
    fs::write(path, encoded).map_err(|_| ConsoleError::StorageUnavailable)
}

fn new_session(runtime: &ConsoleRuntime) -> Result<String, ConsoleError> {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut token, "{byte:02x}").expect("writing to a String cannot fail");
    }
    let mut auth = runtime
        .0
        .lock()
        .map_err(|_| ConsoleError::StorageUnavailable)?;
    auth.sessions.clear();
    auth.sessions
        .insert(token.clone(), Instant::now() + SESSION_LIFETIME);
    auth.failures = 0;
    auth.blocked_until = None;
    Ok(token)
}

fn authorize(runtime: &ConsoleRuntime, token: &str) -> Result<(), ConsoleError> {
    let now = Instant::now();
    let mut auth = runtime
        .0
        .lock()
        .map_err(|_| ConsoleError::StorageUnavailable)?;
    auth.sessions.retain(|_, expires_at| *expires_at > now);
    let expires_at = auth
        .sessions
        .get_mut(token)
        .ok_or(ConsoleError::SessionExpired)?;
    *expires_at = now + SESSION_LIFETIME;
    Ok(())
}

fn verify_password(
    runtime: &ConsoleRuntime,
    stored: &StoredConsole,
    password: Zeroizing<String>,
) -> Result<(), ConsoleError> {
    let now = Instant::now();
    {
        let auth = runtime
            .0
            .lock()
            .map_err(|_| ConsoleError::StorageUnavailable)?;
        if auth.blocked_until.is_some_and(|until| until > now) {
            return Err(ConsoleError::RateLimited);
        }
    }
    let parsed =
        PasswordHash::new(&stored.password_hash).map_err(|_| ConsoleError::StorageUnavailable)?;
    if Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
    {
        let mut auth = runtime
            .0
            .lock()
            .map_err(|_| ConsoleError::StorageUnavailable)?;
        auth.failures = 0;
        auth.blocked_until = None;
        return Ok(());
    }
    let mut auth = runtime
        .0
        .lock()
        .map_err(|_| ConsoleError::StorageUnavailable)?;
    auth.failures = auth.failures.saturating_add(1);
    if auth.failures >= MAX_FAILURES {
        auth.failures = 0;
        auth.blocked_until = Some(now + FAILURE_COOLDOWN);
    }
    Err(ConsoleError::InvalidPassword)
}

#[tauri::command]
fn auth_status(app: AppHandle) -> Result<&'static str, ConsoleError> {
    let path = config_path(&app)?;
    if path.is_file() {
        load_stored(&path)?;
        Ok("locked")
    } else {
        Ok("setup")
    }
}

#[tauri::command]
fn enroll(
    app: AppHandle,
    runtime: State<'_, ConsoleRuntime>,
    password: String,
) -> Result<String, ConsoleError> {
    let password = Zeroizing::new(password);
    if password.chars().count() < 10 {
        return Err(ConsoleError::PasswordTooShort);
    }
    let path = config_path(&app)?;
    if path.exists() {
        return Err(ConsoleError::AlreadyEnrolled);
    }
    let salt = SaltString::generate(&mut PasswordOsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| ConsoleError::StorageUnavailable)?
        .to_string();
    save_stored(
        &path,
        &StoredConsole {
            schema_version: 1,
            password_hash: hash,
            state: default_console_state(),
        },
    )?;
    new_session(&runtime)
}

#[tauri::command]
fn unlock(
    app: AppHandle,
    runtime: State<'_, ConsoleRuntime>,
    password: String,
) -> Result<String, ConsoleError> {
    let stored = load_stored(&config_path(&app)?)?;
    verify_password(&runtime, &stored, Zeroizing::new(password))?;
    new_session(&runtime)
}

#[tauri::command]
fn lock(runtime: State<'_, ConsoleRuntime>, session_token: String) -> Result<(), ConsoleError> {
    let mut auth = runtime
        .0
        .lock()
        .map_err(|_| ConsoleError::StorageUnavailable)?;
    auth.sessions.remove(&session_token);
    Ok(())
}

#[tauri::command]
fn load_console(
    app: AppHandle,
    runtime: State<'_, ConsoleRuntime>,
    session_token: String,
) -> Result<Value, ConsoleError> {
    authorize(&runtime, &session_token)?;
    Ok(load_stored(&config_path(&app)?)?.state)
}

#[tauri::command]
fn save_console(
    app: AppHandle,
    runtime: State<'_, ConsoleRuntime>,
    session_token: String,
    state: Value,
) -> Result<(), ConsoleError> {
    authorize(&runtime, &session_token)?;
    if serde_json::to_vec(&state)
        .map_err(|_| ConsoleError::StateTooLarge)?
        .len()
        > MAX_CONFIG_BYTES / 2
    {
        return Err(ConsoleError::StateTooLarge);
    }
    let path = config_path(&app)?;
    let mut stored = load_stored(&path)?;
    stored.state = state;
    save_stored(&path, &stored)
}

#[tauri::command]
fn reveal_evidence(
    app: AppHandle,
    runtime: State<'_, ConsoleRuntime>,
    session_token: String,
    evidence_id: String,
    password: String,
) -> Result<String, ConsoleError> {
    authorize(&runtime, &session_token)?;
    let stored = load_stored(&config_path(&app)?)?;
    verify_password(&runtime, &stored, Zeroizing::new(password))?;
    if evidence_id.len() > 128
        || !evidence_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(ConsoleError::EvidenceUnavailable);
    }
    Err(ConsoleError::EvidenceUnavailable)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ConsoleRuntime::default())
        .invoke_handler(tauri::generate_handler![
            auth_status,
            enroll,
            unlock,
            lock,
            load_console,
            save_console,
            reveal_evidence
        ])
        .run(tauri::generate_context!())
        .expect("Karma administration console failed to start");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_contains_no_evidence_or_sensitive_content() {
        let state = default_console_state();
        assert_eq!(state["evidence"], json!([]));
        assert_eq!(state["audit"], json!([]));
        assert!(!state.to_string().contains("thumbnailUrl"));
    }

    #[test]
    fn sessions_expire_and_unknown_tokens_are_rejected() {
        let runtime = ConsoleRuntime::default();
        assert!(matches!(
            authorize(&runtime, "unknown"),
            Err(ConsoleError::SessionExpired)
        ));
        let token = new_session(&runtime).unwrap();
        assert!(authorize(&runtime, &token).is_ok());
    }

    #[test]
    fn expired_sessions_are_removed_before_authorization() {
        let runtime = ConsoleRuntime::default();
        runtime
            .0
            .lock()
            .unwrap()
            .sessions
            .insert("expired".into(), Instant::now() - Duration::from_secs(1));
        assert!(matches!(
            authorize(&runtime, "expired"),
            Err(ConsoleError::SessionExpired)
        ));
        assert!(runtime.0.lock().unwrap().sessions.is_empty());
    }

    #[test]
    fn password_verification_accepts_only_the_enrolled_secret() {
        let salt = SaltString::generate(&mut PasswordOsRng);
        let password_hash = Argon2::default()
            .hash_password(b"correct-password", &salt)
            .unwrap()
            .to_string();
        let stored = StoredConsole {
            schema_version: 1,
            password_hash,
            state: default_console_state(),
        };
        let runtime = ConsoleRuntime::default();
        assert!(
            verify_password(&runtime, &stored, Zeroizing::new("correct-password".into())).is_ok()
        );
        assert!(matches!(
            verify_password(&runtime, &stored, Zeroizing::new("wrong-password".into())),
            Err(ConsoleError::InvalidPassword)
        ));
    }

    #[test]
    fn stored_console_round_trips_without_plaintext_password() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("console.json");
        let stored = StoredConsole {
            schema_version: 1,
            password_hash: "$argon2id$test".into(),
            state: default_console_state(),
        };
        save_stored(&path, &stored).unwrap();
        let encoded = fs::read_to_string(&path).unwrap();
        assert!(!encoded.contains("password\":"));
        let loaded = load_stored(&path).unwrap();
        assert_eq!(loaded.schema_version, 1);
    }
}
