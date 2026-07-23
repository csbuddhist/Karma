use std::fmt;

pub trait MonitorInventory {
    type Monitor;
    type Error;

    fn active_monitors(&self) -> Result<Vec<Self::Monitor>, Self::Error>;
}

pub trait CaptureTargetFactory<M> {
    type Error;

    fn create(&self, monitor: &M) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupStatus {
    Ready,
    Degraded,
    Unavailable,
}

impl fmt::Display for StartupStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupSummary {
    pub status: StartupStatus,
    pub monitor_count: usize,
    pub wgc_ready_count: usize,
    pub wgc_failed_count: usize,
}

pub struct StartupProbe;

impl StartupProbe {
    pub fn run<I, F>(inventory: &I, factory: &F) -> StartupSummary
    where
        I: MonitorInventory,
        F: CaptureTargetFactory<I::Monitor>,
    {
        let Ok(monitors) = inventory.active_monitors() else {
            return StartupSummary {
                status: StartupStatus::Unavailable,
                monitor_count: 0,
                wgc_ready_count: 0,
                wgc_failed_count: 0,
            };
        };

        let monitor_count = monitors.len();
        let wgc_ready_count = monitors
            .iter()
            .filter(|monitor| factory.create(monitor).is_ok())
            .count();
        let wgc_failed_count = monitor_count.saturating_sub(wgc_ready_count);
        let status = if monitor_count == 0 || wgc_ready_count == 0 {
            StartupStatus::Unavailable
        } else if wgc_ready_count == monitor_count {
            StartupStatus::Ready
        } else {
            StartupStatus::Degraded
        };

        StartupSummary {
            status,
            monitor_count,
            wgc_ready_count,
            wgc_failed_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeInventory {
        monitors: Result<Vec<u8>, ()>,
    }

    impl MonitorInventory for FakeInventory {
        type Monitor = u8;
        type Error = ();

        fn active_monitors(&self) -> Result<Vec<Self::Monitor>, Self::Error> {
            self.monitors.clone()
        }
    }

    struct FakeCaptureFactory {
        failing_monitor: Option<u8>,
    }

    impl CaptureTargetFactory<u8> for FakeCaptureFactory {
        type Error = ();

        fn create(&self, monitor: &u8) -> Result<(), Self::Error> {
            if self.failing_monitor == Some(*monitor) {
                Err(())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn all_capture_targets_ready() {
        let value = StartupProbe::run(
            &FakeInventory {
                monitors: Ok(vec![1, 2]),
            },
            &FakeCaptureFactory {
                failing_monitor: None,
            },
        );

        assert_eq!(
            value,
            StartupSummary {
                status: StartupStatus::Ready,
                monitor_count: 2,
                wgc_ready_count: 2,
                wgc_failed_count: 0,
            }
        );
    }

    #[test]
    fn partial_failure_is_degraded() {
        let value = StartupProbe::run(
            &FakeInventory {
                monitors: Ok(vec![1, 2, 3]),
            },
            &FakeCaptureFactory {
                failing_monitor: Some(2),
            },
        );

        assert_eq!(value.status, StartupStatus::Degraded);
        assert_eq!(value.wgc_ready_count, 2);
        assert_eq!(value.wgc_failed_count, 1);
    }

    #[test]
    fn zero_monitors_and_inventory_failure_are_unavailable() {
        let factory = FakeCaptureFactory {
            failing_monitor: None,
        };
        assert_eq!(
            StartupProbe::run(
                &FakeInventory {
                    monitors: Ok(vec![]),
                },
                &factory,
            )
            .status,
            StartupStatus::Unavailable
        );
        assert_eq!(
            StartupProbe::run(&FakeInventory { monitors: Err(()) }, &factory,).status,
            StartupStatus::Unavailable
        );
    }
}
