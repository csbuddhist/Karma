//! Programs installed on this computer, read from the Windows uninstall and
//! App Paths registrations and from Start menu and desktop shortcuts, so an
//! administrator can pick application rules instead of typing executable
//! paths. Only the console reads this; the Service still validates every
//! rule it is sent.

// Only Windows lists anything; elsewhere the helpers serve the unit tests.
#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::HashSet;

use serde::Serialize;

/// Most applications one listing returns.
pub const MAX_INSTALLED_APPLICATIONS: usize = 1000;
/// Longest executable path a rule may name; matches the agent's limit.
const MAX_EXECUTABLE_CHARS: usize = karma_ipc::MAX_EXECUTABLE_CHARS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApplication {
    pub name: String,
    pub executable: String,
    pub publisher: Option<String>,
    /// A known browser: allowing it exempts every page it shows.
    pub browser: bool,
}

/// The executable a `DisplayIcon` value names, such as `"C:\App\app.exe",0`.
pub fn icon_executable(display_icon: &str) -> Option<String> {
    let value = display_icon.trim();
    let path = match value.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next()?,
        None => value.rsplit_once(',').map_or(value, |(path, index)| {
            if index.trim().trim_start_matches('-').parse::<i32>().is_ok() {
                path
            } else {
                value
            }
        }),
    };
    let path = path.trim();
    (path.to_lowercase().ends_with(".exe") && !is_uninstaller(path)).then(|| path.to_owned())
}

/// Whether the executable only removes or sets up its program.
pub fn is_uninstaller(path: &str) -> bool {
    let stem = file_stem(path).to_lowercase();
    stem.starts_with("unins") || stem.contains("uninstall") || stem == "setup" || stem == "update"
}

/// The program executable inside an install folder: the only one there, or
/// the one whose name appears in the application's display name.
pub fn pick_install_executable(display_name: &str, executables: &[String]) -> Option<String> {
    let candidates: Vec<&String> = executables
        .iter()
        .filter(|path| !is_uninstaller(path))
        .collect();
    if let [only] = candidates.as_slice() {
        return Some((*only).clone());
    }
    let name = comparable(display_name);
    candidates
        .into_iter()
        .find(|path| {
            let stem = comparable(file_stem(path));
            stem.len() >= 3 && name.contains(&stem)
        })
        .cloned()
}

/// The file name without its extension, splitting Windows paths on any host.
fn file_stem(path: &str) -> &str {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

fn is_known_browser(executable: &str) -> bool {
    let file_name = executable.rsplit(['\\', '/']).next().unwrap_or(executable);
    karma_policy::KNOWN_BROWSER_EXECUTABLES
        .iter()
        .any(|browser| file_name.eq_ignore_ascii_case(browser))
}

fn comparable(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Replaces `%NAME%` references with environment values; unknown names stay.
pub fn expand_environment(value: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut expanded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        expanded.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => match lookup(&after[..end]) {
                Some(replacement) => {
                    expanded.push_str(&replacement);
                    rest = &after[end + 1..];
                }
                None => {
                    expanded.push('%');
                    rest = after;
                }
            },
            _ => {
                expanded.push('%');
                rest = after;
            }
        }
    }
    expanded.push_str(rest);
    expanded
}

/// Keeps the first entry for each executable (case-insensitively), drops
/// entries a rule could not hold, and sorts by name.
pub fn collect(
    entries: impl IntoIterator<Item = InstalledApplication>,
) -> Vec<InstalledApplication> {
    let mut seen = HashSet::new();
    let mut applications: Vec<_> = entries
        .into_iter()
        .filter(|entry| {
            let name = entry.name.trim();
            !name.is_empty()
                && !entry.executable.chars().any(char::is_control)
                && entry.executable.chars().count() <= MAX_EXECUTABLE_CHARS
                && seen.insert(entry.executable.to_lowercase())
        })
        .map(|entry| InstalledApplication {
            name: entry.name.trim().to_owned(),
            browser: is_known_browser(&entry.executable),
            ..entry
        })
        .collect();
    applications.sort_by_cached_key(|entry| (entry.name.to_lowercase(), entry.executable.clone()));
    applications.truncate(MAX_INSTALLED_APPLICATIONS);
    applications
}

#[cfg(windows)]
pub fn list() -> Vec<InstalledApplication> {
    use std::path::Path;

    use winreg::{
        RegKey,
        enums::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        },
    };

    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const APP_PATHS: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths";

    let expand = |value: String| expand_environment(&value, |name| std::env::var(name).ok());
    let existing = |path: String| Path::new(&path).is_file().then_some(path);
    let views = [
        (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_64KEY),
        (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_32KEY),
        (HKEY_CURRENT_USER, KEY_READ),
    ];
    let mut entries = Vec::new();
    for (root, flags) in views {
        let Ok(uninstall) = RegKey::predef(root).open_subkey_with_flags(UNINSTALL, flags) else {
            continue;
        };
        for key_name in uninstall.enum_keys().flatten() {
            let Ok(key) = uninstall.open_subkey_with_flags(&key_name, flags) else {
                continue;
            };
            let Ok(name) = key.get_value::<String, _>("DisplayName") else {
                continue;
            };
            // Updates, components, and children of another entry are not
            // programs an administrator starts.
            if key.get_value::<u32, _>("SystemComponent").ok() == Some(1)
                || key.get_value::<String, _>("ParentKeyName").is_ok()
                || key.get_value::<String, _>("ReleaseType").is_ok()
            {
                continue;
            }
            let from_icon = key
                .get_value::<String, _>("DisplayIcon")
                .ok()
                .and_then(|icon| icon_executable(&expand(icon)))
                .and_then(existing);
            let executable = from_icon.or_else(|| {
                let location = expand(key.get_value::<String, _>("InstallLocation").ok()?);
                let executables: Vec<String> = std::fs::read_dir(location.trim().trim_matches('"'))
                    .ok()?
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.is_file()
                            && path
                                .extension()
                                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
                    })
                    .filter_map(|path| path.to_str().map(str::to_owned))
                    .collect();
                pick_install_executable(&name, &executables)
            });
            if let Some(executable) = executable {
                entries.push(InstalledApplication {
                    name,
                    executable,
                    publisher: key.get_value::<String, _>("Publisher").ok(),
                    browser: false,
                });
            }
        }
    }
    for (root, flags) in views {
        let Ok(app_paths) = RegKey::predef(root).open_subkey_with_flags(APP_PATHS, flags) else {
            continue;
        };
        for key_name in app_paths.enum_keys().flatten() {
            let Some(executable) = app_paths
                .open_subkey_with_flags(&key_name, flags)
                .and_then(|key| key.get_value::<String, _>(""))
                .ok()
                .and_then(|path| icon_executable(&expand(path)))
                .and_then(existing)
            else {
                continue;
            };
            let name = Path::new(&key_name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(&key_name)
                .to_owned();
            entries.push(InstalledApplication {
                name,
                executable,
                publisher: None,
                browser: false,
            });
        }
    }
    // Shortcuts come last: registrations name a program better, and shortcuts
    // add the ones registered without a usable executable.
    for (name, target) in shortcuts::targets() {
        if let Some(executable) = icon_executable(&target).and_then(existing) {
            entries.push(InstalledApplication {
                name,
                executable,
                publisher: None,
                browser: false,
            });
        }
    }
    collect(entries)
}

/// Start menu (all users and this user) and desktop shortcut targets.
#[cfg(windows)]
mod shortcuts {
    use std::{
        fs,
        os::windows::ffi::OsStrExt,
        path::{Path, PathBuf},
    };

    use windows::{
        Win32::{
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize, IPersistFile, STGM_READ,
            },
            UI::Shell::{IShellLinkW, ShellLink},
        },
        core::{Interface, PCWSTR},
    };

    const MAX_SHORTCUTS: usize = 4096;
    const MAX_DEPTH: usize = 6;

    /// `(shortcut name, target path)` pairs; nothing is launched or resolved
    /// over the network.
    pub(super) fn targets() -> Vec<(String, String)> {
        let mut files = Vec::new();
        for (variable, suffix, depth) in [
            (
                "ProgramData",
                r"Microsoft\Windows\Start Menu\Programs",
                MAX_DEPTH,
            ),
            (
                "APPDATA",
                r"Microsoft\Windows\Start Menu\Programs",
                MAX_DEPTH,
            ),
            ("PUBLIC", "Desktop", 0),
            ("USERPROFILE", "Desktop", 0),
        ] {
            if let Some(base) = std::env::var_os(variable) {
                find(&PathBuf::from(base).join(suffix), depth, &mut files);
            }
        }
        if files.is_empty() {
            return Vec::new();
        }
        let mut targets = Vec::new();
        // SAFETY: balanced with CoUninitialize below on this blocking worker thread.
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        // SAFETY: ShellLink is the in-process shell link class implementing IShellLinkW.
        let link: windows::core::Result<IShellLinkW> =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) };
        if let Ok(link) = link {
            if let Ok(persist) = link.cast::<IPersistFile>() {
                for file in files {
                    if let Some(target) = target(&link, &persist, &file) {
                        let name = file
                            .file_stem()
                            .map(|stem| stem.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        targets.push((name, target));
                    }
                }
            }
        }
        if initialized {
            // SAFETY: pairs the successful CoInitializeEx above.
            unsafe { CoUninitialize() };
        }
        targets
    }

    fn find(directory: &Path, depth: usize, files: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            if files.len() >= MAX_SHORTCUTS {
                return;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() && depth > 0 {
                find(&path, depth - 1, files);
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
            {
                files.push(path);
            }
        }
    }

    fn target(link: &IShellLinkW, persist: &IPersistFile, file: &Path) -> Option<String> {
        let path: Vec<u16> = file.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: `path` is NUL-terminated and outlives the call.
        unsafe { persist.Load(PCWSTR(path.as_ptr()), STGM_READ) }.ok()?;
        let mut target = [0_u16; 2048];
        // SAFETY: GetPath writes at most `target.len()` UTF-16 units; no find data is requested.
        unsafe { link.GetPath(&mut target, std::ptr::null_mut(), 0) }.ok()?;
        let length = target.iter().position(|unit| *unit == 0)?;
        (length > 0).then(|| String::from_utf16_lossy(&target[..length]))
    }
}

#[cfg(not(windows))]
pub fn list() -> Vec<InstalledApplication> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, executable: &str) -> InstalledApplication {
        InstalledApplication {
            name: name.into(),
            executable: executable.into(),
            publisher: None,
            browser: false,
        }
    }

    #[test]
    fn display_icon_names_the_program_executable() {
        assert_eq!(
            icon_executable(r#""C:\Program Files\App\app.exe",0"#).as_deref(),
            Some(r"C:\Program Files\App\app.exe")
        );
        assert_eq!(
            icon_executable(r"C:\Games\Game.EXE,-101").as_deref(),
            Some(r"C:\Games\Game.EXE")
        );
        assert_eq!(
            icon_executable(r"C:\Apps\player.exe").as_deref(),
            Some(r"C:\Apps\player.exe")
        );
        assert_eq!(icon_executable(r"C:\Apps\app.ico"), None);
        assert_eq!(icon_executable(r"C:\Apps\unins000.exe,0"), None);
        assert_eq!(icon_executable(r"C:\Apps\Uninstall App.exe"), None);
    }

    #[test]
    fn install_folder_executable_prefers_the_only_or_named_program() {
        let only = [
            r"C:\App\unins000.exe".to_owned(),
            r"C:\App\tool.exe".to_owned(),
        ];
        assert_eq!(
            pick_install_executable("Anything", &only).as_deref(),
            Some(r"C:\App\tool.exe")
        );
        let several = [
            r"C:\Steam\steamerrorreporter.exe".to_owned(),
            r"C:\Steam\steam.exe".to_owned(),
        ];
        assert_eq!(
            pick_install_executable("Steam", &several).as_deref(),
            Some(r"C:\Steam\steam.exe")
        );
        assert_eq!(pick_install_executable("Other", &several), None);
    }

    #[test]
    fn environment_references_are_expanded() {
        let lookup = |name: &str| (name == "ProgramFiles").then(|| r"C:\Program Files".to_owned());
        assert_eq!(
            expand_environment(r"%ProgramFiles%\App\app.exe", lookup),
            r"C:\Program Files\App\app.exe"
        );
        assert_eq!(
            expand_environment("100% %Unknown% %", lookup),
            "100% %Unknown% %"
        );
    }

    #[test]
    fn listing_is_deduplicated_bounded_and_sorted() {
        let applications = collect([
            app(" zeta ", r"C:\Z\z.exe"),
            app("Alpha", r"C:\A\a.exe"),
            app("alpha copy", r"c:\a\A.EXE"),
            app("", r"C:\B\b.exe"),
            app(
                "Long",
                &format!(r"C:\{}.exe", "x".repeat(MAX_EXECUTABLE_CHARS)),
            ),
        ]);
        assert_eq!(
            applications,
            [app("Alpha", r"C:\A\a.exe"), app("zeta", r"C:\Z\z.exe")]
        );
        let browsers = collect([
            app("Chrome", r"C:\Google\Chrome\Application\chrome.exe"),
            app("Reader", r"C:\Reader\reader.exe"),
        ]);
        assert!(browsers[0].browser);
        assert!(!browsers[1].browser);
        let many = collect(
            (0..MAX_INSTALLED_APPLICATIONS + 5)
                .map(|index| app(&format!("App {index:04}"), &format!(r"C:\Apps\{index}.exe"))),
        );
        assert_eq!(many.len(), MAX_INSTALLED_APPLICATIONS);
    }
}
