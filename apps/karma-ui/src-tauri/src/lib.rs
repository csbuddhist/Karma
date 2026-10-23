#[cfg(desktop)]
use tauri::{
    App, AppHandle, Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

mod installed_apps;

#[cfg(desktop)]
const MAIN_WINDOW_LABEL: &str = "main";
#[cfg(desktop)]
const OPEN_CONSOLE_MENU_ID: &str = "karma-open-console";
#[cfg(desktop)]
const QUIT_MENU_ID: &str = "karma-quit";
#[cfg(desktop)]
const AUTOSTART_ARGUMENT: &str = "--autostart";

fn synchronize_recognition_threshold(state: &mut serde_json::Value) {
    let sensitivity = state.pointer("/recognition/sensitivity").cloned();
    if let (Some(recognition), Some(sensitivity)) = (
        state
            .get_mut("recognition")
            .and_then(serde_json::Value::as_object_mut),
        sensitivity,
    ) {
        recognition.insert("immediateThreshold".into(), sensitivity);
    }
}

const EXPORT_SCHEMA: &str = "karma-policy-export";
const EXPORT_VERSION: u32 = 1;
const MAX_EXPORT_BYTES: usize = 1_048_576;
const RUNTIME_STATE_KEYS: [&str; 13] = [
    "serviceConnected",
    "agentConnected",
    "monitors",
    "evidence",
    "audit",
    "network",
    "rememberedTitleTotal",
    "misjudgedTitleTotal",
    "activeBans",
    "productVersion",
    "superPasswordSet",
    "uninstall",
    "update",
];

fn strip_runtime_state(state: &mut serde_json::Value) {
    if let Some(object) = state.as_object_mut() {
        for key in RUNTIME_STATE_KEYS {
            object.remove(key);
        }
    }
}

fn build_export_document(policy: serde_json::Value) -> serde_json::Value {
    let mut policy = policy;
    strip_runtime_state(&mut policy);
    let exported_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default();
    serde_json::json!({
        "schema": EXPORT_SCHEMA,
        "version": EXPORT_VERSION,
        "exported_at_ms": exported_at_ms,
        "policy": policy
    })
}

fn parse_import_document(bytes: &[u8]) -> Option<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ImportDocument {
        schema: String,
        version: u32,
        exported_at_ms: u64,
        policy: serde_json::Value,
    }
    if bytes.len() > MAX_EXPORT_BYTES {
        return None;
    }
    let document: ImportDocument = serde_json::from_slice(bytes).ok()?;
    // The timestamp only proves the field is present; it never influences import.
    let _ = document.exported_at_ms;
    if document.schema != EXPORT_SCHEMA
        || document.version != EXPORT_VERSION
        || !document.policy.is_object()
    {
        return None;
    }
    let mut policy = document.policy;
    strip_runtime_state(&mut policy);
    // Reject backups whose policy cannot be enforced before the webview sees them.
    karma_policy::ContextPolicy::from_value(&policy).ok()?;
    karma_policy::NetworkSchedulePolicy::from_value(&policy).ok()?;
    karma_policy::RepeatOffenderRule::from_value(&policy).ok()?;
    Some(policy)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{build_export_document, parse_import_document};

    #[test]
    fn console_browser_list_matches_the_policy_list() {
        let bridge = include_str!("../../src/bridge.ts");
        let start = bridge
            .find("export const knownBrowserExecutables")
            .expect("bridge.ts declares knownBrowserExecutables");
        let list = &bridge[start..];
        let list = &list[list.find("= [").unwrap() + 3..list.find("];").unwrap()];
        let console: Vec<&str> = list
            .split(',')
            .map(|item| item.trim().trim_matches('"'))
            .filter(|item| !item.is_empty())
            .collect();
        assert_eq!(console, karma_policy::KNOWN_BROWSER_EXECUTABLES);
    }

    #[test]
    fn export_document_round_trips_and_omits_runtime_state() {
        let document = build_export_document(json!({
            "protectionEnabled": true,
            "keywords": [
                {"id":"k1","phrase":"自定义词","category":"high_risk","enabled":true}
            ],
            "serviceConnected": true,
            "monitors": [{"id": "m1"}],
            "evidence": [],
            "audit": [],
            "rememberedTitleTotal": 3,
            "misjudgedTitleTotal": 1,
            "activeBans": [],
            "superPasswordSet": true,
            "productVersion": "0.3.12",
            "uninstall": {"requestedAtMs": 1, "effectiveAtMs": 2, "expiresAtMs": 3},
            "update": {"phase": "checking"},
            "updates": {"autoInstall": false}
        }));

        assert_eq!(document["schema"], "karma-policy-export");
        assert_eq!(document["version"], 1);
        let bytes = serde_json::to_vec(&document).unwrap();
        let policy = parse_import_document(&bytes).expect("export must re-import");
        assert_eq!(policy["protectionEnabled"], true);
        assert_eq!(policy["keywords"].as_array().unwrap().len(), 1);
        assert!(policy.get("serviceConnected").is_none());
        assert!(policy.get("monitors").is_none());
        assert!(policy.get("rememberedTitleTotal").is_none());
        assert!(policy.get("misjudgedTitleTotal").is_none());
        assert!(policy.get("superPasswordSet").is_none());
        assert!(policy.get("productVersion").is_none());
        assert!(policy.get("uninstall").is_none());
        assert!(policy.get("update").is_none());
        assert_eq!(policy["updates"]["autoInstall"], false);
    }

    #[test]
    fn import_rejects_foreign_documents_large_files_and_invalid_policies() {
        assert!(parse_import_document(b"not json").is_none());
        assert!(
            parse_import_document(
                br#"{"schema":"other-format","version":1,"exported_at_ms":1,"policy":{}}"#
            )
            .is_none()
        );
        assert!(
            parse_import_document(
                br#"{"schema":"karma-policy-export","version":2,"exported_at_ms":1,"policy":{}}"#
            )
            .is_none()
        );
        let oversized = vec![b' '; super::MAX_EXPORT_BYTES + 1];
        assert!(parse_import_document(&oversized).is_none());
        // A syntactically valid backup whose website rules cannot be enforced
        // must be refused instead of reaching the console.
        let invalid_policy = br#"{"schema":"karma-policy-export","version":1,"exported_at_ms":1,
            "policy":{"websites":[{"id":"bad","pattern":"example.com/path","action":"block","enabled":true}]}}"#;
        assert!(parse_import_document(invalid_policy).is_none());
        let invalid_schedule = br#"{"schema":"karma-policy-export","version":1,"exported_at_ms":1,
            "policy":{"schedules":[{"id":"s","name":"s","target":"browsers","blocks":[{"day":9,"start":"08:00","end":"09:00"}],"enabled":true}]}}"#;
        assert!(parse_import_document(invalid_schedule).is_none());
        let invalid_repeat_offender =
            br#"{"schema":"karma-policy-export","version":1,"exported_at_ms":1,
            "policy":{"repeatOffender":{"windowMinutes":20,"strikeLimit":100}}}"#;
        assert!(parse_import_document(invalid_repeat_offender).is_none());
    }
}

#[cfg(desktop)]
fn show_console(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(desktop)]
fn setup_tray(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let open_console =
        MenuItem::with_id(app, OPEN_CONSOLE_MENU_ID, "打开控制台", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_MENU_ID, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_console, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Karma 家庭保护")
        .on_menu_event(|app, event| match event.id().as_ref() {
            OPEN_CONSOLE_MENU_ID => show_console(app),
            QUIT_MENU_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                show_console(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    if std::env::args_os().any(|argument| argument == AUTOSTART_ARGUMENT) {
        if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
            window.hide()?;
        }
    }
    Ok(())
}

fn desktop_builder() -> tauri::Builder<tauri::Wry> {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg(AUTOSTART_ARGUMENT)
                .app_name("Karma Family Protection")
                .build(),
        );

    #[cfg(desktop)]
    let builder = builder.setup(setup_tray).on_window_event(|window, event| {
        if window.label() != MAIN_WINDOW_LABEL {
            return;
        }
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window.hide();
        }
    });

    builder
}

#[cfg(not(windows))]
mod local_backend {
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
        #[error("KarmaService 尚未运行或命名管道不可用")]
        ServiceUnavailable,
        #[error("无法写入导出文件")]
        ExportFailed,
        #[error("备份文件无法读取或格式不正确")]
        InvalidBackup,
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
            "launchAtStartup": true,
            "updates": { "autoInstall": true },
            "serviceConnected": false,
            "agentConnected": false,
            "monitors": [],
            "recognition": {
                "imageEnabled": true,
                "ocrEnabled": true,
                "titleMatchingEnabled": true,
                "sensitivity": 82,
                "immediateThreshold": 82,
                "evidenceEnabled": false,
                "evidenceRetentionDays": 7,
                "evidenceSkipAllowedApplications": false,
                "rememberTitlesEnabled": true,
                "ambiguousTitleKeywordsEnabled": false
            },
            "repeatOffender": {"windowMinutes": 20, "strikeLimit": 5},
            "keywords": [],
            "titleKeywordAdditions": [],
            "websites": [],
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
            "audit": [],
            "rememberedTitleTotal": 0,
            "misjudgedTitleTotal": 0,
            "activeBans": []
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
        let parsed = PasswordHash::new(&stored.password_hash)
            .map_err(|_| ConsoleError::StorageUnavailable)?;
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

    fn apply_password_change(
        runtime: &ConsoleRuntime,
        stored: &mut StoredConsole,
        current_password: Zeroizing<String>,
        new_password: Zeroizing<String>,
    ) -> Result<(), ConsoleError> {
        verify_password(runtime, stored, current_password)?;
        if new_password.chars().count() < 10 {
            return Err(ConsoleError::PasswordTooShort);
        }
        let salt = SaltString::generate(&mut PasswordOsRng);
        stored.password_hash = Argon2::default()
            .hash_password(new_password.as_bytes(), &salt)
            .map_err(|_| ConsoleError::StorageUnavailable)?
            .to_string();
        Ok(())
    }

    #[tauri::command]
    fn change_password(
        app: AppHandle,
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        current_password: String,
        new_password: String,
    ) -> Result<(), ConsoleError> {
        authorize(&runtime, &session_token)?;
        if new_password.chars().count() < 10 {
            return Err(ConsoleError::PasswordTooShort);
        }
        let path = config_path(&app)?;
        let mut stored = load_stored(&path)?;
        apply_password_change(
            &runtime,
            &mut stored,
            Zeroizing::new(current_password),
            Zeroizing::new(new_password),
        )?;
        save_stored(&path, &stored)
    }

    #[tauri::command]
    fn export_settings(
        app: AppHandle,
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        path: String,
    ) -> Result<(), ConsoleError> {
        authorize(&runtime, &session_token)?;
        let stored = load_stored(&config_path(&app)?)?;
        let bytes = serde_json::to_vec(&super::build_export_document(stored.state))
            .map_err(|_| ConsoleError::StorageUnavailable)?;
        fs::write(path, bytes).map_err(|_| ConsoleError::ExportFailed)
    }

    #[tauri::command]
    fn import_settings(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        path: String,
    ) -> Result<Value, ConsoleError> {
        authorize(&runtime, &session_token)?;
        let bytes = fs::read(path).map_err(|_| ConsoleError::InvalidBackup)?;
        super::parse_import_document(&bytes).ok_or(ConsoleError::InvalidBackup)
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
        runtime_only: Option<bool>,
    ) -> Result<Value, ConsoleError> {
        let _ = runtime_only;
        authorize(&runtime, &session_token)?;
        Ok(load_stored(&config_path(&app)?)?.state)
    }

    #[tauri::command]
    fn save_console(
        app: AppHandle,
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        mut state: Value,
        super_password: Option<String>,
    ) -> Result<(), ConsoleError> {
        let _ = super_password.map(Zeroizing::new);
        authorize(&runtime, &session_token)?;
        super::synchronize_recognition_threshold(&mut state);
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

    /// Development builds never store evidence, so there is nothing to delete.
    #[tauri::command]
    fn delete_evidence(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        evidence_ids: Vec<String>,
    ) -> Result<(), ConsoleError> {
        authorize(&runtime, &session_token)?;
        let _ = evidence_ids;
        Err(ConsoleError::EvidenceUnavailable)
    }

    /// Development builds never enforce network schedules, so a verified
    /// restore only reports the unrestricted state.
    #[tauri::command]
    fn restore_network(
        app: AppHandle,
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        password: String,
    ) -> Result<Value, ConsoleError> {
        authorize(&runtime, &session_token)?;
        let stored = load_stored(&config_path(&app)?)?;
        verify_password(&runtime, &stored, Zeroizing::new(password))?;
        Ok(json!({
            "restricted": false,
            "activeRuleIds": [],
            "restrictedUntilMs": null,
            "restoredUntilMs": null
        }))
    }

    /// Development builds never close windows, so nothing is remembered.
    #[tauri::command]
    fn list_remembered_titles(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        offset: u32,
        limit: u32,
        query: Option<String>,
        misjudged: Option<bool>,
    ) -> Result<Value, ConsoleError> {
        authorize(&runtime, &session_token)?;
        let _ = (offset, limit, query, misjudged);
        Ok(json!({"items": [], "total": 0}))
    }

    #[tauri::command]
    fn mark_title_misjudged(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        title_id: String,
        super_password: Option<String>,
    ) -> Result<(), ConsoleError> {
        authorize(&runtime, &session_token)?;
        let _ = (title_id, super_password.map(Zeroizing::new));
        Ok(())
    }

    #[tauri::command]
    fn restore_misjudged_title(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        title_id: String,
    ) -> Result<(), ConsoleError> {
        authorize(&runtime, &session_token)?;
        let _ = title_id;
        Ok(())
    }

    #[tauri::command]
    fn forget_remembered_title(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        title_id: String,
        super_password: Option<String>,
    ) -> Result<(), ConsoleError> {
        authorize(&runtime, &session_token)?;
        let _ = (title_id, super_password.map(Zeroizing::new));
        Ok(())
    }

    /// Repeat-offender bans exist only in the Windows Service.
    #[tauri::command]
    fn allow_banned_application(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        executable: String,
        super_password: Option<String>,
    ) -> Result<Value, ConsoleError> {
        authorize(&runtime, &session_token)?;
        let _ = (executable, super_password.map(Zeroizing::new));
        Err(ConsoleError::ServiceUnavailable)
    }

    /// Evidence and its attributed executable exist only in the Windows Service.
    #[tauri::command]
    fn allow_evidence_application(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        evidence_id: String,
        super_password: Option<String>,
    ) -> Result<Value, ConsoleError> {
        authorize(&runtime, &session_token)?;
        let _ = (evidence_id, super_password.map(Zeroizing::new));
        Err(ConsoleError::ServiceUnavailable)
    }

    #[tauri::command]
    async fn list_installed_applications(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
    ) -> Result<Vec<crate::installed_apps::InstalledApplication>, ConsoleError> {
        authorize(&runtime, &session_token)?;
        tauri::async_runtime::spawn_blocking(crate::installed_apps::list)
            .await
            .map_err(|_| ConsoleError::StorageUnavailable)
    }

    /// Authorization and update state belongs to the Windows Service.
    #[tauri::command]
    fn authorization_status(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
    ) -> Result<Value, ConsoleError> {
        authorize(&runtime, &session_token)?;
        Ok(json!({
            "productVersion": karma_ipc::PRODUCT_VERSION,
            "superPasswordSet": false,
            "uninstall": null,
            "update": {"phase": "idle", "lastCheckedAtMs": null, "availableVersion": null, "lastError": null}
        }))
    }

    #[tauri::command]
    fn set_super_password(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        admin_password: String,
        super_password: String,
    ) -> Result<(), ConsoleError> {
        let _ = (
            Zeroizing::new(admin_password),
            Zeroizing::new(super_password),
        );
        authorize(&runtime, &session_token)?;
        Err(ConsoleError::ServiceUnavailable)
    }

    #[tauri::command]
    fn change_super_password(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        current_super_password: String,
        new_super_password: String,
    ) -> Result<(), ConsoleError> {
        let _ = (
            Zeroizing::new(current_super_password),
            Zeroizing::new(new_super_password),
        );
        authorize(&runtime, &session_token)?;
        Err(ConsoleError::ServiceUnavailable)
    }

    #[tauri::command]
    fn request_uninstall(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
        password: String,
    ) -> Result<(), ConsoleError> {
        let _ = Zeroizing::new(password);
        authorize(&runtime, &session_token)?;
        Err(ConsoleError::ServiceUnavailable)
    }

    #[tauri::command]
    fn cancel_uninstall(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
    ) -> Result<(), ConsoleError> {
        authorize(&runtime, &session_token)?;
        Err(ConsoleError::ServiceUnavailable)
    }

    #[tauri::command]
    fn check_for_updates(
        runtime: State<'_, ConsoleRuntime>,
        session_token: String,
    ) -> Result<(), ConsoleError> {
        authorize(&runtime, &session_token)?;
        Err(ConsoleError::ServiceUnavailable)
    }

    #[cfg_attr(mobile, tauri::mobile_entry_point)]
    pub(super) fn run() {
        super::desktop_builder()
            .manage(ConsoleRuntime::default())
            .invoke_handler(tauri::generate_handler![
                auth_status,
                enroll,
                unlock,
                change_password,
                authorization_status,
                set_super_password,
                change_super_password,
                request_uninstall,
                cancel_uninstall,
                check_for_updates,
                lock,
                load_console,
                save_console,
                reveal_evidence,
                delete_evidence,
                restore_network,
                list_remembered_titles,
                forget_remembered_title,
                mark_title_misjudged,
                restore_misjudged_title,
                allow_banned_application,
                allow_evidence_application,
                list_installed_applications,
                export_settings,
                import_settings
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
            assert_eq!(state["launchAtStartup"], true);
            assert_eq!(state["evidence"], json!([]));
            assert_eq!(state["audit"], json!([]));
            assert!(!state.to_string().contains("thumbnailUrl"));
        }

        #[test]
        fn saving_sensitivity_synchronizes_immediate_threshold() {
            let mut state = json!({
                "recognition": {
                    "sensitivity": 67,
                    "immediateThreshold": 95
                }
            });

            super::super::synchronize_recognition_threshold(&mut state);

            assert_eq!(state["recognition"]["immediateThreshold"], 67);
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
        fn password_verification_accepts_only_the_enrolled_secret() {
            let salt = SaltString::generate(&mut PasswordOsRng);
            let stored = StoredConsole {
                schema_version: 1,
                password_hash: Argon2::default()
                    .hash_password(b"correct-password", &salt)
                    .unwrap()
                    .to_string(),
                state: default_console_state(),
            };
            let runtime = ConsoleRuntime::default();
            assert!(
                verify_password(&runtime, &stored, Zeroizing::new("correct-password".into()))
                    .is_ok()
            );
            assert!(matches!(
                verify_password(&runtime, &stored, Zeroizing::new("wrong-password".into())),
                Err(ConsoleError::InvalidPassword)
            ));
        }

        #[test]
        fn password_change_keeps_old_secret_until_current_secret_verifies() {
            let salt = SaltString::generate(&mut PasswordOsRng);
            let mut stored = StoredConsole {
                schema_version: 1,
                password_hash: Argon2::default()
                    .hash_password(b"correct-password", &salt)
                    .unwrap()
                    .to_string(),
                state: default_console_state(),
            };
            let runtime = ConsoleRuntime::default();
            assert!(matches!(
                apply_password_change(
                    &runtime,
                    &mut stored,
                    Zeroizing::new("wrong-password".into()),
                    Zeroizing::new("replacement-password".into()),
                ),
                Err(ConsoleError::InvalidPassword)
            ));
            assert!(matches!(
                apply_password_change(
                    &runtime,
                    &mut stored,
                    Zeroizing::new("correct-password".into()),
                    Zeroizing::new("short".into()),
                ),
                Err(ConsoleError::PasswordTooShort)
            ));
            assert!(
                verify_password(&runtime, &stored, Zeroizing::new("correct-password".into()))
                    .is_ok()
            );
            apply_password_change(
                &runtime,
                &mut stored,
                Zeroizing::new("correct-password".into()),
                Zeroizing::new("replacement-password".into()),
            )
            .unwrap();
            assert!(matches!(
                verify_password(&runtime, &stored, Zeroizing::new("correct-password".into())),
                Err(ConsoleError::InvalidPassword)
            ));
            assert!(
                verify_password(
                    &runtime,
                    &stored,
                    Zeroizing::new("replacement-password".into())
                )
                .is_ok()
            );
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
            assert_eq!(load_stored(&path).unwrap().schema_version, 1);
        }
    }
}

#[cfg(windows)]
mod service_backend {
    use std::{collections::HashMap, fmt::Write as _, sync::Mutex};

    use karma_ipc::{
        BootstrapStatus, ComponentState, MAX_EVIDENCE_BATCH, RequestEnvelope, ServiceErrorCode,
        ServiceFailure, ServiceRequest, ServiceResult,
    };
    use karma_windows_ipc::send_request;
    use rand::{RngCore, rngs::OsRng};
    use serde_json::{Value, json};
    use tauri::State;
    use thiserror::Error;

    #[derive(Debug, Error)]
    enum ConsoleError {
        #[error("KarmaService 尚未运行或命名管道不可用")]
        ServiceUnavailable,
        #[error("管理员密码至少需要 10 个字符")]
        PasswordTooShort,
        #[error("密码不正确")]
        InvalidPassword,
        #[error("验证失败次数过多，请稍后重试")]
        RateLimited,
        #[error("管理会话已失效，请重新解锁")]
        SessionExpired,
        #[error("控制台已经完成初始化")]
        AlreadyEnrolled,
        #[error("控制台尚未设置管理员密码")]
        NotEnrolled,
        #[error("设置已被另一个管理会话修改，当前修改已保留，请检查后再次保存")]
        RevisionConflict,
        #[error("该事件没有可用的加密原图")]
        EvidenceUnavailable,
        #[error("super_password_required")]
        SuperPasswordRequired,
        #[error("更新必须为经过签名验证的较新版本")]
        UpdateRejected,
        #[error("保护服务拒绝了请求")]
        RequestDenied,
        #[error("保护服务返回了无效响应")]
        InvalidResponse,
        #[error("无法写入导出文件")]
        ExportFailed,
        #[error("备份文件无法读取或格式不正确")]
        InvalidBackup,
    }

    impl serde::Serialize for ConsoleError {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serializer.serialize_str(&self.to_string())
        }
    }

    #[derive(Default)]
    struct ServiceUiRuntime(Mutex<HashMap<String, u64>>);

    fn default_console_state() -> Value {
        json!({
            "protectionEnabled": true,
            "launchAtStartup": true,
            "updates": { "autoInstall": true },
            "serviceConnected": true,
            "agentConnected": false,
            "monitors": [],
            "recognition": {
                "imageEnabled": true,
                "ocrEnabled": true,
                "titleMatchingEnabled": true,
                "sensitivity": 82,
                "immediateThreshold": 82,
                "evidenceEnabled": false,
                "evidenceRetentionDays": 7,
                "evidenceSkipAllowedApplications": false,
                "rememberTitlesEnabled": true,
                "ambiguousTitleKeywordsEnabled": false
            },
            "repeatOffender": {"windowMinutes": 20, "strikeLimit": 5},
            "keywords": [],
            "titleKeywordAdditions": [],
            "websites": [],
            "applications": [
                {"id":"browser","name":"浏览器","executable":"受支持浏览器","category":"browser","action":"content_only","enabled":true},
                {"id":"player","name":"播放器","executable":"受支持播放器","category":"player","action":"content_only","enabled":true}
            ],
            "schedules": [],
            "evidence": [],
            "audit": [],
            "rememberedTitleTotal": 0,
            "misjudgedTitleTotal": 0,
            "activeBans": []
        })
    }

    fn opaque() -> String {
        let mut bytes = [0_u8; 24];
        OsRng.fill_bytes(&mut bytes);
        bytes
            .iter()
            .fold(String::with_capacity(48), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            })
    }

    fn request(body: ServiceRequest) -> Result<ServiceResult, ConsoleError> {
        let envelope = RequestEnvelope::new(opaque(), opaque(), karma_ipc::ClientKind::Ui, body);
        let response =
            send_request(&envelope, 3000).map_err(|_| ConsoleError::ServiceUnavailable)?;
        response.result.map_err(map_failure)
    }

    fn map_failure(failure: ServiceFailure) -> ConsoleError {
        match failure.code {
            ServiceErrorCode::AuthenticationFailed => ConsoleError::InvalidPassword,
            ServiceErrorCode::AuthenticationRequired => ConsoleError::SessionExpired,
            ServiceErrorCode::RateLimited => ConsoleError::RateLimited,
            ServiceErrorCode::AlreadyEnrolled => ConsoleError::AlreadyEnrolled,
            ServiceErrorCode::NotEnrolled => ConsoleError::NotEnrolled,
            ServiceErrorCode::RevisionConflict => ConsoleError::RevisionConflict,
            ServiceErrorCode::EvidenceUnavailable => ConsoleError::EvidenceUnavailable,
            ServiceErrorCode::SuperPasswordRequired => ConsoleError::SuperPasswordRequired,
            ServiceErrorCode::UpdateRejected => ConsoleError::UpdateRejected,
            ServiceErrorCode::ServiceUnavailable | ServiceErrorCode::StorageUnavailable => {
                ConsoleError::ServiceUnavailable
            }
            _ => ConsoleError::RequestDenied,
        }
    }

    #[tauri::command]
    fn auth_status() -> Result<&'static str, ConsoleError> {
        match request(ServiceRequest::GetBootstrap)? {
            ServiceResult::Bootstrap(BootstrapStatus::SetupRequired) => Ok("setup"),
            ServiceResult::Bootstrap(BootstrapStatus::Locked) => Ok("locked"),
            _ => Err(ConsoleError::InvalidResponse),
        }
    }

    #[tauri::command]
    fn enroll(password: String) -> Result<String, ConsoleError> {
        session_from(request(ServiceRequest::EnrollAdministrator { password })?)
    }

    #[tauri::command]
    fn unlock(password: String) -> Result<String, ConsoleError> {
        session_from(request(ServiceRequest::Authenticate { password })?)
    }

    #[tauri::command]
    fn change_password(
        session_token: String,
        current_password: String,
        new_password: String,
    ) -> Result<(), ConsoleError> {
        if new_password.chars().count() < 10 {
            return Err(ConsoleError::PasswordTooShort);
        }
        let result = request(ServiceRequest::ChangePassword {
            session_token,
            current_password,
            new_password,
        })?;
        acknowledged(result)
    }

    #[tauri::command]
    fn export_settings(session_token: String, path: String) -> Result<(), ConsoleError> {
        let policy = match request(ServiceRequest::GetPolicy { session_token })? {
            ServiceResult::Policy { policy, .. } => policy,
            _ => return Err(ConsoleError::InvalidResponse),
        };
        let bytes = serde_json::to_vec(&super::build_export_document(policy))
            .map_err(|_| ConsoleError::InvalidResponse)?;
        std::fs::write(path, bytes).map_err(|_| ConsoleError::ExportFailed)
    }

    #[tauri::command]
    fn import_settings(
        runtime: State<'_, ServiceUiRuntime>,
        session_token: String,
        path: String,
    ) -> Result<Value, ConsoleError> {
        if !runtime
            .0
            .lock()
            .map_err(|_| ConsoleError::InvalidResponse)?
            .contains_key(&session_token)
        {
            return Err(ConsoleError::SessionExpired);
        }
        let bytes = std::fs::read(path).map_err(|_| ConsoleError::InvalidBackup)?;
        super::parse_import_document(&bytes).ok_or(ConsoleError::InvalidBackup)
    }

    fn session_from(result: ServiceResult) -> Result<String, ConsoleError> {
        match result {
            ServiceResult::Session { session_token, .. } => Ok(session_token),
            _ => Err(ConsoleError::InvalidResponse),
        }
    }

    #[tauri::command]
    fn lock(
        runtime: State<'_, ServiceUiRuntime>,
        session_token: String,
    ) -> Result<(), ConsoleError> {
        let result = request(ServiceRequest::LockSession {
            session_token: session_token.clone(),
        })?;
        runtime
            .0
            .lock()
            .map_err(|_| ConsoleError::InvalidResponse)?
            .remove(&session_token);
        acknowledged(result)
    }

    fn authorization_json(status: &karma_ipc::ServiceStatus) -> Value {
        json!({
            "productVersion": status.product_version,
            "superPasswordSet": status.super_password_set,
            "uninstall": status.uninstall.map(|uninstall| json!({
                "requestedAtMs": uninstall.requested_at_ms,
                "effectiveAtMs": uninstall.effective_at_ms,
                "expiresAtMs": uninstall.expires_at_ms
            })),
            "update": {
                "phase": status.update.phase,
                "lastCheckedAtMs": status.update.last_checked_at_ms,
                "availableVersion": status.update.available_version,
                "lastError": status.update.last_error
            }
        })
    }

    #[tauri::command]
    fn authorization_status(session_token: String) -> Result<Value, ConsoleError> {
        match request(ServiceRequest::GetStatus { session_token })? {
            ServiceResult::Status(status) => Ok(authorization_json(&status)),
            _ => Err(ConsoleError::InvalidResponse),
        }
    }

    #[tauri::command]
    fn load_console(
        runtime: State<'_, ServiceUiRuntime>,
        session_token: String,
        runtime_only: Option<bool>,
    ) -> Result<Value, ConsoleError> {
        let (revision, policy) = match request(ServiceRequest::GetPolicy {
            session_token: session_token.clone(),
        })? {
            ServiceResult::Policy { revision, policy } => (revision, policy),
            _ => return Err(ConsoleError::InvalidResponse),
        };
        let status = match request(ServiceRequest::GetStatus {
            session_token: session_token.clone(),
        })? {
            ServiceResult::Status(status) => status,
            _ => return Err(ConsoleError::InvalidResponse),
        };
        let evidence = match request(ServiceRequest::ListEvidence {
            session_token: session_token.clone(),
        })? {
            ServiceResult::EvidenceList { items } => items,
            _ => return Err(ConsoleError::InvalidResponse),
        };
        // Only the bans and the title count; the console pages titles itself.
        let enforcement = match request(ServiceRequest::ListEnforcement {
            session_token: session_token.clone(),
            title_offset: 0,
            title_limit: 1,
            title_query: None,
            misjudged: false,
        })? {
            ServiceResult::Enforcement(summary) => summary,
            _ => return Err(ConsoleError::InvalidResponse),
        };
        let audit = match request(ServiceRequest::ListAudit {
            session_token: session_token.clone(),
            limit: karma_ipc::MAX_AUDIT_PAGE,
        })? {
            ServiceResult::AuditList { items } => items,
            _ => return Err(ConsoleError::InvalidResponse),
        };
        if !runtime_only.unwrap_or(false) {
            runtime
                .0
                .lock()
                .map_err(|_| ConsoleError::InvalidResponse)?
                .insert(session_token, revision);
        }

        let mut state = default_console_state();
        if let (Some(target), Some(source)) = (state.as_object_mut(), policy.as_object()) {
            for (key, value) in source {
                target.insert(key.clone(), value.clone());
            }
        }
        let object = state.as_object_mut().ok_or(ConsoleError::InvalidResponse)?;
        if let Some(authorization) = authorization_json(&status).as_object() {
            object.extend(
                authorization
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        }
        object.insert("serviceConnected".into(), Value::Bool(true));
        object.insert("agentConnected".into(), Value::Bool(status.agent_connected));
        object.insert(
            "protectionEnabled".into(),
            Value::Bool(status.protection_enabled),
        );
        object.insert(
            "monitors".into(),
            Value::Array(status.monitors.into_iter().map(|monitor| json!({
                "id": monitor.monitor_id,
                "name": monitor.name,
                "resolution": format!("{} × {}", monitor.width, monitor.height),
                "state": component_state(monitor.frame_status, monitor.image_status, monitor.ocr_status),
                "fps": 0,
                "latencyMs": monitor.latency_micros / 1000
            })).collect()),
        );
        object.insert("network".into(), network_status_json(&status.network));
        object.insert(
            "rememberedTitleTotal".into(),
            Value::from(enforcement.remembered_title_total),
        );
        object.insert(
            "misjudgedTitleTotal".into(),
            Value::from(enforcement.misjudged_title_total),
        );
        object.insert(
            "activeBans".into(),
            Value::Array(
                enforcement
                    .active_bans
                    .iter()
                    .map(|ban| {
                        json!({
                            "executable": ban.executable,
                            "bannedUntilMs": ban.banned_until_ms
                        })
                    })
                    .collect(),
            ),
        );
        object.insert(
            "audit".into(),
            Value::Array(audit.into_iter().map(audit_entry_json).collect()),
        );
        object.insert(
            "evidence".into(),
            Value::Array(
                evidence
                    .into_iter()
                    .map(|item| {
                        json!({
                            "id": item.id,
                            "capturedAt": item.captured_at_ms.to_string(),
                            "monitorName": item.monitor_name,
                            "applicationName": item.application_name,
                            "reason": item.reason_code,
                            "risk": if item.risk_millis >= 950 { "critical" } else { "high" },
                            "riskMillis": item.risk_millis,
                            "originalAvailable": item.original_available
                        })
                    })
                    .collect(),
            ),
        );
        Ok(state)
    }

    /// An audit row for the console. The service writes `detail` as a JSON
    /// object; anything else is dropped rather than shown raw.
    fn audit_entry_json(entry: karma_ipc::AuditEntry) -> Value {
        let detail = entry
            .detail
            .as_deref()
            .and_then(|detail| serde_json::from_str::<Value>(detail).ok())
            .filter(Value::is_object)
            .unwrap_or(Value::Null);
        json!({
            "id": entry.id.to_string(),
            "occurredAtMs": entry.occurred_at_ms,
            "kind": entry.kind,
            "outcome": entry.outcome,
            "detail": detail
        })
    }

    fn component_state(
        states: ComponentState,
        image: ComponentState,
        ocr: ComponentState,
    ) -> &'static str {
        if [states, image, ocr].contains(&ComponentState::Unavailable) {
            "offline"
        } else if [states, image, ocr]
            .iter()
            .all(|state| *state == ComponentState::Healthy)
        {
            "healthy"
        } else {
            "degraded"
        }
    }

    #[tauri::command]
    fn save_console(
        runtime: State<'_, ServiceUiRuntime>,
        session_token: String,
        mut state: Value,
        super_password: Option<String>,
    ) -> Result<(), ConsoleError> {
        super::synchronize_recognition_threshold(&mut state);
        let revision = *runtime
            .0
            .lock()
            .map_err(|_| ConsoleError::InvalidResponse)?
            .get(&session_token)
            .ok_or(ConsoleError::SessionExpired)?;
        if let Some(object) = state.as_object_mut() {
            for runtime_key in super::RUNTIME_STATE_KEYS {
                object.remove(runtime_key);
            }
        }
        let result = request(ServiceRequest::PutPolicy {
            session_token: session_token.clone(),
            expected_revision: revision,
            policy: state,
            super_password,
        });
        if matches!(result, Err(ConsoleError::RevisionConflict)) {
            // Keep the webview's edits. Refresh only the expected revision so a
            // reviewed, explicit retry can be accepted by the Service.
            if let Ok(ServiceResult::Policy { revision, .. }) = request(ServiceRequest::GetPolicy {
                session_token: session_token.clone(),
            }) {
                runtime
                    .0
                    .lock()
                    .map_err(|_| ConsoleError::InvalidResponse)?
                    .insert(session_token.clone(), revision);
            }
        }
        match result? {
            ServiceResult::PolicySaved { revision } => {
                runtime
                    .0
                    .lock()
                    .map_err(|_| ConsoleError::InvalidResponse)?
                    .insert(session_token, revision);
                Ok(())
            }
            _ => Err(ConsoleError::InvalidResponse),
        }
    }

    #[tauri::command]
    fn reveal_evidence(
        session_token: String,
        evidence_id: String,
        password: String,
    ) -> Result<String, ConsoleError> {
        match request(ServiceRequest::RevealEvidence {
            session_token,
            password,
            evidence_id,
        })? {
            ServiceResult::EvidenceImage {
                media_type,
                bytes_base64,
            } => Ok(format!("data:{media_type};base64,{bytes_base64}")),
            _ => Err(ConsoleError::InvalidResponse),
        }
    }

    /// Deletes evidence items in Service-sized batches. The Service skips items
    /// that are already gone.
    #[tauri::command]
    fn delete_evidence(
        session_token: String,
        evidence_ids: Vec<String>,
    ) -> Result<(), ConsoleError> {
        if evidence_ids.is_empty() {
            return Ok(());
        }
        for chunk in evidence_ids.chunks(MAX_EVIDENCE_BATCH) {
            match request(ServiceRequest::DeleteEvidenceBatch {
                session_token: session_token.clone(),
                evidence_ids: chunk.to_vec(),
            })? {
                ServiceResult::Acknowledged => {}
                _ => return Err(ConsoleError::InvalidResponse),
            }
        }
        Ok(())
    }

    fn network_status_json(status: &karma_ipc::NetworkStatus) -> Value {
        json!({
            "restricted": status.restricted,
            "activeRuleIds": status.active_rule_ids,
            "restrictedUntilMs": status.restricted_until_ms,
            "restoredUntilMs": status.restored_until_ms
        })
    }

    #[tauri::command]
    fn list_remembered_titles(
        session_token: String,
        offset: u32,
        limit: u32,
        query: Option<String>,
        misjudged: Option<bool>,
    ) -> Result<Value, ConsoleError> {
        let summary = match request(ServiceRequest::ListEnforcement {
            session_token,
            title_offset: offset,
            title_limit: limit,
            title_query: query.filter(|query| !query.trim().is_empty()),
            misjudged: misjudged.unwrap_or(false),
        })? {
            ServiceResult::Enforcement(summary) => summary,
            _ => return Err(ConsoleError::InvalidResponse),
        };
        Ok(json!({
            "items": summary.remembered_titles.iter().map(|item| json!({
                "id": item.id,
                "executable": item.executable,
                "title": item.title,
                "browserHost": item.browser_host,
                "rememberedAtMs": item.remembered_at_ms,
                "reason": item.reason.as_ref().map(|reason| json!({
                    "reasonCode": reason.reason_code,
                    "riskMillis": reason.risk_millis,
                    "thresholdMillis": reason.threshold_millis
                }))
            })).collect::<Vec<_>>(),
            "total": summary.remembered_title_total
        }))
    }

    #[tauri::command]
    fn forget_remembered_title(
        session_token: String,
        title_id: String,
        super_password: Option<String>,
    ) -> Result<(), ConsoleError> {
        acknowledged(request(ServiceRequest::ForgetRememberedTitle {
            session_token,
            title_id,
            super_password,
        })?)
    }

    #[tauri::command]
    fn mark_title_misjudged(
        session_token: String,
        title_id: String,
        super_password: Option<String>,
    ) -> Result<(), ConsoleError> {
        acknowledged(request(ServiceRequest::MarkTitleMisjudged {
            session_token,
            title_id,
            super_password,
        })?)
    }

    #[tauri::command]
    fn restore_misjudged_title(
        session_token: String,
        title_id: String,
    ) -> Result<(), ConsoleError> {
        acknowledged(request(ServiceRequest::RestoreMisjudgedTitle {
            session_token,
            title_id,
        })?)
    }

    /// Returns the application rules the Service saved, so the console can
    /// merge them into its draft.
    #[tauri::command]
    fn allow_banned_application(
        runtime: State<'_, ServiceUiRuntime>,
        session_token: String,
        executable: String,
        super_password: Option<String>,
    ) -> Result<Value, ConsoleError> {
        let expected_revision = *runtime
            .0
            .lock()
            .map_err(|_| ConsoleError::InvalidResponse)?
            .get(&session_token)
            .ok_or(ConsoleError::SessionExpired)?;
        match request(ServiceRequest::AllowBannedApplication {
            session_token: session_token.clone(),
            expected_revision,
            executable,
            super_password,
        })? {
            ServiceResult::Policy { revision, policy } => {
                runtime
                    .0
                    .lock()
                    .map_err(|_| ConsoleError::InvalidResponse)?
                    .insert(session_token, revision);
                Ok(policy
                    .get("applications")
                    .cloned()
                    .unwrap_or_else(|| Value::Array(Vec::new())))
            }
            _ => Err(ConsoleError::InvalidResponse),
        }
    }

    /// Always allows the executable the Service attributed an evidence item
    /// to; returns the saved application rules for the console to merge.
    #[tauri::command]
    fn allow_evidence_application(
        runtime: State<'_, ServiceUiRuntime>,
        session_token: String,
        evidence_id: String,
        super_password: Option<String>,
    ) -> Result<Value, ConsoleError> {
        let expected_revision = *runtime
            .0
            .lock()
            .map_err(|_| ConsoleError::InvalidResponse)?
            .get(&session_token)
            .ok_or(ConsoleError::SessionExpired)?;
        match request(ServiceRequest::AllowEvidenceApplication {
            session_token: session_token.clone(),
            expected_revision,
            evidence_id,
            super_password,
        })? {
            ServiceResult::Policy { revision, policy } => {
                runtime
                    .0
                    .lock()
                    .map_err(|_| ConsoleError::InvalidResponse)?
                    .insert(session_token, revision);
                Ok(policy
                    .get("applications")
                    .cloned()
                    .unwrap_or_else(|| Value::Array(Vec::new())))
            }
            _ => Err(ConsoleError::InvalidResponse),
        }
    }

    /// Reads installed programs as the signed-in user; only a session this
    /// console unlocked may ask.
    #[tauri::command]
    async fn list_installed_applications(
        runtime: State<'_, ServiceUiRuntime>,
        session_token: String,
    ) -> Result<Vec<crate::installed_apps::InstalledApplication>, ConsoleError> {
        if !runtime
            .0
            .lock()
            .map_err(|_| ConsoleError::InvalidResponse)?
            .contains_key(&session_token)
        {
            return Err(ConsoleError::SessionExpired);
        }
        tauri::async_runtime::spawn_blocking(crate::installed_apps::list)
            .await
            .map_err(|_| ConsoleError::InvalidResponse)
    }

    #[tauri::command]
    fn restore_network(session_token: String, password: String) -> Result<Value, ConsoleError> {
        match request(ServiceRequest::RestoreNetwork {
            session_token,
            password,
        })? {
            ServiceResult::NetworkStatus(status) => Ok(network_status_json(&status)),
            _ => Err(ConsoleError::InvalidResponse),
        }
    }

    #[tauri::command]
    fn set_super_password(
        session_token: String,
        admin_password: String,
        super_password: String,
    ) -> Result<(), ConsoleError> {
        acknowledged(request(ServiceRequest::SetSuperPassword {
            session_token,
            admin_password,
            super_password,
        })?)
    }

    #[tauri::command]
    fn change_super_password(
        session_token: String,
        current_super_password: String,
        new_super_password: String,
    ) -> Result<(), ConsoleError> {
        acknowledged(request(ServiceRequest::ChangeSuperPassword {
            session_token,
            current_super_password,
            new_super_password,
        })?)
    }

    #[tauri::command]
    fn request_uninstall(session_token: String, password: String) -> Result<(), ConsoleError> {
        acknowledged(request(ServiceRequest::RequestUninstall {
            session_token,
            password,
        })?)
    }

    #[tauri::command]
    fn cancel_uninstall(session_token: String) -> Result<(), ConsoleError> {
        acknowledged(request(ServiceRequest::CancelUninstall { session_token })?)
    }

    #[tauri::command]
    fn check_for_updates(session_token: String) -> Result<(), ConsoleError> {
        acknowledged(request(ServiceRequest::CheckForUpdates { session_token })?)
    }

    fn acknowledged(result: ServiceResult) -> Result<(), ConsoleError> {
        if matches!(result, ServiceResult::Acknowledged) {
            Ok(())
        } else {
            Err(ConsoleError::InvalidResponse)
        }
    }

    pub(super) fn run() {
        super::desktop_builder()
            .manage(ServiceUiRuntime::default())
            .invoke_handler(tauri::generate_handler![
                auth_status,
                enroll,
                unlock,
                change_password,
                authorization_status,
                set_super_password,
                change_super_password,
                request_uninstall,
                cancel_uninstall,
                check_for_updates,
                lock,
                load_console,
                save_console,
                reveal_evidence,
                delete_evidence,
                restore_network,
                list_remembered_titles,
                forget_remembered_title,
                mark_title_misjudged,
                restore_misjudged_title,
                allow_banned_application,
                allow_evidence_application,
                list_installed_applications,
                export_settings,
                import_settings
            ])
            .run(tauri::generate_context!())
            .expect("Karma administration console failed to start");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(windows)]
    service_backend::run();
    #[cfg(not(windows))]
    local_backend::run();
}
