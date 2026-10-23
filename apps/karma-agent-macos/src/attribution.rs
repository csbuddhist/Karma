//! Which application a display's frames and the foreground window belong to.
//!
//! The frontmost ordinary window names the foreground application. A frame is
//! attributed to that application's window covering most of the frame's
//! display, using the same rule as the Windows agent; when no such window
//! exists, the frame has no reliable source and nothing is closed.

use karma_agent_core::{
    attribution::{AttributionResult, Rect, SourceAttributor, WindowCandidate},
    service::{
        ForegroundContext, SourceContext, SourceIdentityProvider, random_opaque, unix_time_ms,
    },
};
use karma_ipc::{AgentContextObservation, ProcessIdentity};
use karma_macos::{
    display::{self, WindowSnapshot},
    process,
};

/// The foreground application's windows, frontmost first.
fn foreground_windows(windows: &[WindowSnapshot]) -> Option<(u32, Vec<&WindowSnapshot>)> {
    let own_pid = std::process::id();
    let foreground = windows.iter().find(|window| window.pid != own_pid)?;
    Some((
        foreground.pid,
        windows
            .iter()
            .filter(|window| window.pid == foreground.pid)
            .collect(),
    ))
}

/// The foreground window covering most of `display`, with its title.
pub fn attribute(display: Rect, windows: &[WindowSnapshot]) -> Option<(u32, isize, String)> {
    let (pid, candidates) = foreground_windows(windows)?;
    let candidates: Vec<WindowCandidate> = candidates
        .iter()
        .map(|window| WindowCandidate {
            handle: window.number as isize,
            pid: window.pid,
            bounds: window.bounds,
        })
        .collect();
    let AttributionResult::Reliable(attributed) =
        SourceAttributor::resolve(display, Some(pid), &candidates)
    else {
        return None;
    };
    let title = windows
        .iter()
        .find(|window| window.number as isize == attributed.handle)
        .map(|window| window.title.clone())
        .unwrap_or_default();
    Some((attributed.pid, attributed.handle, title))
}

fn identity(pid: u32) -> Option<ProcessIdentity> {
    let info = process::inspect(pid).ok().flatten()?;
    Some(ProcessIdentity {
        process_id: info.pid,
        started_at_ms: info.started_at_ms,
        executable_name: info.executable_path.to_str()?.to_owned(),
        executable_sha256: None,
    })
}

/// Attributes one display's frames to the foreground window covering it.
pub struct MacosSourceIdentityProvider {
    display_bounds: Rect,
    cached_process: Option<ProcessIdentity>,
}

impl MacosSourceIdentityProvider {
    pub fn new(display_bounds: Rect) -> Self {
        Self {
            display_bounds,
            cached_process: None,
        }
    }
}

impl SourceIdentityProvider for MacosSourceIdentityProvider {
    fn snapshot(&mut self) -> Option<SourceContext> {
        let windows = display::front_to_back_windows();
        let (pid, window_handle, window_title) = attribute(self.display_bounds, &windows)?;
        let cached = self
            .cached_process
            .as_ref()
            .filter(|process| process.process_id == pid)
            .cloned();
        let identity = match cached {
            Some(identity) => identity,
            None => {
                let identity = identity(pid)?;
                self.cached_process = Some(identity.clone());
                identity
            }
        };
        Some(SourceContext {
            identity,
            window_handle,
            window_title,
        })
    }

    /// Reading browser address bars needs the Accessibility permission and
    /// arrives in a later phase; title and image rules work without it.
    fn browser_host(&mut self, _source: &SourceContext) -> Option<String> {
        None
    }
}

/// The frontmost window for the foreground-context monitor.
pub struct MacosForegroundContext;

impl ForegroundContext for MacosForegroundContext {
    fn observe(&mut self, agent_instance_id: &str) -> Option<AgentContextObservation> {
        let windows = display::front_to_back_windows();
        let (pid, candidates) = foreground_windows(&windows)?;
        let title = candidates.first()?.title.clone();
        if title.is_empty() {
            return None;
        }
        Some(AgentContextObservation {
            event_id: format!("context-{}", random_opaque()),
            agent_instance_id: agent_instance_id.into(),
            occurred_at_ms: unix_time_ms(),
            source: identity(pid)?,
            window_title: title,
            browser_host: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(number: u32, pid: u32, bounds: Rect, title: &str) -> WindowSnapshot {
        WindowSnapshot {
            number,
            pid,
            bounds,
            title: title.into(),
        }
    }

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }

    const LEFT: Rect = Rect {
        left: 0,
        top: 0,
        right: 1000,
        bottom: 800,
    };
    const RIGHT: Rect = Rect {
        left: 1000,
        top: 0,
        right: 2000,
        bottom: 800,
    };

    #[test]
    fn frames_belong_to_the_foreground_window_covering_the_display() {
        let windows = [
            window(10, 42, rect(100, 100, 900, 700), "Video"),
            window(11, 42, rect(1100, 0, 1300, 200), "Small"),
            window(20, 7, rect(1000, 0, 2000, 800), "Other app"),
        ];
        assert_eq!(attribute(LEFT, &windows), Some((42, 10, "Video".into())));
        assert_eq!(attribute(RIGHT, &windows), Some((42, 11, "Small".into())));
    }

    #[test]
    fn a_display_without_a_foreground_window_has_no_reliable_source() {
        let windows = [
            window(10, 42, rect(100, 100, 900, 700), "Editor"),
            window(20, 7, rect(1000, 0, 2000, 800), "Background video"),
        ];
        assert_eq!(attribute(RIGHT, &windows), None);
        assert_eq!(attribute(LEFT, &[]), None);
    }

    #[test]
    fn equally_large_foreground_windows_are_not_guessed_between() {
        let windows = [
            window(10, 42, rect(0, 0, 500, 800), "A"),
            window(11, 42, rect(500, 0, 1000, 800), "B"),
        ];
        assert_eq!(attribute(LEFT, &windows), None);
    }
}
