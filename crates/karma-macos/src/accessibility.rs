//! Browser address bars through the Accessibility API, the macOS counterpart
//! of UI Automation on Windows.
//!
//! Reading another application's interface needs the Accessibility
//! permission (System Settings → Privacy & Security → Accessibility). Without
//! it no host is read, and title and image rules work as before.

use std::{collections::VecDeque, ptr::NonNull};

use karma_agent_core::address_bar::{
    ADDRESS_BAR_MINIMUM_SCORE, address_bar_score, host_from_address_bar_value,
};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFDictionary, CFRetained, CFString, CFType, kCFBooleanTrue,
};

/// Bounds how long an unresponsive browser can stall one attribute read.
const MESSAGING_TIMEOUT_SECONDS: f32 = 0.25;
/// Toolbars sit near the top of a window's element tree; page content, which
/// can hold thousands of elements, is never searched.
const MAXIMUM_DEPTH: usize = 12;
const MAXIMUM_ELEMENTS: usize = 400;
const AX_ERROR_SUCCESS: i32 = 0;
const TEXT_FIELD_ROLES: &[&str] = &["AXTextField", "AXComboBox"];
const WEB_CONTENT_ROLE: &str = "AXWebArea";

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    static kAXTrustedCheckOptionPrompt: Option<&'static CFString>;
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: Option<&CFDictionary>) -> u8;
    fn AXUIElementCreateApplication(pid: libc::pid_t) -> Option<NonNull<CFType>>;
    fn AXUIElementSetMessagingTimeout(element: &CFType, timeout_seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: &CFType,
        attribute: &CFString,
        value: *mut *mut CFType,
    ) -> i32;
}

/// Whether this process may read other applications' interfaces.
pub fn is_trusted() -> bool {
    // SAFETY: AXIsProcessTrusted has no preconditions.
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Asks macOS to show the Accessibility consent prompt, which also lists the
/// agent in System Settings for the user to allow. Returns whether access is
/// already allowed.
pub fn request_trust() -> bool {
    // SAFETY: both are constant objects provided by the system frameworks.
    let (Some(key), Some(value)) = (unsafe { kAXTrustedCheckOptionPrompt }, unsafe {
        kCFBooleanTrue
    }) else {
        return is_trusted();
    };
    let options = CFDictionary::<CFString, CFBoolean>::from_slices(&[key], &[value]);
    // SAFETY: `options` is a valid dictionary for the duration of the call.
    unsafe { AXIsProcessTrustedWithOptions(Some(options.as_opaque())) != 0 }
}

/// The host the address bar of `pid`'s focused window shows, or `None` when
/// it has no recognizable address bar, the address is not a web address, or
/// Accessibility is not allowed.
pub fn browser_host(pid: u32) -> Option<String> {
    if !is_trusted() {
        return None;
    }
    let pid = libc::pid_t::try_from(pid).ok().filter(|&pid| pid > 0)?;
    // SAFETY: AXUIElementCreateApplication returns a +1 reference or null.
    let application = unsafe { CFRetained::from_raw(AXUIElementCreateApplication(pid)?) };
    // SAFETY: `application` is a valid accessibility element.
    unsafe { AXUIElementSetMessagingTimeout(&application, MESSAGING_TIMEOUT_SECONDS) };
    let attributes = Attributes::new();
    let window = attribute(&application, &attributes.focused_window)?;
    let mut best: Option<(u8, String)> = None;
    let mut queue = VecDeque::from([(window, 0_usize)]);
    let mut visited = 0_usize;
    while let Some((element, depth)) = queue.pop_front() {
        visited += 1;
        if visited > MAXIMUM_ELEMENTS {
            break;
        }
        let role = string_attribute(&element, &attributes.role).unwrap_or_default();
        if role == WEB_CONTENT_ROLE {
            continue;
        }
        if TEXT_FIELD_ROLES.contains(&role.as_str()) {
            if let Some(candidate) = address_bar_candidate(&element, &attributes) {
                if best
                    .as_ref()
                    .is_none_or(|(best_score, _)| candidate.0 > *best_score)
                {
                    best = Some(candidate);
                }
            }
            continue;
        }
        if depth >= MAXIMUM_DEPTH {
            continue;
        }
        let Some(children) = attribute(&element, &attributes.children)
            .and_then(|children| children.downcast::<CFArray>().ok())
        else {
            continue;
        };
        // SAFETY: AXChildren is an array of accessibility elements.
        let children = unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(children) };
        for index in 0..children.len() {
            if let Some(child) = children.get(index) {
                queue.push_back((child, depth + 1));
            }
        }
    }
    best.and_then(|(score, host)| (score >= ADDRESS_BAR_MINIMUM_SCORE).then_some(host))
}

/// The score and host of a text field whose value is a web address.
fn address_bar_candidate(element: &CFType, attributes: &Attributes) -> Option<(u8, String)> {
    let value = string_attribute(element, &attributes.value)?;
    let host = host_from_address_bar_value(&value)?;
    // Chrome and Edge describe the omnibox, Firefox gives it a placeholder,
    // and Safari names it in its identifier.
    let name = [
        &attributes.description,
        &attributes.title,
        &attributes.placeholder,
    ]
    .iter()
    .filter_map(|name| string_attribute(element, name))
    .collect::<Vec<_>>()
    .join(" ");
    let identifier = string_attribute(element, &attributes.identifier).unwrap_or_default();
    Some((address_bar_score(&name, &identifier, &value), host))
}

struct Attributes {
    focused_window: CFRetained<CFString>,
    children: CFRetained<CFString>,
    role: CFRetained<CFString>,
    value: CFRetained<CFString>,
    description: CFRetained<CFString>,
    title: CFRetained<CFString>,
    placeholder: CFRetained<CFString>,
    identifier: CFRetained<CFString>,
}

impl Attributes {
    fn new() -> Self {
        Self {
            focused_window: CFString::from_static_str("AXFocusedWindow"),
            children: CFString::from_static_str("AXChildren"),
            role: CFString::from_static_str("AXRole"),
            value: CFString::from_static_str("AXValue"),
            description: CFString::from_static_str("AXDescription"),
            title: CFString::from_static_str("AXTitle"),
            placeholder: CFString::from_static_str("AXPlaceholderValue"),
            identifier: CFString::from_static_str("AXIdentifier"),
        }
    }
}

fn attribute(element: &CFType, name: &CFString) -> Option<CFRetained<CFType>> {
    let mut value = std::ptr::null_mut();
    // SAFETY: `element` is an accessibility element and `value` a valid out
    // pointer; on success it holds a +1 reference.
    let result = unsafe { AXUIElementCopyAttributeValue(element, name, &mut value) };
    if result != AX_ERROR_SUCCESS {
        return None;
    }
    // SAFETY: the copy rule transfers ownership of the returned value.
    NonNull::new(value).map(|value| unsafe { CFRetained::from_raw(value) })
}

fn string_attribute(element: &CFType, name: &CFString) -> Option<String> {
    attribute(element, name)?
        .downcast::<CFString>()
        .ok()
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processes_without_windows_have_no_browser_host() {
        assert_eq!(browser_host(0), None);
        assert_eq!(browser_host(std::process::id()), None);
    }
}
