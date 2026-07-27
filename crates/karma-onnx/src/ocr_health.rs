#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OcrInferenceHealth {
    inferences: u64,
    failures: u64,
    skipped_boxes: u64,
    resource_limit_events: u64,
    total_latency_micros: u64,
    last_latency_micros: u64,
}

impl OcrInferenceHealth {
    pub fn inferences(self) -> u64 {
        self.inferences
    }

    pub fn failures(self) -> u64 {
        self.failures
    }

    pub fn skipped_boxes(self) -> u64 {
        self.skipped_boxes
    }

    pub fn resource_limit_events(self) -> u64 {
        self.resource_limit_events
    }

    pub fn total_latency_micros(self) -> u64 {
        self.total_latency_micros
    }

    pub fn last_latency_micros(self) -> u64 {
        self.last_latency_micros
    }

    pub(crate) fn record_success(
        &mut self,
        latency_micros: u64,
        skipped_boxes: usize,
        resource_limit_reached: bool,
    ) {
        self.inferences = self.inferences.saturating_add(1);
        self.skipped_boxes = self
            .skipped_boxes
            .saturating_add(skipped_boxes.min(u64::MAX as usize) as u64);
        self.resource_limit_events = self
            .resource_limit_events
            .saturating_add(u64::from(resource_limit_reached));
        self.total_latency_micros = self.total_latency_micros.saturating_add(latency_micros);
        self.last_latency_micros = latency_micros;
    }

    pub(crate) fn record_failure(&mut self) {
        self.failures = self.failures.saturating_add(1);
    }
}
