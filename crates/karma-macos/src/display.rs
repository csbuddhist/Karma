//! Active displays and on-screen windows from CoreGraphics.
//!
//! Display bounds and window bounds share CoreGraphics' global coordinate
//! space (points, origin at the top-left of the main display), so a window's
//! overlap with a display is a plain rectangle intersection.

use karma_agent_core::attribution::Rect;
use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFString, CFType, CGRect};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayBounds, CGError, CGGetActiveDisplayList,
    CGPreflightScreenCaptureAccess, CGRectMakeWithDictionaryRepresentation,
    CGRequestScreenCaptureAccess, CGWindowListCopyWindowInfo, CGWindowListOption, kCGNullWindowID,
    kCGWindowAlpha, kCGWindowBounds, kCGWindowLayer, kCGWindowName, kCGWindowNumber,
    kCGWindowOwnerPID,
};

use crate::CaptureError;

/// The Windows agent supports at most 16 monitors in its heartbeat as well.
const MAXIMUM_DISPLAYS: u32 = 16;
/// Ordinary application windows; menus, the Dock, and overlays use others.
const NORMAL_WINDOW_LAYER: i64 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplaySnapshot {
    pub id: CGDirectDisplayID,
    /// Global bounds in points.
    pub bounds: Rect,
}

impl DisplaySnapshot {
    /// A stable identifier for the protocol, unique among active displays.
    pub fn monitor_id(&self) -> String {
        format!("display-{}", self.id)
    }
}

/// Every display that is drawing, in CoreGraphics order (main display first).
pub fn active_displays() -> Result<Vec<DisplaySnapshot>, CaptureError> {
    let mut identifiers = [0 as CGDirectDisplayID; MAXIMUM_DISPLAYS as usize];
    let mut count = 0_u32;
    // SAFETY: both pointers are valid for the sizes passed.
    let result =
        unsafe { CGGetActiveDisplayList(MAXIMUM_DISPLAYS, identifiers.as_mut_ptr(), &mut count) };
    if result != CGError::Success {
        return Err(CaptureError::DisplaysUnavailable);
    }
    Ok(identifiers[..count.min(MAXIMUM_DISPLAYS) as usize]
        .iter()
        .map(|&id| DisplaySnapshot {
            id,
            bounds: rect(CGDisplayBounds(id)),
        })
        .collect())
}

/// One on-screen application window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowSnapshot {
    pub number: u32,
    pub pid: u32,
    pub bounds: Rect,
    /// Empty when the window has no title or Screen Recording is not allowed.
    pub title: String,
}

/// Visible ordinary windows, frontmost first.
pub fn front_to_back_windows() -> Vec<WindowSnapshot> {
    let Some(windows) = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        kCGNullWindowID,
    ) else {
        return Vec::new();
    };
    // SAFETY: CGWindowListCopyWindowInfo returns an array of dictionaries
    // keyed by strings.
    let windows = unsafe { windows.cast_unchecked::<CFDictionary<CFString, CFType>>() };
    (0..windows.len())
        .filter_map(|index| windows.get(index))
        .filter_map(|window| window_snapshot(&window))
        .collect()
}

fn window_snapshot(window: &CFDictionary<CFString, CFType>) -> Option<WindowSnapshot> {
    // SAFETY: the keys are CoreGraphics' own constant strings.
    let (layer_key, alpha_key, number_key, pid_key, bounds_key, name_key) = unsafe {
        (
            kCGWindowLayer,
            kCGWindowAlpha,
            kCGWindowNumber,
            kCGWindowOwnerPID,
            kCGWindowBounds,
            kCGWindowName,
        )
    };
    if number(window, layer_key)?.as_i64()? != NORMAL_WINDOW_LAYER {
        return None;
    }
    if number(window, alpha_key)
        .and_then(|alpha| alpha.as_f64())
        .is_some_and(|alpha| alpha <= 0.0)
    {
        return None;
    }
    let bounds_value = window.get(bounds_key)?;
    let bounds_dictionary = bounds_value.downcast_ref::<CFDictionary>()?;
    let mut bounds = CGRect::default();
    // SAFETY: `bounds` is a valid out pointer and the dictionary is a
    // CoreGraphics rectangle representation.
    if !unsafe { CGRectMakeWithDictionaryRepresentation(Some(bounds_dictionary), &mut bounds) } {
        return None;
    }
    let bounds = rect(bounds);
    if bounds.right <= bounds.left || bounds.bottom <= bounds.top {
        return None;
    }
    Some(WindowSnapshot {
        number: u32::try_from(number(window, number_key)?.as_i64()?).ok()?,
        pid: u32::try_from(number(window, pid_key)?.as_i64()?).ok()?,
        bounds,
        title: window
            .get(name_key)
            .and_then(|name| name.downcast_ref::<CFString>().map(ToString::to_string))
            .unwrap_or_default(),
    })
}

fn number(window: &CFDictionary<CFString, CFType>, key: &CFString) -> Option<CFRetained<CFNumber>> {
    window.get(key)?.downcast::<CFNumber>().ok()
}

fn rect(value: CGRect) -> Rect {
    let clamp = |coordinate: f64| coordinate.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32;
    Rect {
        left: clamp(value.origin.x),
        top: clamp(value.origin.y),
        right: clamp(value.origin.x + value.size.width),
        bottom: clamp(value.origin.y + value.size.height),
    }
}

/// Whether this process may capture the screen, without prompting.
pub fn screen_capture_allowed() -> bool {
    CGPreflightScreenCaptureAccess()
}

/// Asks macOS to show the Screen Recording consent prompt once; the user
/// grants access in System Settings. Returns whether access is already
/// allowed.
pub fn request_screen_capture() -> bool {
    CGRequestScreenCaptureAccess()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_are_enumerated_with_non_empty_bounds() {
        let displays = active_displays().unwrap();
        // Headless build machines have no display; the call must still work.
        for display in displays {
            assert!(display.bounds.right > display.bounds.left);
            assert!(display.bounds.bottom > display.bounds.top);
            assert_eq!(display.monitor_id(), format!("display-{}", display.id));
        }
    }

    #[test]
    fn listed_windows_have_owners_and_area() {
        for window in front_to_back_windows() {
            assert_ne!(window.pid, 0);
            assert!(window.bounds.right > window.bounds.left);
        }
    }

    #[test]
    fn coordinates_round_to_the_nearest_point() {
        let value = CGRect {
            origin: objc2_core_foundation::CGPoint { x: -0.4, y: 10.6 },
            size: objc2_core_foundation::CGSize {
                width: 100.5,
                height: 20.0,
            },
        };
        assert_eq!(
            rect(value),
            Rect {
                left: 0,
                top: 11,
                right: 100,
                bottom: 31
            }
        );
    }
}
