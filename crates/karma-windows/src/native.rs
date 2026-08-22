use std::{ffi::c_void, marker::PhantomData, rc::Rc};

use karma_domain::MonitorId;
use thiserror::Error;
use windows::{
    Graphics::Capture::GraphicsCaptureItem,
    Win32::{
        Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, FILETIME, GetLastError, HANDLE, LPARAM, RECT,
        },
        Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR},
        System::Threading::{
            GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
        System::WinRT::{
            Graphics::Capture::IGraphicsCaptureItemInterop, RO_INIT_MULTITHREADED, RoInitialize,
            RoUninitialize,
        },
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            Variant::VARIANT,
        },
        UI::{
            Accessibility::{
                CUIAutomation, IUIAutomation, IUIAutomationValuePattern, TreeScope_Descendants,
                UIA_ControlTypePropertyId, UIA_EditControlTypeId, UIA_ValuePatternId,
            },
            WindowsAndMessaging::{
                GetForegroundWindow, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
                GetWindowThreadProcessId,
            },
        },
    },
    core::{self, PWSTR},
};

use crate::MappedFrameError;
use crate::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorHandle(pub isize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorSnapshot {
    pub id: MonitorId,
    pub handle: MonitorHandle,
    pub bounds: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForegroundWindowSnapshot {
    pub handle: isize,
    pub pid: u32,
    pub bounds: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSnapshot {
    pub process_id: u32,
    pub started_at_ms: i64,
    pub executable_name: String,
}

const WINDOWS_TO_UNIX_EPOCH_MS: u64 = 11_644_473_600_000;
const MAX_PROCESS_PATH_U16: usize = 32_768;
const MAX_WINDOW_TITLE_U16: usize = 512;
const MAX_UIA_EDIT_CONTROLS: i32 = 128;

#[derive(Debug, Error)]
pub enum WindowsAdapterError {
    #[error("Windows API failed during {operation}")]
    WindowsApi {
        operation: &'static str,
        #[source]
        source: core::Error,
    },
    #[error("capture target returned a non-positive size")]
    InvalidCaptureSize,
    #[error("capture frame returned a negative relative timestamp")]
    InvalidCaptureTimestamp,
    #[error("capture texture is smaller than its content")]
    CaptureTextureTooSmall,
    #[error("capture texture has unsupported DXGI format {actual}")]
    UnsupportedCaptureFormat { actual: i32 },
    #[error("mapped frame layout is invalid")]
    MappedFrame(#[source] MappedFrameError),
    #[error("staging source texture does not match the requested frame")]
    StagingSourceMismatch,
    #[error("prepared frame data is invalid")]
    FrameData(#[source] karma_ai::FrameError),
    #[error("process image path is invalid")]
    InvalidProcessPath,
    #[error("process start time is invalid")]
    InvalidProcessStartTime,
}

impl WindowsAdapterError {
    pub(crate) fn api(operation: &'static str, source: core::Error) -> Self {
        Self::WindowsApi { operation, source }
    }
}

pub struct WindowsRuntimeApartment {
    _thread_bound: PhantomData<Rc<()>>,
}

impl WindowsRuntimeApartment {
    pub fn initialize_mta() -> Result<Self, WindowsAdapterError> {
        // SAFETY: the Agent initializes WinRT once on its startup thread and the
        // returned guard balances the successful call with RoUninitialize.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
            .map_err(|source| WindowsAdapterError::api("RoInitialize", source))?;
        Ok(Self {
            _thread_bound: PhantomData,
        })
    }
}

impl Drop for WindowsRuntimeApartment {
    fn drop(&mut self) {
        // SAFETY: this guard is created only after a successful RoInitialize on
        // the same thread and is neither Send nor moved across the startup path.
        unsafe { RoUninitialize() };
    }
}

fn rect_from_native(value: RECT) -> Rect {
    Rect {
        left: value.left,
        top: value.top,
        right: value.right,
        bottom: value.bottom,
    }
}

fn monitor_id_from_handle(handle: MonitorHandle) -> MonitorId {
    MonitorId(format!("hmonitor-{:x}", handle.0 as usize))
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _device_context: HDC,
    bounds: *mut RECT,
    state: LPARAM,
) -> core::BOOL {
    if bounds.is_null() || state.0 == 0 {
        return false.into();
    }

    // SAFETY: `enumerate_active_monitors` passes a live, exclusive Vec pointer for
    // the synchronous duration of EnumDisplayMonitors. Windows invokes this
    // callback before that function returns and does not retain the pointer.
    let monitors = unsafe { &mut *(state.0 as *mut Vec<MonitorSnapshot>) };
    // SAFETY: Windows guarantees a valid RECT pointer for a monitor callback;
    // null was rejected above and RECT is Copy.
    let native_bounds = unsafe { *bounds };
    let handle = MonitorHandle(monitor.0 as isize);
    monitors.push(MonitorSnapshot {
        id: monitor_id_from_handle(handle),
        handle,
        bounds: rect_from_native(native_bounds),
    });
    true.into()
}

pub fn enumerate_active_monitors() -> Result<Vec<MonitorSnapshot>, WindowsAdapterError> {
    let mut monitors = Vec::new();
    let state = LPARAM((&mut monitors as *mut Vec<MonitorSnapshot>) as isize);

    // SAFETY: the callback and state pointer obey the synchronous lifetime
    // contract documented in `collect_monitor`; no HDC or clipping RECT is used.
    let succeeded = unsafe { EnumDisplayMonitors(None, None, Some(collect_monitor), state) };
    if !succeeded.as_bool() {
        return Err(WindowsAdapterError::api(
            "EnumDisplayMonitors",
            core::Error::from_thread(),
        ));
    }
    Ok(monitors)
}

pub fn foreground_window() -> Result<Option<ForegroundWindowSnapshot>, WindowsAdapterError> {
    // SAFETY: GetForegroundWindow has no input pointers or ownership transfer.
    let handle = unsafe { GetForegroundWindow() };
    if handle.is_invalid() {
        return Ok(None);
    }

    let mut bounds = RECT::default();
    // SAFETY: `bounds` is a valid writable RECT for the duration of the call.
    unsafe { GetWindowRect(handle, &mut bounds) }
        .map_err(|source| WindowsAdapterError::api("GetWindowRect", source))?;

    let mut pid = 0;
    // SAFETY: `pid` is a valid writable u32 and `handle` came from Windows.
    let thread_id = unsafe { GetWindowThreadProcessId(handle, Some(&mut pid)) };
    if thread_id == 0 || pid == 0 {
        return Err(WindowsAdapterError::api(
            "GetWindowThreadProcessId",
            core::Error::from_thread(),
        ));
    }

    Ok(Some(ForegroundWindowSnapshot {
        handle: handle.0 as isize,
        pid,
        bounds: rect_from_native(bounds),
    }))
}

pub fn window_title(handle: isize) -> String {
    let handle = windows::Win32::Foundation::HWND(handle as *mut c_void);
    let length = unsafe { GetWindowTextLengthW(handle) }
        .max(0)
        .min(MAX_WINDOW_TITLE_U16.saturating_sub(1) as i32) as usize;
    if length == 0 {
        return String::new();
    }
    let mut buffer = vec![0_u16; length.saturating_add(1)];
    let copied = unsafe { GetWindowTextW(handle, &mut buffer) }.max(0) as usize;
    buffer.truncate(copied.min(length));
    String::from_utf16_lossy(&buffer)
}

pub fn browser_host(handle: isize) -> Option<String> {
    let automation: IUIAutomation = unsafe {
        CoCreateInstance(
            &CUIAutomation,
            None::<&windows::core::IUnknown>,
            CLSCTX_INPROC_SERVER,
        )
    }
    .ok()?;
    let root = unsafe {
        automation.ElementFromHandle(windows::Win32::Foundation::HWND(handle as *mut c_void))
    }
    .ok()?;
    let control_type = VARIANT::from(UIA_EditControlTypeId.0);
    let condition =
        unsafe { automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &control_type) }
            .ok()?;
    let elements = unsafe { root.FindAll(TreeScope_Descendants, &condition) }.ok()?;
    let count = unsafe { elements.Length() }
        .ok()?
        .clamp(0, MAX_UIA_EDIT_CONTROLS);
    let mut best: Option<(u8, String)> = None;
    for index in 0..count {
        let Some(element) = (unsafe { elements.GetElement(index) }).ok() else {
            continue;
        };
        if unsafe { element.CurrentIsPassword() }
            .ok()
            .is_some_and(|value| value.as_bool())
        {
            continue;
        }
        let Some(value) = (unsafe {
            element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        })
        .ok()
        .and_then(|pattern| unsafe { pattern.CurrentValue() }.ok())
        .map(|value| value.to_string()) else {
            continue;
        };
        let Some(host) = host_from_address_bar_value(&value) else {
            continue;
        };
        let name = unsafe { element.CurrentName() }
            .map(|value| value.to_string())
            .unwrap_or_default();
        let automation_id = unsafe { element.CurrentAutomationId() }
            .map(|value| value.to_string())
            .unwrap_or_default();
        let score = address_bar_score(&name, &automation_id, &value);
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            best = Some((score, host));
        }
    }
    best.and_then(|(score, host)| (score >= 4).then_some(host))
}

fn address_bar_score(name: &str, automation_id: &str, value: &str) -> u8 {
    let name = name.to_lowercase();
    let automation_id = automation_id.to_lowercase();
    let mut score = u8::from(value.starts_with("http://") || value.starts_with("https://"));
    if [
        "address",
        "地址",
        "アドレス",
        "адрес",
        "网址",
        "網址",
        "omnibox",
        "url",
    ]
    .iter()
    .any(|hint| name.contains(hint))
    {
        score = score.saturating_add(4);
    }
    if ["address", "location", "omnibox", "url"]
        .iter()
        .any(|hint| automation_id.contains(hint))
    {
        score = score.saturating_add(5);
    }
    score
}

fn host_from_address_bar_value(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return None;
    }
    let url = url::Url::parse(value)
        .or_else(|_| url::Url::parse(&format!("https://{value}")))
        .ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    url.host_str().map(str::to_owned)
}

pub fn inspect_process(process_id: u32) -> Result<Option<ProcessSnapshot>, WindowsAdapterError> {
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
    {
        Ok(handle) => OwnedHandle(handle),
        Err(_) if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER => return Ok(None),
        Err(source) => return Err(WindowsAdapterError::api("OpenProcess", source)),
    };
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) }
        .map_err(|source| WindowsAdapterError::api("GetProcessTimes", source))?;
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
    .map_err(|source| WindowsAdapterError::api("QueryFullProcessImageNameW", source))?;
    path.truncate(path_len as usize);
    let executable_name =
        String::from_utf16(&path).map_err(|_| WindowsAdapterError::InvalidProcessPath)?;
    Ok(Some(ProcessSnapshot {
        process_id,
        started_at_ms: filetime_to_unix_ms(creation)?,
        executable_name,
    }))
}

fn filetime_to_unix_ms(value: FILETIME) -> Result<i64, WindowsAdapterError> {
    let ticks = (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime);
    let unix_ms = (ticks / 10_000)
        .checked_sub(WINDOWS_TO_UNIX_EPOCH_MS)
        .ok_or(WindowsAdapterError::InvalidProcessStartTime)?;
    i64::try_from(unix_ms).map_err(|_| WindowsAdapterError::InvalidProcessStartTime)
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub struct WgcCaptureTarget {
    item: GraphicsCaptureItem,
}

impl WgcCaptureTarget {
    pub fn for_monitor(handle: MonitorHandle) -> Result<Self, WindowsAdapterError> {
        let factory = core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
            .map_err(|source| WindowsAdapterError::api("GraphicsCaptureItem factory", source))?;
        let monitor = HMONITOR(handle.0 as *mut c_void);
        // SAFETY: the HMONITOR came from EnumDisplayMonitors and the requested
        // WinRT type matches the interop method's supported GraphicsCaptureItem.
        let item = unsafe { factory.CreateForMonitor::<GraphicsCaptureItem>(monitor) }
            .map_err(|source| WindowsAdapterError::api("CreateForMonitor", source))?;
        Ok(Self { item })
    }

    pub fn size(&self) -> Result<(u32, u32), WindowsAdapterError> {
        let size = self
            .item
            .Size()
            .map_err(|source| WindowsAdapterError::api("GraphicsCaptureItem.Size", source))?;
        if size.Width <= 0 || size.Height <= 0 {
            return Err(WindowsAdapterError::InvalidCaptureSize);
        }
        Ok((size.Width as u32, size.Height as u32))
    }

    pub(crate) fn capture_item(&self) -> &GraphicsCaptureItem {
        &self.item
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_handle_preserves_native_value() {
        assert_eq!(MonitorHandle(42).0, 42);
    }

    #[test]
    fn monitor_identifier_uses_protocol_safe_characters() {
        let id = monitor_id_from_handle(MonitorHandle(0x1a2b));

        assert_eq!(id.0, "hmonitor-1a2b");
        assert!(
            id.0.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        );
    }
}
