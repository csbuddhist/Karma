//! Process identity and signalling through libproc and kernel audit tokens.

use std::{
    ffi::OsString,
    mem::{MaybeUninit, size_of},
    os::unix::ffi::OsStringExt,
    path::PathBuf,
};

use karma_macos_ipc::AuditToken;

use crate::ProcessError;

/// `PROC_PIDPATHINFO_MAXSIZE` from `<sys/proc_info.h>`.
const MAX_PATH_BYTES: usize = 4096;
/// `TASK_AUDIT_TOKEN` from `<mach/task_info.h>`.
const TASK_AUDIT_TOKEN: u32 = 15;
const AUDIT_TOKEN_WORDS: u32 = (size_of::<AuditToken>() / size_of::<u32>()) as u32;
const KERN_SUCCESS: i32 = 0;

type MachPort = u32;

unsafe extern "C" {
    static mach_task_self_: MachPort;
    fn task_name_for_pid(target: MachPort, pid: libc::pid_t, name: *mut MachPort) -> i32;
    fn task_info(task: MachPort, flavor: u32, info: *mut i32, count: *mut u32) -> i32;
    fn mach_port_deallocate(task: MachPort, name: MachPort) -> i32;
    fn proc_pidpath_audittoken(token: *mut AuditToken, buffer: *mut libc::c_void, size: u32)
    -> i32;
    fn proc_signal_with_audittoken(token: *mut AuditToken, signal: libc::c_int) -> i32;
}

/// What libproc reports about one running process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    /// Start time in Unix milliseconds; together with `pid` it names one
    /// process instance.
    pub started_at_ms: i64,
    pub uid: u32,
    pub executable_path: PathBuf,
}

/// Reads the identity of `pid`, or `None` when no such process exists.
pub fn inspect(pid: u32) -> Result<Option<ProcessInfo>, ProcessError> {
    let pid = libc::pid_t::try_from(pid).map_err(|_| ProcessError::InvalidProcess)?;
    if pid <= 0 {
        return Err(ProcessError::InvalidProcess);
    }
    let Some(info) = bsd_info(pid)? else {
        return Ok(None);
    };
    let Some(executable_path) = executable_path(pid)? else {
        return Ok(None);
    };
    Ok(Some(ProcessInfo {
        pid: pid as u32,
        started_at_ms: started_at_ms(info.pbi_start_tvsec, info.pbi_start_tvusec)?,
        uid: info.pbi_uid,
        executable_path,
    }))
}

fn bsd_info(pid: libc::pid_t) -> Result<Option<libc::proc_bsdinfo>, ProcessError> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: `info` is writable for `size` bytes and outlives the call.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if written == size {
        // SAFETY: the kernel filled the whole zero-initialized structure.
        return Ok(Some(unsafe { info.assume_init() }));
    }
    if process_exists(pid) {
        Err(ProcessError::AccessDenied)
    } else {
        Ok(None)
    }
}

fn executable_path(pid: libc::pid_t) -> Result<Option<PathBuf>, ProcessError> {
    let mut buffer = vec![0_u8; MAX_PATH_BYTES];
    // SAFETY: `buffer` is writable for its whole length.
    let length =
        unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    if length > 0 {
        buffer.truncate(length as usize);
        return Ok(Some(PathBuf::from(OsString::from_vec(buffer))));
    }
    if process_exists(pid) {
        Err(ProcessError::AccessDenied)
    } else {
        Ok(None)
    }
}

fn process_exists(pid: libc::pid_t) -> bool {
    // SAFETY: signal 0 performs only the existence and permission check.
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn started_at_ms(seconds: u64, microseconds: u64) -> Result<i64, ProcessError> {
    seconds
        .checked_mul(1000)
        .and_then(|milliseconds| milliseconds.checked_add(microseconds / 1000))
        .and_then(|milliseconds| i64::try_from(milliseconds).ok())
        .ok_or(ProcessError::InvalidProcess)
}

/// The audit token of the process instance currently running as `pid`.
/// Requires root for processes of other users.
pub fn audit_token(pid: u32) -> Result<AuditToken, ProcessError> {
    let pid = libc::pid_t::try_from(pid).map_err(|_| ProcessError::InvalidProcess)?;
    let mut name: MachPort = 0;
    // SAFETY: `name` is a valid out pointer; the task self port is always valid.
    let result = unsafe { task_name_for_pid(mach_task_self_, pid, &mut name) };
    if result != KERN_SUCCESS {
        return Err(ProcessError::AccessDenied);
    }
    let mut token = AuditToken([0; 8]);
    let mut count = AUDIT_TOKEN_WORDS;
    // SAFETY: `token` provides `count` writable 32-bit words.
    let result = unsafe {
        task_info(
            name,
            TASK_AUDIT_TOKEN,
            token.0.as_mut_ptr().cast(),
            &mut count,
        )
    };
    // SAFETY: `name` is a send right this function received above.
    unsafe {
        mach_port_deallocate(mach_task_self_, name);
    }
    if result != KERN_SUCCESS || count != AUDIT_TOKEN_WORDS || token.pid() != pid {
        return Err(ProcessError::AccessDenied);
    }
    Ok(token)
}

/// The executable of exactly the process instance `token` names, or `None`
/// once that instance has exited or replaced its image with `exec`.
pub fn executable_path_for(token: &AuditToken) -> Option<PathBuf> {
    let mut token = *token;
    let mut buffer = vec![0_u8; MAX_PATH_BYTES];
    // SAFETY: `token` and `buffer` are valid for the duration of the call.
    let length = unsafe {
        proc_pidpath_audittoken(&mut token, buffer.as_mut_ptr().cast(), buffer.len() as u32)
    };
    (length > 0).then(|| {
        buffer.truncate(length as usize);
        PathBuf::from(OsString::from_vec(buffer))
    })
}

/// Sends `signal` to the process instance `token` names. The kernel refuses
/// when that instance no longer exists, even if its pid has been reused.
pub fn signal(token: &AuditToken, signal: libc::c_int) -> Result<(), ProcessError> {
    let mut token = *token;
    // SAFETY: `token` is valid for the duration of the call.
    match unsafe { proc_signal_with_audittoken(&mut token, signal) } {
        0 => Ok(()),
        libc::ESRCH => Err(ProcessError::NotRunning),
        _ => Err(ProcessError::AccessDenied),
    }
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};

    use super::*;

    #[test]
    fn the_current_process_is_inspected_with_its_own_identity() {
        let pid = std::process::id();
        let info = inspect(pid).unwrap().unwrap();

        assert_eq!(info.pid, pid);
        // SAFETY: getuid has no preconditions.
        assert_eq!(info.uid, unsafe { libc::getuid() });
        assert_eq!(
            info.executable_path.canonicalize().unwrap(),
            std::env::current_exe().unwrap().canonicalize().unwrap()
        );
        assert!(info.started_at_ms > 1_600_000_000_000);
        assert_eq!(
            inspect(pid).unwrap().unwrap().started_at_ms,
            info.started_at_ms
        );
    }

    #[test]
    fn a_process_that_does_not_exist_is_reported_as_absent() {
        let mut child = Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();

        assert_eq!(inspect(pid).unwrap(), None);
        assert_eq!(inspect(0), Err(ProcessError::InvalidProcess));
    }

    #[test]
    fn signals_reach_only_the_process_instance_the_token_names() {
        let mut child = Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        let token = audit_token(child.id()).unwrap();
        assert_eq!(token.pid() as u32, child.id());
        assert_eq!(
            executable_path_for(&token).unwrap(),
            inspect(child.id()).unwrap().unwrap().executable_path
        );

        signal(&token, libc::SIGTERM).unwrap();
        let status = child.wait().unwrap();
        assert!(!status.success());
        assert_eq!(executable_path_for(&token), None);
        assert_eq!(signal(&token, libc::SIGTERM), Err(ProcessError::NotRunning));
    }
}
