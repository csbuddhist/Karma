use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

use karma_ai::{FrameWork, ImageClassifier, PreparedFrame};
use karma_onnx::InferenceHealth;

const CONSECUTIVE_FAILURE_LIMIT: u8 = 3;

#[derive(Clone, Default)]
pub struct InferenceHealthHandle {
    inner: Arc<Mutex<InferenceHealthState>>,
}

#[derive(Default)]
struct InferenceHealthState {
    metrics: InferenceHealth,
    consecutive_failures: u8,
    unavailable: bool,
}

impl InferenceHealthHandle {
    pub fn snapshot(&self) -> InferenceHealth {
        lock_health(&self.inner).metrics
    }

    pub fn is_available(&self) -> bool {
        !lock_health(&self.inner).unavailable
    }
}

fn lock_health(health: &Mutex<InferenceHealthState>) -> MutexGuard<'_, InferenceHealthState> {
    health
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct ScheduledImageConsumer<C> {
    classifier: C,
    health: InferenceHealthHandle,
}

impl<C> ScheduledImageConsumer<C>
where
    C: ImageClassifier,
{
    pub fn new(classifier: C) -> Self {
        Self {
            classifier,
            health: InferenceHealthHandle::default(),
        }
    }

    pub fn health_handle(&self) -> InferenceHealthHandle {
        self.health.clone()
    }

    #[cfg(test)]
    pub fn health(&self) -> InferenceHealth {
        self.health.snapshot()
    }

    pub fn consume(&mut self, frame: PreparedFrame, work: FrameWork) {
        if !work.run_image {
            return;
        }
        let started = Instant::now();
        let result = self.classifier.classify(&frame);
        let mut state = lock_health(&self.health.inner);
        if result.is_ok() {
            let micros = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
            state.metrics.record_success(micros);
            state.consecutive_failures = 0;
            state.unavailable = false;
        } else {
            state.metrics.record_failure();
            state.consecutive_failures = state.consecutive_failures.saturating_add(1);
            state.unavailable = state.consecutive_failures >= CONSECUTIVE_FAILURE_LIMIT;
        }
    }

    #[cfg(test)]
    fn classifier(&self) -> &C {
        &self.classifier
    }
}

#[cfg(windows)]
impl<C> karma_windows::PreparedFrameConsumer for ScheduledImageConsumer<C>
where
    C: ImageClassifier + Send + 'static,
{
    fn consume(&mut self, frame: PreparedFrame, work: FrameWork) {
        ScheduledImageConsumer::consume(self, frame, work);
    }
}

#[cfg(test)]
mod tests {
    use karma_ai::{
        BgraFrame, FrameDimensions, FramePreparationConfig, FramePreparer, FrameWork,
        ImageClassifier, ImageInference,
    };
    use karma_domain::{MonitorId, RiskCategory};

    use super::ScheduledImageConsumer;

    struct FakeClassifier {
        calls: usize,
        fails: bool,
    }

    impl ImageClassifier for FakeClassifier {
        type Error = ();

        fn classify(
            &mut self,
            _frame: &karma_ai::PreparedFrame,
        ) -> Result<ImageInference, Self::Error> {
            self.calls += 1;
            if self.fails {
                Err(())
            } else {
                Ok(ImageInference {
                    score_millis: 750,
                    categories: vec![RiskCategory::Nudity],
                })
            }
        }
    }

    fn prepared_frame() -> karma_ai::PreparedFrame {
        let dimensions = FrameDimensions::new(1, 1).unwrap();
        let frame = BgraFrame::new(
            MonitorId("monitor-a".into()),
            1,
            dimensions,
            dimensions.tight_stride().unwrap(),
            vec![0, 0, 0, 255],
        )
        .unwrap();
        FramePreparer::new(FramePreparationConfig::default())
            .prepare(frame)
            .unwrap()
    }

    #[test]
    fn scheduled_image_work_controls_classification_and_health() {
        let mut consumer = ScheduledImageConsumer::new(FakeClassifier {
            calls: 0,
            fails: false,
        });
        let shared_health = consumer.health_handle();

        consumer.consume(
            prepared_frame(),
            FrameWork {
                run_image: false,
                run_ocr: true,
            },
        );
        assert_eq!(consumer.classifier().calls, 0);
        assert_eq!(consumer.health().inferences(), 0);

        consumer.consume(
            prepared_frame(),
            FrameWork {
                run_image: true,
                run_ocr: false,
            },
        );
        assert_eq!(consumer.classifier().calls, 1);
        assert_eq!(consumer.health().inferences(), 1);
        assert_eq!(shared_health.snapshot().inferences(), 1);
        assert_eq!(consumer.health().failures(), 0);
    }

    #[test]
    fn classifier_failure_is_counted_without_panicking() {
        let mut consumer = ScheduledImageConsumer::new(FakeClassifier {
            calls: 0,
            fails: true,
        });

        consumer.consume(
            prepared_frame(),
            FrameWork {
                run_image: true,
                run_ocr: false,
            },
        );

        assert_eq!(consumer.classifier().calls, 1);
        assert_eq!(consumer.health().inferences(), 0);
        assert_eq!(consumer.health().failures(), 1);
    }

    #[test]
    fn consecutive_failures_mark_ai_unavailable_until_success() {
        let mut consumer = ScheduledImageConsumer::new(FakeClassifier {
            calls: 0,
            fails: true,
        });
        let health = consumer.health_handle();

        for _ in 0..3 {
            consumer.consume(
                prepared_frame(),
                FrameWork {
                    run_image: true,
                    run_ocr: false,
                },
            );
        }
        assert!(!health.is_available());

        consumer.classifier.fails = false;
        consumer.consume(
            prepared_frame(),
            FrameWork {
                run_image: true,
                run_ocr: false,
            },
        );
        assert!(health.is_available());
    }
}
