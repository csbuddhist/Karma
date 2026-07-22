use std::collections::HashMap;

use karma_domain::MonitorId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameMetadata {
    pub monitor_id: MonitorId,
    pub captured_at_ms: i64,
    pub fingerprint: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameWork {
    pub run_image: bool,
    pub run_ocr: bool,
}

#[derive(Debug, Clone, Copy)]
struct MonitorSchedule {
    image_at_ms: i64,
    ocr_at_ms: i64,
    ocr_fingerprint: u64,
}

#[derive(Debug, Default)]
pub struct FrameScheduler {
    monitors: HashMap<MonitorId, MonitorSchedule>,
}

impl FrameScheduler {
    pub fn select(&mut self, frame: FrameMetadata) -> FrameWork {
        let Some(previous) = self.monitors.get_mut(&frame.monitor_id) else {
            self.monitors.insert(
                frame.monitor_id,
                MonitorSchedule {
                    image_at_ms: frame.captured_at_ms,
                    ocr_at_ms: frame.captured_at_ms,
                    ocr_fingerprint: frame.fingerprint,
                },
            );
            return FrameWork {
                run_image: true,
                run_ocr: true,
            };
        };

        let run_image = frame.captured_at_ms - previous.image_at_ms >= 500;
        let run_ocr = frame.captured_at_ms - previous.ocr_at_ms >= 1000
            && frame.fingerprint != previous.ocr_fingerprint;

        if run_image {
            previous.image_at_ms = frame.captured_at_ms;
        }
        if run_ocr {
            previous.ocr_at_ms = frame.captured_at_ms;
            previous.ocr_fingerprint = frame.fingerprint;
        }

        FrameWork { run_image, run_ocr }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use karma_domain::MonitorId;

    fn frame(monitor: &str, at: i64, fingerprint: u64) -> FrameMetadata {
        FrameMetadata {
            monitor_id: MonitorId(monitor.into()),
            captured_at_ms: at,
            fingerprint,
        }
    }

    #[test]
    fn first_frame_runs_image_and_ocr() {
        assert_eq!(
            FrameScheduler::default().select(frame("a", 1000, 1)),
            FrameWork {
                run_image: true,
                run_ocr: true,
            }
        );
    }

    #[test]
    fn image_is_capped_at_two_fps() {
        let mut value = FrameScheduler::default();
        value.select(frame("a", 1000, 1));
        assert!(!value.select(frame("a", 1499, 2)).run_image);
        assert!(value.select(frame("a", 1500, 3)).run_image);
    }

    #[test]
    fn ocr_requires_one_second_and_changed_frame() {
        let mut value = FrameScheduler::default();
        value.select(frame("a", 1000, 1));
        assert!(!value.select(frame("a", 2000, 1)).run_ocr);
        assert!(value.select(frame("a", 2001, 2)).run_ocr);
    }

    #[test]
    fn monitors_have_independent_state() {
        let mut value = FrameScheduler::default();
        value.select(frame("a", 1000, 1));
        assert_eq!(
            value.select(frame("b", 1100, 1)),
            FrameWork {
                run_image: true,
                run_ocr: true,
            }
        );
    }
}
