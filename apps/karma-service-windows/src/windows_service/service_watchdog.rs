//! KarmaService's mirror of KarmaGuardian: it restarts the guardian service when
//! that companion is found stopped, so the two services recover each other.
//!
//! This watchdog runs only while KarmaService itself is running. An authorized
//! shutdown (for example a password-authorized uninstall) sets the shutdown flag,
//! which stops this thread, so the mirror never fights a legitimate removal. The
//! uninstall script additionally removes KarmaGuardian before stopping
//! KarmaService.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const GUARDIAN_SERVICE: &str = "KarmaGuardian";
/// Fast backstop interval, matching the guardian's own watch: the service ACL
/// blocks a clean stop, so this mainly re-starts the guardian after an End-task
/// kill once SCM's restart budget is spent.
const CHECK_INTERVAL: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_secs(1);

pub fn start(shutdown: Arc<AtomicBool>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("karma-guardian-watchdog".into())
        .spawn(move || run(&shutdown))
        .expect("KarmaService failed to start its guardian watchdog")
}

/// Check the guardian once on entry, then every `CHECK_INTERVAL`, waking promptly
/// when `shutdown` is set.
fn run(shutdown: &AtomicBool) {
    let mut elapsed = CHECK_INTERVAL;
    while !shutdown.load(Ordering::Acquire) {
        if elapsed >= CHECK_INTERVAL {
            crate::scm::ensure_running(GUARDIAN_SERVICE);
            elapsed = Duration::ZERO;
        }
        thread::sleep(TICK);
        elapsed = elapsed.saturating_add(TICK);
    }
}
