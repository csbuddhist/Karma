//! Reopens the installed console in the signed-in user's session after an
//! update finished. The update worker force-closes the console to replace its
//! image; only the fixed installed `karma-ui.exe` is ever started here, never
//! a caller-supplied path, command line, or session.
//! 升级完成后在已登录用户的会话中重新打开控制台。升级时为替换文件会强制关闭控制台；
//! 这里只启动安装目录中固定的 `karma-ui.exe`，绝不接受调用方提供的路径、命令行或会话。

use std::{ffi::c_void, io, os::windows::ffi::OsStrExt, path::Path, ptr};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock},
            RemoteDesktop::{WTSGetActiveConsoleSessionId, WTSQueryUserToken},
            Threading::{
                CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, PROCESS_INFORMATION, STARTUPINFOW,
            },
        },
    },
    core::{PCWSTR, PWSTR},
};

const CONSOLE_EXE: &str = "karma-ui.exe";

/// Starts the console unless it already runs or nobody is signed in at the
/// physical console.
pub fn relaunch(install: &Path) -> io::Result<()> {
    if super::installed_processes::any_installed_running(install, &[CONSOLE_EXE])? {
        return Ok(());
    }
    let session_id = unsafe { WTSGetActiveConsoleSessionId() };
    if session_id == u32::MAX {
        return Ok(());
    }
    let mut user_token = HANDLE::default();
    if unsafe { WTSQueryUserToken(session_id, &mut user_token) }.is_err() {
        // No interactive user: the console starts at the next sign-in.
        return Ok(());
    }
    let user_token = OwnedHandle(user_token);
    let console = install.join(CONSOLE_EXE);
    if !console.is_file() {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }
    // WebView2 needs the user's own profile variables (LOCALAPPDATA etc.).
    let environment = UserEnvironment::new(&user_token)?;
    let application = wide(console.as_os_str());
    let current_directory = wide(install.as_os_str());
    let mut desktop: Vec<u16> = "winsta0\\default".encode_utf16().chain(Some(0)).collect();
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        lpDesktop: PWSTR(desktop.as_mut_ptr()),
        ..Default::default()
    };
    let mut information = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessAsUserW(
            Some(user_token.0),
            PCWSTR(application.as_ptr()),
            None,
            None,
            None,
            false,
            CREATE_UNICODE_ENVIRONMENT,
            Some(environment.0),
            PCWSTR(current_directory.as_ptr()),
            &startup,
            &mut information,
        )
    }
    .map_err(io::Error::other)?;
    unsafe {
        let _ = CloseHandle(information.hThread);
        let _ = CloseHandle(information.hProcess);
    }
    Ok(())
}

fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct UserEnvironment(*mut c_void);

impl UserEnvironment {
    fn new(token: &OwnedHandle) -> io::Result<Self> {
        let mut block = ptr::null_mut();
        unsafe { CreateEnvironmentBlock(&mut block, Some(token.0), false) }
            .map_err(io::Error::other)?;
        Ok(Self(block))
    }
}

impl Drop for UserEnvironment {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyEnvironmentBlock(self.0);
        }
    }
}
