use std::{io, path::Path};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, QueryFullProcessImageNameW,
                TerminateProcess, WaitForSingleObject,
            },
        },
    },
    core::PWSTR,
};

const MAX_PROCESS_PATH_U16: usize = 32_768;
const EXIT_WAIT_MS: u32 = 5_000;

/// Terminates every process, in any session, whose image is exactly one of the
/// fixed `names` inside `install`. Restart Manager cannot close processes in
/// another session from the Service's session 0, so match full image paths.
/// Only fixed installed filenames are accepted, never caller-supplied PIDs.
/// 服务位于会话 0，Restart Manager 无法关闭其他会话中的进程，因此按完整映像路径
/// 匹配；只接受固定的安装文件名，绝不接受调用方提供的 PID。
pub fn terminate_installed(install: &Path, names: &[&str]) -> io::Result<()> {
    let mut failed = 0_u32;
    for_each_named_process(names, |process_id| {
        if terminate_if_installed(process_id, install, names).is_err() {
            failed += 1;
        }
        true
    })?;
    if failed == 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{failed} installed client process(es) could not be closed"
        )))
    }
}

/// Whether any process in any session runs one of the fixed installed `names`.
/// 是否有任一会话中的进程正在运行安装目录内的固定文件名之一。
pub fn any_installed_running(install: &Path, names: &[&str]) -> io::Result<bool> {
    let mut found = false;
    for_each_named_process(names, |process_id| {
        found = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
            .ok()
            .map(OwnedHandle)
            .and_then(|handle| image_path(&handle).ok())
            .is_some_and(|image| is_installed_image(&image, install, names));
        !found
    })?;
    Ok(found)
}

/// Visits other processes whose file name is one of `names` until `visit`
/// returns false.
fn for_each_named_process(names: &[&str], mut visit: impl FnMut(u32) -> bool) -> io::Result<()> {
    let snapshot = OwnedHandle(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.map_err(io::Error::other)?,
    );
    let own_pid = unsafe { GetCurrentProcessId() };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) }.is_ok();
    while more {
        let file_name = utf16_until_nul(&entry.szExeFile);
        if entry.th32ProcessID != own_pid
            && names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&file_name))
            && !visit(entry.th32ProcessID)
        {
            break;
        }
        more = unsafe { Process32NextW(snapshot.0, &mut entry) }.is_ok();
    }
    Ok(())
}

fn image_path(handle: &OwnedHandle) -> io::Result<String> {
    let mut path = vec![0_u16; MAX_PROCESS_PATH_U16];
    let mut path_len = path.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            handle.0,
            Default::default(),
            PWSTR(path.as_mut_ptr()),
            &mut path_len,
        )
    }
    .map_err(io::Error::other)?;
    path.truncate(path_len as usize);
    Ok(String::from_utf16_lossy(&path))
}

fn terminate_if_installed(process_id: u32, install: &Path, names: &[&str]) -> io::Result<()> {
    let handle = match unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE,
            false,
            process_id,
        )
    } {
        Ok(handle) => OwnedHandle(handle),
        // The process may have exited since the snapshot was taken.
        Err(_) if !process_exists(process_id) => return Ok(()),
        Err(error) => return Err(io::Error::other(error)),
    };
    if !is_installed_image(&image_path(&handle)?, install, names) {
        return Ok(());
    }
    unsafe { TerminateProcess(handle.0, 1) }.map_err(io::Error::other)?;
    unsafe { WaitForSingleObject(handle.0, EXIT_WAIT_MS) };
    Ok(())
}

fn process_exists(process_id: u32) -> bool {
    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map(|handle| drop(OwnedHandle(handle)))
        .is_ok()
}

fn is_installed_image(image: &str, install: &Path, names: &[&str]) -> bool {
    names.iter().any(|name| {
        install
            .join(name)
            .to_str()
            .is_some_and(|expected| expected.eq_ignore_ascii_case(image))
    })
}

fn utf16_until_nul(value: &[u16]) -> String {
    let end = value
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fixed_names_inside_install_match() {
        let install = Path::new(r"C:\Program Files\Karma");
        let names = ["karma-ui.exe"];
        assert!(is_installed_image(
            r"c:\program files\karma\KARMA-UI.exe",
            install,
            &names
        ));
        assert!(!is_installed_image(
            r"C:\Users\child\karma-ui.exe",
            install,
            &names
        ));
        assert!(!is_installed_image(
            r"C:\Program Files\Karma\KarmaService.exe",
            install,
            &names
        ));
    }
}
