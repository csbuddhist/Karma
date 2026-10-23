//! Brings the session agent back when the console user's agent stops
//! reporting, the counterpart of the Windows agent watchdog.
//!
//! launchd already restarts an agent that exits (`KeepAlive`). A user can
//! still unload the LaunchAgent from their own launchd domain, so the root
//! service bootstraps it into the console user's domain again and kicks it.
//! Attempts back off exponentially so a broken agent cannot cause a storm.

use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI64, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use karma_macos::session;

use crate::host::unix_time_ms;

pub const AGENT_LABEL: &str = "com.karma.agent";
const LAUNCHCTL: &str = "/bin/launchctl";
const POLL_INTERVAL: Duration = Duration::from_secs(5);
/// Heartbeats arrive about every 10 seconds.
const SILENCE_LIMIT_MS: i64 = 30_000;
const FIRST_RETRY_MS: i64 = 30_000;
const MAXIMUM_RETRY_MS: i64 = 5 * 60_000;

/// When the watchdog should act, decided from plain values so it can be
/// tested without launchd.
#[derive(Debug, Default)]
struct Recovery {
    console_user: Option<u32>,
    watched_since_ms: i64,
    next_attempt_ms: i64,
    retry_ms: i64,
}

impl Recovery {
    /// Returns the uid whose agent must be restarted now, if any.
    fn poll(
        &mut self,
        console_user: Option<u32>,
        last_heartbeat_ms: i64,
        now_ms: i64,
    ) -> Option<u32> {
        if console_user != self.console_user {
            // A new session gets the full silence limit to start its agent.
            *self = Self {
                console_user,
                watched_since_ms: now_ms,
                next_attempt_ms: 0,
                retry_ms: FIRST_RETRY_MS,
            };
        }
        let uid = console_user?;
        let reference = last_heartbeat_ms.max(self.watched_since_ms);
        if now_ms.saturating_sub(reference) < SILENCE_LIMIT_MS {
            self.retry_ms = FIRST_RETRY_MS;
            return None;
        }
        if now_ms < self.next_attempt_ms {
            return None;
        }
        self.next_attempt_ms = now_ms.saturating_add(self.retry_ms);
        self.retry_ms = self.retry_ms.saturating_mul(2).min(MAXIMUM_RETRY_MS);
        Some(uid)
    }
}

pub fn start(
    launch_agent_plist: PathBuf,
    last_heartbeat_ms: Arc<AtomicI64>,
    shutdown: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("karma-agent-watchdog".into())
        .spawn(move || {
            let mut recovery = Recovery::default();
            while !shutdown.load(Ordering::Acquire) {
                if let Some(uid) = recovery.poll(
                    session::console_user(),
                    last_heartbeat_ms.load(Ordering::Acquire),
                    unix_time_ms(),
                ) {
                    restart_agent(uid, &launch_agent_plist);
                }
                thread::sleep(POLL_INTERVAL);
            }
        })
        .expect("KarmaService failed to start its agent watchdog")
}

fn restart_agent(uid: u32, plist: &std::path::Path) {
    let domain = format!("gui/{uid}");
    // Fails harmlessly when the agent is still loaded.
    let _ = launchctl(&["bootstrap", &domain, &plist.to_string_lossy()]);
    let restarted = launchctl(&["kickstart", "-k", &format!("{domain}/{AGENT_LABEL}")]);
    eprintln!(
        "status={} component=agent_watchdog action=restart",
        if restarted { "recovering" } else { "degraded" }
    );
}

fn launchctl(arguments: &[&str]) -> bool {
    Command::new(LAUNCHCTL)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_silent_agent_is_restarted_with_exponential_backoff() {
        let mut recovery = Recovery::default();
        assert_eq!(recovery.poll(Some(501), 0, 1_000), None);
        assert_eq!(recovery.poll(Some(501), 0, 31_000), Some(501));
        assert_eq!(recovery.poll(Some(501), 0, 40_000), None);
        assert_eq!(recovery.poll(Some(501), 0, 61_000), Some(501));
        assert_eq!(recovery.poll(Some(501), 0, 100_000), None);
        assert_eq!(recovery.poll(Some(501), 0, 121_000), Some(501));
    }

    #[test]
    fn heartbeats_and_session_changes_reset_the_watchdog() {
        let mut recovery = Recovery::default();
        assert_eq!(recovery.poll(Some(501), 0, 0), None);
        assert_eq!(recovery.poll(Some(501), 0, 31_000), Some(501));
        assert_eq!(recovery.poll(Some(501), 50_000, 60_000), None);
        assert_eq!(recovery.retry_ms, FIRST_RETRY_MS);

        assert_eq!(recovery.poll(Some(502), 50_000, 70_000), None);
        assert_eq!(recovery.poll(Some(502), 50_000, 99_000), None);
        assert_eq!(recovery.poll(Some(502), 50_000, 100_000), Some(502));
        assert_eq!(recovery.poll(None, 50_000, 200_000), None);
    }
}
