#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameProcessingHealth {
    processed_frames: u64,
    gpu_fallbacks: u64,
    failures: u64,
}

impl FrameProcessingHealth {
    pub fn processed_frames(self) -> u64 {
        self.processed_frames
    }

    pub fn gpu_fallbacks(self) -> u64 {
        self.gpu_fallbacks
    }

    pub fn failures(self) -> u64 {
        self.failures
    }

    #[cfg(any(windows, test))]
    fn record_success(&mut self, used_fallback: bool) {
        self.processed_frames = self.processed_frames.saturating_add(1);
        if used_fallback {
            self.gpu_fallbacks = self.gpu_fallbacks.saturating_add(1);
        }
    }

    #[cfg(any(windows, test))]
    fn record_failure(&mut self) {
        self.failures = self.failures.saturating_add(1);
    }
}

#[cfg(any(windows, test))]
trait ProcessingPaths {
    type Output;
    type Error;

    fn gpu(&mut self) -> Option<Self::Output>;
    fn fallback(&mut self) -> Result<Self::Output, Self::Error>;
}

#[cfg(any(windows, test))]
fn process_prefer_gpu<P: ProcessingPaths>(paths: &mut P) -> Result<(P::Output, bool), P::Error> {
    match paths.gpu() {
        Some(output) => Ok((output, false)),
        None => paths.fallback().map(|output| (output, true)),
    }
}

#[cfg(windows)]
mod native {
    use karma_ai::{FrameError, FramePreparationConfig, FramePreparer, PreparedFrame};
    use thiserror::Error;

    use super::{FrameProcessingHealth, ProcessingPaths, process_prefer_gpu};
    use crate::{
        CapturedGpuFrame, D3d11CaptureDevice, GpuFrameScaler, NativeCaptureTexture,
        StagingTextureReader, WindowsAdapterError,
    };

    #[derive(Debug, Error)]
    pub enum FrameProcessingError {
        #[error("Windows frame access failed")]
        Windows(#[from] WindowsAdapterError),
        #[error("portable frame preparation failed")]
        Frame(#[from] FrameError),
    }

    struct WindowsPaths<'a, 'frame> {
        gpu_scaler: &'a mut Option<GpuFrameScaler>,
        staging: &'a mut StagingTextureReader,
        preparer: &'a FramePreparer,
        config: FramePreparationConfig,
        frame: &'frame CapturedGpuFrame,
        texture: &'a NativeCaptureTexture<'frame>,
        captured_at_ms: i64,
    }

    impl ProcessingPaths for WindowsPaths<'_, '_> {
        type Output = PreparedFrame;
        type Error = FrameProcessingError;

        fn gpu(&mut self) -> Option<Self::Output> {
            let result = (|| {
                let target = self.config.target(self.texture.dimensions()).ok()?;
                let output = self.gpu_scaler.as_mut()?.scale(self.texture, target).ok()?;
                let mapped = self
                    .staging
                    .read(
                        self.frame.monitor_id().clone(),
                        self.captured_at_ms,
                        output,
                        target,
                    )
                    .ok()?;
                self.preparer.prepare(mapped).ok()
            })();
            if result.is_none() {
                *self.gpu_scaler = None;
            }
            result
        }

        fn fallback(&mut self) -> Result<Self::Output, Self::Error> {
            let mapped = self.staging.read_source(
                self.frame.monitor_id().clone(),
                self.captured_at_ms,
                self.texture.texture(),
                self.texture.dimensions(),
            )?;
            Ok(self.preparer.prepare(mapped)?)
        }
    }

    pub struct WindowsFrameProcessor {
        gpu_scaler: Option<GpuFrameScaler>,
        staging: StagingTextureReader,
        preparer: FramePreparer,
        config: FramePreparationConfig,
        health: FrameProcessingHealth,
    }

    impl WindowsFrameProcessor {
        pub fn new(device: &D3d11CaptureDevice, config: FramePreparationConfig) -> Self {
            Self {
                gpu_scaler: GpuFrameScaler::new(device).ok(),
                staging: StagingTextureReader::new(device),
                preparer: FramePreparer::new(config),
                config,
                health: FrameProcessingHealth::default(),
            }
        }

        fn process_inner(
            &mut self,
            frame: &CapturedGpuFrame,
        ) -> Result<(PreparedFrame, bool), FrameProcessingError> {
            let captured_at_ms = frame.captured_at_ms()?;
            let texture = NativeCaptureTexture::from_frame(frame)?;
            let mut paths = WindowsPaths {
                gpu_scaler: &mut self.gpu_scaler,
                staging: &mut self.staging,
                preparer: &self.preparer,
                config: self.config,
                frame,
                texture: &texture,
                captured_at_ms,
            };
            process_prefer_gpu(&mut paths)
        }

        pub fn process(
            &mut self,
            frame: &CapturedGpuFrame,
        ) -> Result<PreparedFrame, FrameProcessingError> {
            match self.process_inner(frame) {
                Ok((prepared, used_fallback)) => {
                    self.health.record_success(used_fallback);
                    Ok(prepared)
                }
                Err(error) => {
                    self.health.record_failure();
                    Err(error)
                }
            }
        }

        pub fn health(&self) -> FrameProcessingHealth {
            self.health
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn processor_can_be_owned_by_a_processing_worker() {
            fn require_send<T: Send>() {}
            require_send::<WindowsFrameProcessor>();
        }

        #[test]
        fn constructor_has_a_stable_signature() {
            let _new: fn(&D3d11CaptureDevice, FramePreparationConfig) -> WindowsFrameProcessor =
                WindowsFrameProcessor::new;
        }
    }
}

#[cfg(windows)]
pub use native::{FrameProcessingError, WindowsFrameProcessor};

#[cfg(test)]
mod tests {
    use super::{FrameProcessingHealth, ProcessingPaths, process_prefer_gpu};

    struct FakePaths {
        gpu: Option<u8>,
        fallback: Result<u8, &'static str>,
        gpu_calls: usize,
        fallback_calls: usize,
    }

    impl Default for FakePaths {
        fn default() -> Self {
            Self {
                gpu: None,
                fallback: Err("fallback not configured"),
                gpu_calls: 0,
                fallback_calls: 0,
            }
        }
    }

    impl ProcessingPaths for FakePaths {
        type Output = u8;
        type Error = &'static str;

        fn gpu(&mut self) -> Option<Self::Output> {
            self.gpu_calls += 1;
            self.gpu
        }

        fn fallback(&mut self) -> Result<Self::Output, Self::Error> {
            self.fallback_calls += 1;
            self.fallback
        }
    }

    #[test]
    fn gpu_success_skips_fallback() {
        let mut paths = FakePaths {
            gpu: Some(7),
            fallback: Ok(9),
            ..Default::default()
        };
        assert_eq!(process_prefer_gpu(&mut paths), Ok((7, false)));
        assert_eq!((paths.gpu_calls, paths.fallback_calls), (1, 0));
    }

    #[test]
    fn missing_or_failed_gpu_uses_fallback() {
        let mut paths = FakePaths {
            gpu: None,
            fallback: Ok(9),
            ..Default::default()
        };
        assert_eq!(process_prefer_gpu(&mut paths), Ok((9, true)));
        assert_eq!((paths.gpu_calls, paths.fallback_calls), (1, 1));
    }

    #[test]
    fn fallback_error_is_preserved() {
        let mut paths = FakePaths {
            gpu: None,
            fallback: Err("map failed"),
            ..Default::default()
        };
        assert_eq!(process_prefer_gpu(&mut paths), Err("map failed"));
    }

    #[test]
    fn health_counters_start_at_zero() {
        assert_eq!(FrameProcessingHealth::default().processed_frames(), 0);
        assert_eq!(FrameProcessingHealth::default().gpu_fallbacks(), 0);
        assert_eq!(FrameProcessingHealth::default().failures(), 0);
    }

    #[test]
    fn health_counters_track_only_categories_not_content() {
        let mut health = FrameProcessingHealth::default();
        health.record_success(false);
        health.record_success(true);
        health.record_failure();
        assert_eq!(health.processed_frames(), 2);
        assert_eq!(health.gpu_fallbacks(), 1);
        assert_eq!(health.failures(), 1);
    }
}
