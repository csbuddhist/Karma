//! Fixed-location, SYSTEM-only update handoff. Neither IPC nor the helper
//! accepts an executable path, command line, process id, or staging location.

use std::{
    env,
    fs::{self, File, OpenOptions},
    io,
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use karma_ipc::{PRODUCT_VERSION, UpdatePhase, UpdateStatus};
use karma_service_core::{ServiceCore, UpdateAction};
use karma_update::{
    MANIFEST_FILE, RELEASE_PUBLIC_KEY, SIGNATURE_FILE, UpdateError, Version, reject_links,
};
use serde::{Deserialize, Serialize};
use windows::{
    Win32::{
        Foundation::{
            ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_SHARING_VIOLATION, HANDLE, HLOCAL,
            LocalFree,
        },
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW,
                GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
            },
            CheckTokenMembership, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
            PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
            SetFileSecurityW,
        },
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, CreateDirectoryW, GetFileInformationByHandle,
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        },
        System::{
            Registry::{
                HKEY, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, KEY_WOW64_64KEY, REG_SZ, RegCloseKey,
                RegOpenKeyExW, RegSetValueExW,
            },
            RestartManager::{
                RmEndSession, RmForceShutdown, RmRegisterResources, RmShutdown, RmStartSession,
            },
        },
    },
    core::{BOOL, PCWSTR, PWSTR, w},
};

use crate::update_transaction::{self as transaction, Phase};

const STATE: &str = "update-state";
const INBOX: &str = "update-inbox";
const STAGED: &str = "staged";
const JOURNAL: &str = "transaction.json";
const WORKER: &str = "worker.exe";
const RESULT: &str = "result.json";
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const REPLACE_RETRIES: u32 = 20;
const ROLLBACK_ATTEMPTS: u32 = 6;
/// Installed executables that run outside SCM and lock their image files.
/// 不受 SCM 管理、运行时会锁定自身映像文件的已安装程序。
const CLIENT_EXECUTABLES: [&str; 3] = [
    "karma-ui.exe",
    "karma-agent-windows.exe",
    "KarmaControl.exe",
];
const PRIVATE_ACL: PCWSTR = w!("D:P(A;OICI;FA;;;SY)");
const INSTALL_ACL: PCWSTR = w!("D:P(A;OICI;FA;;;SY)(A;OICI;GRGX;;;BA)(A;OICI;GRGX;;;BU)");
const UNINSTALL_ACL: PCWSTR = w!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;GRGX;;;BU)");
const INBOX_ACL: PCWSTR = w!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)");

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Completion {
    version: String,
    error: Option<String>,
}

fn write_completion(root: &Path, completion: &Completion) -> Result<(), UpdateError> {
    let pending = root.join("result.pending");
    reject_links(&pending)?;
    if pending.exists() {
        fs::remove_file(&pending)?;
    }
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&pending)?;
    use std::io::Write as _;
    output.write_all(&serde_json::to_vec(completion).map_err(|_| UpdateError::InvalidManifest)?)?;
    output.sync_all()?;
    drop(output);
    replace(&pending, &root.join(RESULT), None)?;
    Ok(())
}

fn import_completion(core: &ServiceCore) -> Result<(), UpdateError> {
    let root = state()?;
    let Ok(_lock) = lock(&root) else {
        return Ok(());
    };
    let path = root.join(RESULT);
    reject_links(&path)?;
    if !path.exists() {
        return Ok(());
    }
    if fs::metadata(&path)?.len() > 4096 {
        return Err(UpdateError::TooLarge);
    }
    let completion: Completion =
        serde_json::from_slice(&fs::read(&path)?).map_err(|_| UpdateError::InvalidManifest)?;
    if completion.error.is_some() {
        status(
            core,
            UpdatePhase::Failed,
            Some(completion.version),
            completion.error,
        )?;
    } else {
        status(core, UpdatePhase::UpToDate, None, None)?;
    }
    fs::remove_file(path)?;
    Ok(())
}

fn error_code(error: &UpdateError) -> &'static str {
    match error {
        UpdateError::Io(_) => "storage",
        UpdateError::InvalidManifest => "invalid_manifest",
        UpdateError::BadSignature => "bad_signature",
        UpdateError::NotNewer { .. } => "not_newer",
        UpdateError::HashMismatch(_) => "hash_mismatch",
        UpdateError::MissingFile(_) => "missing_file",
        UpdateError::TooLarge => "too_large",
        UpdateError::InvalidArchive => "invalid_archive",
        UpdateError::Network => "network",
        UpdateError::UnsafePath => "unsafe_path",
    }
}

fn create_private_directory(path: &Path) -> Result<(), UpdateError> {
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            w!("O:SYD:P(A;OICI;FA;;;SY)"),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
    }
    .map_err(|_| UpdateError::UnsafePath)?;
    let security = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: false.into(),
    };
    let path = wide(path);
    let result = unsafe { CreateDirectoryW(PCWSTR(path.as_ptr()), Some(&security)) }
        .map_err(|error| UpdateError::Io(winerror(error)));
    unsafe {
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
    }
    result
}

fn installed() -> Version {
    PRODUCT_VERSION.parse().expect("product version is valid")
}
fn state() -> Result<PathBuf, UpdateError> {
    Ok(super::service_data_directory()
        .map_err(|_| UpdateError::UnsafePath)?
        .join(STATE))
}
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn winerror(error: windows::core::Error) -> io::Error {
    io::Error::other(error.to_string())
}

/// Every predecessor is copied and synced before the write-ahead journal.
/// Rename the synced same-volume replacement without moving the installed
/// image out of the way first. Never use delete-then-copy or a pending reboot.
fn replace(source: &Path, target: &Path, backup: Option<&Path>) -> io::Result<()> {
    let from = wide(source);
    let to = wide(target);
    if backup.is_some_and(|path| !path.is_file()) {
        return Err(io::Error::other("update predecessor is missing"));
    }
    // A short-lived reader (antivirus, a `KarmaControl version` probe) can hold
    // the target briefly; retry access/sharing violations before failing.
    // 杀毒软件或版本探测可能短暂占用目标文件，拒绝访问/共享冲突时重试。
    let mut attempt = 0;
    loop {
        let moved = unsafe {
            MoveFileExW(
                PCWSTR(from.as_ptr()),
                PCWSTR(to.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        match moved {
            Ok(()) => break,
            Err(error)
                if attempt < REPLACE_RETRIES
                    && [ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION]
                        .iter()
                        .any(|code| error.code() == code.to_hresult()) =>
            {
                attempt += 1;
                thread::sleep(Duration::from_millis(250));
            }
            Err(error) => return Err(winerror(error)),
        }
    }
    OpenOptions::new().write(true).open(target)?.sync_all()
}

fn is_system() -> Result<(), UpdateError> {
    let mut sid = PSID::default();
    unsafe { ConvertStringSidToSidW(w!("S-1-5-18"), &mut sid) }
        .map_err(|_| UpdateError::UnsafePath)?;
    let mut member = BOOL::default();
    let result = unsafe { CheckTokenMembership(None, sid, &mut member) };
    unsafe {
        let _ = LocalFree(Some(HLOCAL(sid.0)));
    }
    if result.is_ok() && member.as_bool() {
        Ok(())
    } else {
        Err(UpdateError::UnsafePath)
    }
}

fn diagnostics_only() -> bool {
    env::args_os().any(|argument| argument == "--console") && is_system().is_err()
}

fn require_system_owner(path: &Path) -> Result<(), UpdateError> {
    let path = wide(path);
    let mut owner = PSID::default();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    unsafe {
        GetNamedSecurityInfoW(
            PCWSTR(path.as_ptr()),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            None,
            None,
            &mut descriptor,
        )
    }
    .ok()
    .map_err(|_| UpdateError::UnsafePath)?;
    let mut expected = PSID::default();
    let result = (|| {
        unsafe { ConvertStringSidToSidW(w!("S-1-5-18"), &mut expected) }
            .map_err(|_| UpdateError::UnsafePath)?;
        unsafe { windows::Win32::Security::EqualSid(owner, expected) }
            .map_err(|_| UpdateError::UnsafePath)
    })();
    unsafe {
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        if !expected.0.is_null() {
            let _ = LocalFree(Some(HLOCAL(expected.0)));
        }
    }
    result
}

fn acl(path: &Path, sddl: PCWSTR) -> Result<(), UpdateError> {
    reject_links(path)?;
    if fs::metadata(path)?.is_file() {
        // Query-only handles do not conflict with the updater's share-zero
        // worker lock. No file contents are read or written for this check.
        let file = OpenOptions::new().access_mode(0).open(path)?;
        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut information) }
            .map_err(|_| UpdateError::UnsafePath)?;
        // Altering an inode's security descriptor would also alter every
        // hardlink outside the install tree. Never change such an object's ACL.
        if information.nNumberOfLinks != 1 {
            return Err(UpdateError::UnsafePath);
        }
    }
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl,
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
    }
    .map_err(|_| UpdateError::UnsafePath)?;
    let path = wide(path);
    let result = unsafe {
        SetFileSecurityW(
            PCWSTR(path.as_ptr()),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    }
    .ok()
    .map_err(|_| UpdateError::UnsafePath);
    unsafe {
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
    }
    result
}

fn tree_acl(directory: &Path, sddl: PCWSTR) -> Result<(), UpdateError> {
    acl(directory, sddl)?;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        reject_links(&entry.path())?;
        if entry.file_type()?.is_dir() {
            tree_acl(&entry.path(), sddl)?;
        } else {
            acl(&entry.path(), sddl)?;
        }
    }
    Ok(())
}

pub fn authorize_uninstall(install: &Path) -> Result<(), UpdateError> {
    tree_acl(install, UNINSTALL_ACL)?;
    let private = state()?;
    if private.exists() {
        tree_acl(&private, INBOX_ACL)?;
    }
    Ok(())
}

fn initialize() -> Result<PathBuf, UpdateError> {
    is_system()?;
    let root = state()?;
    reject_links(&root)?;
    if !root.exists() {
        create_private_directory(&root)?;
    }
    // An administrator may precreate the name under ProgramData. Refuse that
    // object rather than trusting an owner who can subsequently reset its ACL.
    require_system_owner(&root)?;
    acl(&root, PRIVATE_ACL)?;
    let inbox = root.parent().ok_or(UpdateError::UnsafePath)?.join(INBOX);
    reject_links(&inbox)?;
    if !inbox.exists() {
        fs::create_dir(&inbox)?;
    }
    acl(&inbox, INBOX_ACL)?;
    Ok(root)
}

fn lock(root: &Path) -> Result<File, UpdateError> {
    let path = root.join("worker.lock");
    reject_links(&path)?;
    Ok(OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)?)
}

fn launch(root: &Path, recover: bool) -> Result<(), UpdateError> {
    let worker = root.join(WORKER);
    reject_links(&worker)?;
    if !recover {
        if worker.exists() {
            fs::remove_file(&worker)?;
        }
        let mut input = File::open(env::current_exe()?)?;
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&worker)?;
        io::copy(&mut input, &mut output)?;
        output.sync_all()?;
        acl(&worker, PRIVATE_ACL)?;
    }
    Command::new(worker)
        .arg(if recover {
            "--recover-staged-update"
        } else {
            "--apply-staged-update"
        })
        .current_dir(root)
        .spawn()?;
    Ok(())
}

/// Called before opening the core database. An interrupted transaction is
/// recovered outside the installed image, allowing that executable to unlock.
pub fn startup(install: &Path) -> Result<bool, UpdateError> {
    // Retain elevated administrator console diagnostics. Only an SCM/SYSTEM
    // host may initialize privileged update storage or recover transactions.
    if diagnostics_only() {
        return Ok(true);
    }
    let root = initialize()?;
    // Observe the journal under the worker lock when no worker is active; a
    // stale Applied observation must not roll back a just-committed update.
    let worker_lock = match lock(&root) {
        Ok(lock) => Some(lock),
        Err(UpdateError::Io(error)) if matches!(error.raw_os_error(), Some(32 | 33)) => None,
        Err(error) => return Err(error),
    };
    let worker_active = worker_lock.is_none();
    let journal = root.join(JOURNAL);
    if !journal.exists() {
        if worker_lock.is_some() {
            transaction::cleanup_unjournaled(install)?;
        }
        return Ok(true);
    }
    let plan = transaction::read_plan(&journal)?;
    if plan.install != install {
        return Err(UpdateError::UnsafePath);
    }
    if transaction::requires_recovery(plan.phase, worker_active) {
        if !worker_active {
            launch(&root, true)?;
        }
        return Ok(false);
    }
    if plan.version != PRODUCT_VERSION {
        return Err(UpdateError::InvalidManifest);
    }
    // A signed complete image may boot while the worker is waiting for SCM's
    // Running state. The worker commits only after both services start.
    karma_update::verify_directory(
        install,
        plan.installed_version.parse()?,
        &RELEASE_PUBLIC_KEY,
    )?;
    Ok(true)
}

fn status(
    core: &ServiceCore,
    phase: UpdatePhase,
    version: Option<String>,
    error: Option<String>,
) -> Result<(), UpdateError> {
    core.record_update_status(
        UpdateStatus {
            phase,
            last_checked_at_ms: Some(super::unix_time_ms()),
            available_version: version,
            last_error: error,
        },
        super::unix_time_ms(),
    )
    .map_err(|_| UpdateError::UnsafePath)
}

pub fn start(
    core: Arc<ServiceCore>,
    install: PathBuf,
    shutdown: Arc<AtomicBool>,
    updating: Arc<AtomicBool>,
) -> JoinHandle<()> {
    let diagnostic = diagnostics_only();
    thread::Builder::new()
        .name("karma-updater".into())
        .spawn(move || {
            // Give protection workers the first startup interval; then check every
            // six hours. Network operations stay off the IPC and protection threads.
            let mut next_check = Instant::now() + Duration::from_secs(60);
            while !shutdown.load(Ordering::Acquire) {
                if !diagnostic {
                    if let Err(error) = cleanup_completed(&install).map(|completed| {
                        if completed {
                            updating.store(false, Ordering::Release);
                            let _ = status(&core, UpdatePhase::UpToDate, None, None);
                        }
                    }) {
                        eprintln!("Karma update cleanup: {error}");
                    }
                    if !transaction_active() {
                        if let Err(error) = import_completion(&core) {
                            eprintln!("Karma update result: {error}");
                        }
                    }
                }
                let action = core.take_update_action().ok().flatten();
                let auto = core.auto_install_updates().unwrap_or(true);
                let periodic = !diagnostic && Instant::now() >= next_check;
                if action.is_some() || periodic {
                    next_check = Instant::now() + CHECK_INTERVAL;
                    let result = stage_and_handoff(
                        &core,
                        &install,
                        action == Some(UpdateAction::ApplyStaged),
                        action.is_some() || auto,
                        &shutdown,
                    );
                    match result {
                        Ok(true) => {
                            updating.store(true, Ordering::Release);
                            shutdown.store(true, Ordering::Release);
                            super::wake_pipe_server();
                            break;
                        }
                        Ok(false) => {}
                        Err(error) => {
                            let _ = status(
                                &core,
                                UpdatePhase::Failed,
                                None,
                                Some(error_code(&error).into()),
                            );
                            eprintln!("Karma update: {error}");
                        }
                    }
                }
                thread::sleep(Duration::from_secs(1));
            }
        })
        .expect("KarmaService failed to start updater")
}

fn stage_and_handoff(
    core: &ServiceCore,
    install: &Path,
    inbox: bool,
    apply: bool,
    shutdown: &AtomicBool,
) -> Result<bool, UpdateError> {
    let root = initialize()?;
    let _lock = lock(&root)?;
    if root.join(JOURNAL).exists() {
        return Err(UpdateError::UnsafePath);
    }
    transaction::cleanup_unjournaled(install)?;
    if shutdown.load(Ordering::Acquire) {
        return Err(UpdateError::UnsafePath);
    }
    status(core, UpdatePhase::Checking, None, None)?;
    let staged = root.join(STAGED);
    let update = if inbox {
        karma_update::stage_directory(
            &root.parent().ok_or(UpdateError::UnsafePath)?.join(INBOX),
            &staged,
            installed(),
            &RELEASE_PUBLIC_KEY,
        )?
    } else {
        let releases = karma_update::fetch_releases(installed())?;
        let Some(asset) = karma_update::select_release(&releases, installed())? else {
            status(core, UpdatePhase::UpToDate, None, None)?;
            return Ok(false);
        };
        if !apply {
            status(
                core,
                UpdatePhase::Idle,
                Some(asset.version.to_string()),
                None,
            )?;
            return Ok(false);
        }
        status(
            core,
            UpdatePhase::Downloading,
            Some(asset.version.to_string()),
            None,
        )?;
        let download = root.join("release.zip");
        reject_links(&download)?;
        karma_update::download(&asset, &download, installed())?;
        let update = karma_update::stage_zip(&download, &staged, installed(), &RELEASE_PUBLIC_KEY)?;
        fs::remove_file(download)?;
        if update.manifest.version() != asset.version {
            return Err(UpdateError::InvalidManifest);
        }
        update
    };
    for name in [
        "KarmaService.exe",
        "KarmaGuardian.exe",
        "KarmaControl.exe",
        "karma-agent-windows.exe",
        "karma-ui.exe",
    ] {
        if !update.manifest.files.contains_key(name) {
            return Err(UpdateError::MissingFile(name.into()));
        }
    }
    if shutdown.load(Ordering::Acquire) {
        return Err(UpdateError::UnsafePath);
    }
    tree_acl(install, INSTALL_ACL)?;
    // Derive deletions only from a previously developer-signed installed
    // manifest. Never infer deletions from arbitrary files in the directory.
    let retired = transaction::retired_files(old_signed_files(install), &update.manifest);
    let plan = transaction::prepare(install, &staged, &update.manifest, PRODUCT_VERSION, retired)
        .inspect_err(|_| {
        let _ = transaction::cleanup_unjournaled(install);
    })?;
    // Rehash the same-volume copies as well, not just the original inbox.
    let verified = karma_update::verify_directory(
        &install.join(transaction::PREPARED),
        installed(),
        &RELEASE_PUBLIC_KEY,
    );
    if let Err(error) = verified {
        transaction::cleanup(&plan)?;
        return Err(error);
    }
    if let Err(error) = status(
        core,
        UpdatePhase::Installing,
        Some(plan.version.clone()),
        None,
    ) {
        transaction::cleanup(&plan)?;
        return Err(error);
    }
    if shutdown.load(Ordering::Acquire) {
        transaction::cleanup(&plan)?;
        return Err(UpdateError::UnsafePath);
    }
    transaction::write_plan(&root.join(JOURNAL), &plan, replace)?;
    if root.join(RESULT).exists() {
        fs::remove_file(root.join(RESULT))?;
    }
    if let Err(error) = launch(&root, false) {
        transaction::cleanup(&plan)?;
        fs::remove_file(root.join(JOURNAL))?;
        return Err(error);
    }
    Ok(true)
}

fn old_signed_files(install: &Path) -> Vec<String> {
    let verified = (|| {
        let bytes = fs::read(install.join(MANIFEST_FILE)).ok()?;
        let signature = fs::read_to_string(install.join(SIGNATURE_FILE)).ok()?;
        karma_update::verify_manifest(&bytes, &signature, &RELEASE_PUBLIC_KEY).ok()
    })();
    verified
        .map(|manifest| manifest.files.into_keys().collect())
        .unwrap_or_default()
}

fn cleanup_completed(install: &Path) -> Result<bool, UpdateError> {
    let root = state()?;
    let Ok(_lock) = lock(&root) else {
        return Ok(false);
    };
    let journal = root.join(JOURNAL);
    if !journal.exists() {
        return Ok(false);
    }
    let plan = transaction::read_plan(&journal)?;
    if plan.phase != Phase::Committed || plan.version != PRODUCT_VERSION || plan.install != install
    {
        return Ok(false);
    }
    transaction::cleanup(&plan)?;
    let staged = root.join(STAGED);
    if staged.exists() {
        karma_update::remove_directory(&staged)?;
    }
    if root.join(WORKER).exists() {
        fs::remove_file(root.join(WORKER))?;
    }
    fs::remove_file(journal)?;
    Ok(true)
}

pub fn transaction_active() -> bool {
    !diagnostics_only() && state().is_ok_and(|root| root.join(JOURNAL).exists())
}

struct RestartServices;
impl Drop for RestartServices {
    fn drop(&mut self) {
        crate::scm::ensure_running("KarmaService");
        crate::scm::ensure_running("KarmaGuardian");
    }
}

/// Close every installed client executable before replacement or rollback.
/// Resources are fixed installed filenames, never caller-supplied PIDs or
/// names. The console hides to the tray on WM_CLOSE, so a graceful-only close
/// never succeeds; it holds no state (the service owns all data), so Restart
/// Manager force-closes it once a signed update is being applied.
/// 控制台收到关闭消息只会隐藏到托盘，仅请求正常关闭永远失败；控制台不持有
/// 数据，因此在应用已验签的升级时由 Restart Manager 强制关闭。
fn close_clients(install: &Path) -> Result<(), UpdateError> {
    struct Session(u32);
    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                let _ = RmEndSession(self.0);
            }
        }
    }
    let mut paths = Vec::new();
    for name in CLIENT_EXECUTABLES {
        let path = install.join(name);
        reject_links(&path)?;
        if path.is_file() {
            paths.push(wide(&path));
        }
    }
    if paths.is_empty() {
        return Ok(());
    }
    let mut handle = 0;
    let mut key = [0_u16; 33];
    unsafe { RmStartSession(&mut handle, None, PWSTR(key.as_mut_ptr())) }
        .ok()
        .map_err(|error| UpdateError::Io(winerror(error)))?;
    let session = Session(handle);
    let resources: Vec<_> = paths.iter().map(|path| PCWSTR(path.as_ptr())).collect();
    unsafe { RmRegisterResources(session.0, Some(&resources), None, None) }
        .ok()
        .map_err(|error| UpdateError::Io(winerror(error)))?;
    unsafe { RmShutdown(session.0, RmForceShutdown.0 as u32, None) }
        .ok()
        .map_err(|error| UpdateError::Io(winerror(error)))
}

/// Restore the predecessor image. A client relaunched by the user can lock a
/// file again, so close clients and retry with a delay instead of letting a
/// failed rollback restart the service into an immediate recovery loop.
/// 用户可能重新打开控制台而再次占用文件：关闭客户端并延时重试，避免回滚失败后
/// 服务立刻重启、反复进入恢复的死循环。
fn rollback_installed(plan: &transaction::Plan) -> Result<(), UpdateError> {
    let mut attempt = 1;
    loop {
        let _ = close_clients(&plan.install);
        match transaction::rollback(plan, replace) {
            Ok(()) => return Ok(()),
            Err(error) if attempt >= ROLLBACK_ATTEMPTS => return Err(error),
            Err(error) => {
                eprintln!("Karma update rollback attempt {attempt}: {error}");
                attempt += 1;
                thread::sleep(Duration::from_secs(5));
            }
        }
    }
}

/// Handles helper flags only as LocalSystem, using only the protected fixed
/// journal. A user cannot turn this mode into an arbitrary privileged launcher.
pub fn run_helper(recover: bool) -> Result<(), UpdateError> {
    let root = initialize()?;
    // Allow the launching host's workers to observe its shutdown flag first.
    thread::sleep(Duration::from_secs(2));
    let _lock = lock(&root)?;
    let journal = root.join(JOURNAL);
    let mut plan = transaction::read_plan(&journal)?;
    // A delayed recovery process may acquire the lock after the original
    // worker already committed. It must never undo a committed transaction.
    if recover && plan.phase == Phase::Committed {
        return Ok(());
    }
    let running_worker = env::current_exe()?;
    if running_worker != root.join(WORKER) {
        return Err(UpdateError::UnsafePath);
    }
    let _restart = RestartServices;
    // SCM names are constants. Never accept process IDs or caller-selected
    // services, and retain each service's ACL/recovery configuration.
    crate::scm_update::stop_and_wait("KarmaGuardian", Duration::from_secs(30))?;
    let result = (|| {
        crate::scm_update::stop_and_wait("KarmaService", Duration::from_secs(30))?;
        close_clients(&plan.install)?;
        if recover
            || plan.phase == Phase::Applying
                && !plan
                    .install
                    .join(transaction::PREPARED)
                    .join(MANIFEST_FILE)
                    .exists()
        {
            rollback_installed(&plan)?;
            transaction::cleanup(&plan)?;
            fs::remove_file(&journal)?;
            update_display_version(&plan.installed_version)?;
            return Ok(());
        }
        karma_update::verify_directory(
            &plan.install.join(transaction::PREPARED),
            plan.installed_version.parse()?,
            &RELEASE_PUBLIC_KEY,
        )?;
        transaction::apply(&plan, replace)?;
        plan.phase = Phase::Applied;
        transaction::write_plan(&journal, &plan, replace)?;
        tree_acl(&plan.install, INSTALL_ACL)?;
        crate::scm_update::start_and_wait("KarmaService", Duration::from_secs(45))?;
        crate::scm_update::start_and_wait("KarmaGuardian", Duration::from_secs(30))?;
        update_display_version(&plan.version)?;
        plan.phase = Phase::Committed;
        transaction::write_plan(&journal, &plan, replace)?;
        Ok::<(), UpdateError>(())
    })();
    if result.is_err() {
        let _ = crate::scm_update::stop_and_wait("KarmaGuardian", Duration::from_secs(30));
        let _ = crate::scm_update::stop_and_wait("KarmaService", Duration::from_secs(30));
        // Leave the journal intact if rollback fails. The next service startup
        // launches the same protected recovery worker and retries.
        plan.phase = Phase::Applying;
        transaction::write_plan(&journal, &plan, replace)?;
        rollback_installed(&plan)?;
        transaction::cleanup(&plan)?;
        fs::remove_file(&journal)?;
        let _ = update_display_version(&plan.installed_version);
    }
    write_completion(
        &root,
        &Completion {
            version: plan.version.clone(),
            error: result
                .as_ref()
                .err()
                .map(|error| error_code(error).into())
                .or_else(|| recover.then(|| "storage".into())),
        },
    )?;
    // Even a failed update restores both watchdogs. No service is uninstalled,
    // no SCM permissions/recovery actions are relaxed, and WFP stays persistent.
    crate::scm::ensure_running("KarmaService");
    crate::scm::ensure_running("KarmaGuardian");
    result
}

fn update_display_version(version: &str) -> Result<(), UpdateError> {
    let mut key = HKEY::default();
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Karma"),
            None,
            KEY_SET_VALUE | KEY_WOW64_64KEY,
            &mut key,
        )
    };
    // A ZIP-only installation may have no Add/Remove Programs entry. Update an
    // existing NSIS entry without manufacturing a partial uninstaller record.
    if opened == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    opened
        .ok()
        .map_err(|error| UpdateError::Io(winerror(error)))?;
    let bytes: Vec<u8> = version
        .encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    let result = unsafe { RegSetValueExW(key, w!("DisplayVersion"), None, REG_SZ, Some(&bytes)) }
        .ok()
        .map_err(|error| UpdateError::Io(winerror(error)));
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}
