use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

use karma_ai::{
    FrameWork, ImageClassifier, ImageInference, OcrEngine, OcrMatchSummary, PreparedFrame, WordPack,
};
use karma_onnx::InferenceHealth;

const CONSECUTIVE_FAILURE_LIMIT: u8 = 3;

pub fn should_capture_evidence(
    enabled: bool,
    threshold_millis: u16,
    score_millis: u16,
    captured_at_ms: i64,
    last_evidence_at_ms: i64,
    cooldown_ms: i64,
) -> bool {
    enabled
        && score_millis >= threshold_millis
        && captured_at_ms.saturating_sub(last_evidence_at_ms) >= cooldown_ms
}

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

fn record_result<E>(health: &InferenceHealthHandle, started: Instant, result: Result<(), E>) {
    let mut state = lock_health(&health.inner);
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

/// Receives an OCR classification summary at the in-memory observation boundary.
///
/// Implementations must not retain sensitive OCR-derived data beyond their intended purpose.
pub trait OcrSummarySink {
    fn consume(&mut self, summary: OcrMatchSummary);

    fn consume_image(&mut self, _frame: &PreparedFrame, _inference: &ImageInference) {}
}

/// The default runtime sink deliberately retains only a delivery count.
#[derive(Default)]
pub struct CountingOcrSummarySink {
    summaries: u64,
}

impl CountingOcrSummarySink {
    #[cfg(test)]
    pub fn summaries(&self) -> u64 {
        self.summaries
    }
}

impl OcrSummarySink for CountingOcrSummarySink {
    fn consume(&mut self, _summary: OcrMatchSummary) {
        self.summaries = self.summaries.saturating_add(1);
    }
}

/// Runs the image and OCR engines independently against one prepared frame.
pub struct ScheduledInferenceConsumer<I, O, S> {
    image_classifier: I,
    ocr_engine: O,
    word_pack: WordPack,
    sink: S,
    image_health: InferenceHealthHandle,
    ocr_health: InferenceHealthHandle,
}

impl<I, O, S> ScheduledInferenceConsumer<I, O, S>
where
    I: ImageClassifier,
    O: OcrEngine,
    S: OcrSummarySink,
{
    pub fn new(image_classifier: I, ocr_engine: O, word_pack: WordPack, sink: S) -> Self {
        Self {
            image_classifier,
            ocr_engine,
            word_pack,
            sink,
            image_health: InferenceHealthHandle::default(),
            ocr_health: InferenceHealthHandle::default(),
        }
    }

    pub fn image_health_handle(&self) -> InferenceHealthHandle {
        self.image_health.clone()
    }

    pub fn ocr_health_handle(&self) -> InferenceHealthHandle {
        self.ocr_health.clone()
    }

    /// Marks an OCR engine that could not be initialized as unavailable without fabricating an
    /// inference failure. A later successful OCR classification restores availability.
    pub fn mark_ocr_unavailable(&mut self) {
        let mut state = lock_health(&self.ocr_health.inner);
        state.consecutive_failures = CONSECUTIVE_FAILURE_LIMIT;
        state.unavailable = true;
    }

    pub fn consume(&mut self, frame: PreparedFrame, work: FrameWork) {
        if work.run_image {
            let started = Instant::now();
            let result = self.image_classifier.classify(&frame).map(|inference| {
                self.sink.consume_image(&frame, &inference);
            });
            record_result(&self.image_health, started, result);
        }

        if work.run_ocr {
            let started = Instant::now();
            let result = self
                .ocr_engine
                .classify(&frame, &self.word_pack)
                .map(|summary| {
                    self.sink.consume(summary);
                });
            record_result(&self.ocr_health, started, result);
        }
    }

    #[cfg(test)]
    fn image_health(&self) -> InferenceHealth {
        self.image_health.snapshot()
    }

    #[cfg(test)]
    fn ocr_health(&self) -> InferenceHealth {
        self.ocr_health.snapshot()
    }

    #[cfg(test)]
    fn image_classifier(&self) -> &I {
        &self.image_classifier
    }

    #[cfg(test)]
    fn ocr_engine(&self) -> &O {
        &self.ocr_engine
    }

    #[cfg(test)]
    fn ocr_engine_mut(&mut self) -> &mut O {
        &mut self.ocr_engine
    }

    #[cfg(test)]
    fn sink(&self) -> &S {
        &self.sink
    }
}

#[cfg(windows)]
impl<I, O, S> karma_windows::PreparedFrameConsumer for ScheduledInferenceConsumer<I, O, S>
where
    I: ImageClassifier + Send + 'static,
    O: OcrEngine + Send + 'static,
    S: OcrSummarySink + Send + 'static,
{
    fn consume(&mut self, frame: PreparedFrame, work: FrameWork) {
        ScheduledInferenceConsumer::consume(self, frame, work);
    }
}

#[cfg(test)]
mod tests {
    use karma_ai::{
        BgraFrame, FrameDimensions, FramePreparationConfig, FramePreparer, FrameWork,
        ImageClassifier, ImageInference, OcrEngine, OcrMatchSummary, PreparedFrame, WordPack,
    };
    use karma_domain::{MonitorId, OcrRisk, RiskCategory};

    use super::{CountingOcrSummarySink, OcrSummarySink, ScheduledInferenceConsumer};

    struct FakeImageClassifier {
        calls: usize,
        fails: bool,
    }

    impl ImageClassifier for FakeImageClassifier {
        type Error = ();

        fn classify(&mut self, _frame: &PreparedFrame) -> Result<ImageInference, Self::Error> {
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

    struct FakeOcrEngine {
        calls: usize,
        fails: bool,
    }

    impl OcrEngine for FakeOcrEngine {
        type Error = ();

        fn classify(
            &mut self,
            _frame: &PreparedFrame,
            _word_pack: &WordPack,
        ) -> Result<OcrMatchSummary, Self::Error> {
            self.calls += 1;
            if self.fails {
                Err(())
            } else {
                Ok(OcrMatchSummary {
                    risk: OcrRisk::Keyword,
                    categories: vec!["explicit_term".into()],
                    exemption_context: false,
                })
            }
        }

        fn resource_limit_events(&self) -> u64 {
            0
        }
    }

    #[derive(Default)]
    struct RecordingSink {
        calls: usize,
    }

    impl OcrSummarySink for RecordingSink {
        fn consume(&mut self, _summary: OcrMatchSummary) {
            self.calls += 1;
        }
    }

    fn prepared_frame() -> PreparedFrame {
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

    fn consumer(
        image_fails: bool,
        ocr_fails: bool,
    ) -> ScheduledInferenceConsumer<FakeImageClassifier, FakeOcrEngine, RecordingSink> {
        ScheduledInferenceConsumer::new(
            FakeImageClassifier {
                calls: 0,
                fails: image_fails,
            },
            FakeOcrEngine {
                calls: 0,
                fails: ocr_fails,
            },
            WordPack::compile(vec![]).unwrap(),
            RecordingSink::default(),
        )
    }

    #[test]
    fn schedules_each_engine_for_all_work_combinations() {
        for (run_image, run_ocr, expected_image, expected_ocr) in [
            (false, false, 0, 0),
            (true, false, 1, 0),
            (false, true, 0, 1),
            (true, true, 1, 1),
        ] {
            let mut value = consumer(false, false);
            value.consume(prepared_frame(), FrameWork { run_image, run_ocr });
            assert_eq!(value.image_classifier().calls, expected_image);
            assert_eq!(value.ocr_engine().calls, expected_ocr);
        }
    }

    #[test]
    fn evidence_gate_requires_enablement_threshold_and_cooldown() {
        use super::should_capture_evidence;

        assert!(!should_capture_evidence(false, 950, 990, 10_000, 0, 5_000));
        assert!(!should_capture_evidence(true, 950, 949, 10_000, 0, 5_000));
        assert!(!should_capture_evidence(true, 950, 950, 4_999, 0, 5_000));
        assert!(should_capture_evidence(true, 950, 950, 5_000, 0, 5_000));
    }

    #[test]
    fn either_engine_failure_does_not_suppress_the_other_engine() {
        let mut image_failure = consumer(true, false);
        image_failure.consume(
            prepared_frame(),
            FrameWork {
                run_image: true,
                run_ocr: true,
            },
        );
        assert_eq!(image_failure.ocr_engine().calls, 1);
        assert_eq!(image_failure.image_health().failures(), 1);
        assert_eq!(image_failure.ocr_health().inferences(), 1);

        let mut ocr_failure = consumer(false, true);
        ocr_failure.consume(
            prepared_frame(),
            FrameWork {
                run_image: true,
                run_ocr: true,
            },
        );
        assert_eq!(ocr_failure.image_classifier().calls, 1);
        assert_eq!(ocr_failure.image_health().inferences(), 1);
        assert_eq!(ocr_failure.ocr_health().failures(), 1);
    }

    #[test]
    fn passes_only_ocr_summary_to_sink_and_default_sink_retains_a_count() {
        let mut value = consumer(false, false);
        value.consume(
            prepared_frame(),
            FrameWork {
                run_image: false,
                run_ocr: true,
            },
        );
        assert_eq!(value.sink().calls, 1);

        let mut default_sink = CountingOcrSummarySink::default();
        default_sink.consume(OcrMatchSummary {
            risk: OcrRisk::Keyword,
            categories: vec!["must-not-retain".into()],
            exemption_context: false,
        });
        assert_eq!(default_sink.summaries(), 1);
    }

    #[test]
    fn three_ocr_failures_only_mark_ocr_unavailable_and_success_recovers_it() {
        let mut value = consumer(false, true);
        let image_health = value.image_health_handle();
        let ocr_health = value.ocr_health_handle();
        for _ in 0..3 {
            value.consume(
                prepared_frame(),
                FrameWork {
                    run_image: false,
                    run_ocr: true,
                },
            );
        }
        assert!(image_health.is_available());
        assert!(!ocr_health.is_available());

        value.ocr_engine_mut().fails = false;
        value.consume(
            prepared_frame(),
            FrameWork {
                run_image: false,
                run_ocr: true,
            },
        );
        assert!(ocr_health.is_available());
    }

    #[test]
    fn unavailable_ocr_can_start_degraded_without_affecting_image_health() {
        let mut value = consumer(false, false);
        value.mark_ocr_unavailable();

        assert!(value.image_health_handle().is_available());
        assert!(!value.ocr_health_handle().is_available());
    }

    #[test]
    fn each_consumer_keeps_independent_monitor_health() {
        let mut first = consumer(false, true);
        let second = consumer(false, false);
        for _ in 0..3 {
            first.consume(
                prepared_frame(),
                FrameWork {
                    run_image: false,
                    run_ocr: true,
                },
            );
        }
        assert!(!first.ocr_health_handle().is_available());
        assert!(second.ocr_health_handle().is_available());
    }
}
