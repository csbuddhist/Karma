use std::{ffi::c_void, marker::PhantomData, rc::Rc};

use karma_domain::MonitorId;
use thiserror::Error;
use windows::{
    Graphics::Capture::GraphicsCaptureItem,
    Win32::{
        Foundation::{LPARAM, RECT},
        Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR},
        System::WinRT::{
            Graphics::Capture::IGraphicsCaptureItemInterop, RO_INIT_MULTITHREADED, RoInitialize,
            RoUninitialize,
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId},
    },
    core,
};

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
        id: MonitorId(format!("hmonitor:{:x}", handle.0 as usize)),
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
}
