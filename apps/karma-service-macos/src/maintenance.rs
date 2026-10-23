//! Periodic service work that has no macOS enforcement backend yet.
//!
//! The network schedule is still evaluated every minute, so its audit trail,
//! early restores, and uninstall waiting periods behave as on Windows. Until
//! the Network Extension filter exists, an active block is recorded as an
//! enforcement failure instead of being reported as applied. Update requests
//! are answered with a failure for the same reason, so the console never
//! waits on a check that will not run.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use karma_ipc::{UpdatePhase, UpdateStatus};
use karma_service_core::ServiceCore;

use crate::host::unix_time_ms;

const TICK: Duration = Duration::from_secs(1);
const SCHEDULE_INTERVAL: Duration = Duration::from_secs(60);
pub const UNSUPPORTED_UPDATE_ERROR: &str = "unsupported_platform";

pub fn start(core: Arc<ServiceCore>, shutdown: Arc<AtomicBool>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("karma-maintenance".into())
        .spawn(move || {
            let mut next_schedule = Instant::now();
            while !shutdown.load(Ordering::Acquire) {
                if Instant::now() >= next_schedule {
                    evaluate_schedule(&core);
                    next_schedule = Instant::now() + SCHEDULE_INTERVAL;
                }
                answer_update_requests(&core);
                thread::sleep(TICK);
            }
        })
        .expect("KarmaService failed to start its maintenance worker")
}

fn evaluate_schedule(core: &ServiceCore) {
    let now_ms = unix_time_ms();
    match core.network_enforcement(now_ms) {
        Ok(enforcement) => {
            let enforced = !enforcement.status.restricted;
            if core.record_network_enforcement(enforced, now_ms).is_err() {
                eprintln!("status=degraded component=network_schedule error=storage_unavailable");
            }
        }
        Err(_) => eprintln!("status=degraded component=network_schedule error=core_unavailable"),
    }
}

fn answer_update_requests(core: &ServiceCore) {
    if !matches!(core.take_update_action(), Ok(Some(_))) {
        return;
    }
    let now_ms = unix_time_ms();
    let status = UpdateStatus {
        phase: UpdatePhase::Failed,
        last_checked_at_ms: Some(now_ms),
        available_version: None,
        last_error: Some(UNSUPPORTED_UPDATE_ERROR.into()),
    };
    if core.record_update_status(status, now_ms).is_err() {
        eprintln!("status=degraded component=updater error=storage_unavailable");
    }
}
