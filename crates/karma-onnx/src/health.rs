#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InferenceHealth {
    inferences: u64,
    failures: u64,
    total_latency_micros: u64,
    last_latency_micros: u64,
}

impl InferenceHealth {
    pub fn inferences(self) -> u64 {
        self.inferences
    }

    pub fn failures(self) -> u64 {
        self.failures
    }

    pub fn total_latency_micros(self) -> u64 {
        self.total_latency_micros
    }

    pub fn last_latency_micros(self) -> u64 {
        self.last_latency_micros
    }

    pub(crate) fn record_success(&mut self, latency_micros: u64) {
        self.inferences = self.inferences.saturating_add(1);
        self.total_latency_micros = self.total_latency_micros.saturating_add(latency_micros);
        self.last_latency_micros = latency_micros;
    }

    pub(crate) fn record_failure(&mut self) {
        self.failures = self.failures.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_success_and_failure_without_content() {
        let mut health = InferenceHealth::default();

        health.record_success(120);
        health.record_failure();

        assert_eq!(health.inferences(), 1);
        assert_eq!(health.failures(), 1);
        assert_eq!(health.total_latency_micros(), 120);
        assert_eq!(health.last_latency_micros(), 120);
    }
}
