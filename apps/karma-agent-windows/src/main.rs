#[cfg(any(windows, test))]
mod inference_consumer;
#[cfg(any(windows, test))]
mod ocr_profile;
#[cfg(any(windows, test))]
mod startup;

#[cfg(windows)]
use inference_consumer::{
    CountingOcrSummarySink, InferenceHealthHandle, ScheduledInferenceConsumer,
};
#[cfg(windows)]
use karma_ai::{OcrEngine, OcrMatchSummary, PreparedFrame, WordPack};
#[cfg(windows)]
use karma_domain::OcrRisk;
#[cfg(windows)]
use karma_onnx::{InferenceErrorKind, OnnxOcrEngine, VerifiedImageModel, VerifiedOcrBundle};
#[cfg(windows)]
use karma_windows::{
    D3d11CaptureDevice, FrameWorkerStatus, MonitorSnapshot, WgcCaptureSession, WgcCaptureTarget,
    WindowsAdapterError, WindowsFrameProcessor, WindowsFrameWorker, WindowsRuntimeApartment,
    enumerate_active_monitors,
};
#[cfg(windows)]
use startup::{CaptureTargetFactory, MonitorInventory, StartupProbe};

#[cfg(any(windows, test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct OcrRuntimeConfig {
    lightweight_manifest: std::path::PathBuf,
    accurate_manifest: Option<std::path::PathBuf>,
    profile: karma_ai::OcrModelProfile,
    preference: ocr_profile::OcrProfilePreference,
}

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OcrRuntimeConfigError {
    MissingLightweightManifest,
    InvalidProfile,
}

#[cfg(any(windows, test))]
impl OcrRuntimeConfig {
    #[cfg(windows)]
    fn from_environment() -> Result<Self, OcrRuntimeConfigError> {
        Self::from_values(
            std::env::var("KARMA_OCR_LIGHTWEIGHT_MANIFEST")
                .ok()
                .as_deref(),
            std::env::var("KARMA_OCR_ACCURATE_MANIFEST").ok().as_deref(),
            std::env::var("KARMA_OCR_PROFILE").ok().as_deref(),
        )
    }

    fn from_values(
        lightweight_manifest: Option<&str>,
        accurate_manifest: Option<&str>,
        profile: Option<&str>,
    ) -> Result<Self, OcrRuntimeConfigError> {
        use std::str::FromStr;

        let lightweight_manifest = lightweight_manifest
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .ok_or(OcrRuntimeConfigError::MissingLightweightManifest)?;
        let preference = match profile {
            Some(value) => ocr_profile::OcrProfilePreference::from_str(value)
                .map_err(|_| OcrRuntimeConfigError::InvalidProfile)?,
            None => ocr_profile::OcrProfilePreference::Auto,
        };
        let profile = match preference {
            ocr_profile::OcrProfilePreference::Accurate => karma_ai::OcrModelProfile::Accurate,
            ocr_profile::OcrProfilePreference::Auto
            | ocr_profile::OcrProfilePreference::Lightweight => {
                karma_ai::OcrModelProfile::Lightweight
            }
        };
        Ok(Self {
            lightweight_manifest,
            accurate_manifest: accurate_manifest
                .filter(|value| !value.is_empty())
                .map(std::path::PathBuf::from),
            profile,
            preference,
        })
    }
}

#[cfg(any(windows, test))]
enum OcrPreflight<T, E> {
    Ready(T),
    Unavailable(Option<E>),
}

#[cfg(any(windows, test))]
impl<T, E> OcrPreflight<T, E> {
    #[cfg(test)]
    fn bundle(&self) -> Option<&T> {
        match self {
            Self::Ready(bundle) => Some(bundle),
            Self::Unavailable(_) => None,
        }
    }

    #[cfg(windows)]
    fn error(&self) -> Option<&E> {
        match self {
            Self::Ready(_) | Self::Unavailable(None) => None,
            Self::Unavailable(Some(error)) => Some(error),
        }
    }

    #[cfg(windows)]
    fn into_bundle(self) -> Option<T> {
        match self {
            Self::Ready(bundle) => Some(bundle),
            Self::Unavailable(_) => None,
        }
    }
}

/// Validates an OCR candidate before entering the capture-start continuation.
///
/// The continuation receives an explicit type state, so capture cannot start until the validation
/// closure has either completed successfully or selected the degraded OCR state.
#[cfg(any(windows, test))]
fn start_after_ocr_preflight<T, E, R>(
    candidate: Option<T>,
    validate: impl FnOnce(&T) -> Result<(), E>,
    start_capture: impl FnOnce(OcrPreflight<T, E>) -> R,
) -> R {
    let preflight = match candidate {
        Some(candidate) => match validate(&candidate) {
            Ok(()) => OcrPreflight::Ready(candidate),
            Err(error) => OcrPreflight::Unavailable(Some(error)),
        },
        None => OcrPreflight::Unavailable(None),
    };
    start_capture(preflight)
}

#[cfg(any(windows, test))]
fn health_status_line(
    component: &str,
    status: &str,
    inferences: u64,
    failures: u64,
    latency_micros: u64,
    unavailable_monitors: u64,
) -> String {
    format!(
        "status={status} component={component} inferences={inferences} failures={failures} \
         latency_total_us={latency_micros} unavailable_monitors={unavailable_monitors}"
    )
}

#[cfg(windows)]
struct VerifiedOcrCandidateFactory<'a> {
    accurate: Option<&'a VerifiedOcrBundle>,
}

#[cfg(windows)]
impl ocr_profile::OcrCandidateFactory for VerifiedOcrCandidateFactory<'_> {
    type Engine = OnnxOcrEngine;

    fn create_accurate(&self) -> ocr_profile::AccurateCandidate<Self::Engine> {
        match self.accurate {
            Some(bundle) => match bundle.create_engine() {
                Ok(engine) => ocr_profile::AccurateCandidate::Ready(engine),
                Err(_) => ocr_profile::AccurateCandidate::Rejected,
            },
            None => ocr_profile::AccurateCandidate::Missing,
        }
    }

    fn reference_summary_matches(&self, summary: &OcrMatchSummary) -> bool {
        const BENCHMARK_CATEGORIES: [&str; 5] = [
            "benchmark_ui_0",
            "benchmark_ui_1",
            "benchmark_ui_2",
            "benchmark_ui_3",
            "benchmark_ui_4",
        ];
        summary.risk == OcrRisk::Keyword
            && summary
                .categories
                .iter()
                .map(String::as_str)
                .eq(BENCHMARK_CATEGORIES)
            && !summary.exemption_context
    }
}

#[cfg(windows)]
fn select_ocr_bundle<'a>(
    config: &OcrRuntimeConfig,
    lightweight: &'a VerifiedOcrBundle,
    accurate: Option<&'a VerifiedOcrBundle>,
    active_display_count: usize,
) -> &'a VerifiedOcrBundle {
    if config.preference == ocr_profile::OcrProfilePreference::Lightweight {
        return lightweight;
    }
    let factory = VerifiedOcrCandidateFactory { accurate };
    let Some(accurate_bundle) = accurate else {
        return lightweight;
    };
    let key = ocr_profile::BenchmarkKey {
        profile: karma_ai::OcrModelProfile::Accurate,
        bundle_version: accurate_bundle.manifest().asset.version.clone(),
        cpu_architecture: std::env::consts::ARCH.into(),
        logical_cpu_cores: std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1),
        active_display_count,
    };
    let cache_path = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|directory| directory.join("Karma").join("ocr-profile-benchmark.json"))
        .unwrap_or_else(|| std::path::PathBuf::from("ocr-profile-benchmark.json"));
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut clock = ocr_profile::SystemBenchmarkClock::new();
    match ocr_profile::ProfileSelector::new(cache_path).select(
        config.preference,
        key,
        &factory,
        &mut clock,
    ) {
        Ok(selection) if selection.profile == karma_ai::OcrModelProfile::Accurate => {
            accurate_bundle
        }
        Ok(_) | Err(_) => lightweight,
    }
}

#[cfg(windows)]
enum RuntimeOcrEngine {
    Active(OnnxOcrEngine),
    Unavailable,
}

#[cfg(windows)]
impl OcrEngine for RuntimeOcrEngine {
    type Error = ();

    fn classify(
        &mut self,
        frame: &PreparedFrame,
        word_pack: &WordPack,
    ) -> Result<OcrMatchSummary, Self::Error> {
        match self {
            Self::Active(engine) => engine.classify(frame, word_pack).map_err(|_| ()),
            Self::Unavailable => Err(()),
        }
    }

    fn resource_limit_events(&self) -> u64 {
        match self {
            Self::Active(engine) => engine.resource_limit_events(),
            Self::Unavailable => 0,
        }
    }
}

#[cfg(windows)]
struct WindowsMonitorInventory;

#[cfg(windows)]
impl MonitorInventory for WindowsMonitorInventory {
    type Monitor = MonitorSnapshot;
    type Error = WindowsAdapterError;

    fn active_monitors(&self) -> Result<Vec<Self::Monitor>, Self::Error> {
        enumerate_active_monitors()
    }
}

#[cfg(windows)]
struct WindowsCaptureTargetFactory;

#[cfg(windows)]
impl CaptureTargetFactory<MonitorSnapshot> for WindowsCaptureTargetFactory {
    type Error = WindowsAdapterError;

    fn create(&self, monitor: &MonitorSnapshot) -> Result<(), Self::Error> {
        WgcCaptureTarget::for_monitor(monitor.handle)?.size()?;
        Ok(())
    }
}

#[cfg(windows)]
fn run_windows(
    model: &VerifiedImageModel,
    ocr_bundle: Option<&VerifiedOcrBundle>,
) -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = WindowsRuntimeApartment::initialize_mta()?;
    let summary = StartupProbe::run(&WindowsMonitorInventory, &WindowsCaptureTargetFactory);
    println!(
        "status={} monitors={} wgc_ready={} wgc_failed={}",
        summary.status, summary.monitor_count, summary.wgc_ready_count, summary.wgc_failed_count
    );

    let mut workers = Vec::new();
    let mut image_health = Vec::<InferenceHealthHandle>::new();
    let mut ocr_health = Vec::<InferenceHealthHandle>::new();
    for monitor in enumerate_active_monitors()? {
        let result: Result<_, Box<dyn std::error::Error>> = (|| {
            // Each monitor owns a separate immediate context because D3D11
            // immediate contexts must not be used concurrently by workers.
            let device = D3d11CaptureDevice::new()?;
            let target = WgcCaptureTarget::for_monitor(monitor.handle)?;
            let session = WgcCaptureSession::start(monitor.id.clone(), target, &device)?;
            let processor =
                WindowsFrameProcessor::new(&device, karma_ai::FramePreparationConfig::default());
            let (ocr_engine, ocr_initialized) = match ocr_bundle {
                Some(bundle) => match bundle.create_engine() {
                    Ok(engine) => (RuntimeOcrEngine::Active(engine), true),
                    Err(error) => {
                        eprintln!(
                            "status=degraded component=ocr_inference monitor={} error={}",
                            monitor.id.0,
                            error.kind()
                        );
                        (RuntimeOcrEngine::Unavailable, false)
                    }
                },
                None => (RuntimeOcrEngine::Unavailable, false),
            };
            let mut consumer = ScheduledInferenceConsumer::new(
                model.create_classifier()?,
                ocr_engine,
                WordPack::compile(vec![]).expect("an empty word pack is valid"),
                CountingOcrSummarySink::default(),
            );
            if !ocr_initialized {
                consumer.mark_ocr_unavailable();
            }
            let image_monitor_health = consumer.image_health_handle();
            let ocr_monitor_health = consumer.ocr_health_handle();
            let worker = WindowsFrameWorker::start(session, processor, consumer)?;
            Ok((worker, image_monitor_health, ocr_monitor_health))
        })();
        match result {
            Ok((worker, image_monitor_health, ocr_monitor_health)) => {
                workers.push(worker);
                image_health.push(image_monitor_health);
                ocr_health.push(ocr_monitor_health);
            }
            Err(error) => eprintln!(
                "status=degraded component=frame_pipeline monitor={} error={}",
                monitor.id.0, error
            ),
        }
    }
    if workers.is_empty() {
        return Err(std::io::Error::other("no monitor frame workers started").into());
    }

    let mut health_tick = 0u8;
    loop {
        let active = workers.iter().any(|worker| {
            matches!(
                worker.report().status(),
                FrameWorkerStatus::Starting | FrameWorkerStatus::Running
            )
        });
        if !active {
            return Err(std::io::Error::other("all monitor frame workers stopped").into());
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        health_tick = health_tick.saturating_add(1);
        if health_tick == 10 {
            let (inferences, failures, latency_micros, unavailable_monitors) =
                image_health.iter().fold(
                    (0u64, 0u64, 0u64, 0u64),
                    |(inferences, failures, latency_micros, unavailable), health| {
                        let current = health.snapshot();
                        (
                            inferences.saturating_add(current.inferences()),
                            failures.saturating_add(current.failures()),
                            latency_micros.saturating_add(current.total_latency_micros()),
                            unavailable.saturating_add(u64::from(!health.is_available())),
                        )
                    },
                );
            let status = if unavailable_monitors == 0 {
                "running"
            } else {
                "unavailable"
            };
            println!(
                "{}",
                health_status_line(
                    "image_inference",
                    status,
                    inferences,
                    failures,
                    latency_micros,
                    unavailable_monitors,
                )
            );
            let (inferences, failures, latency_micros, unavailable_monitors) =
                ocr_health.iter().fold(
                    (0u64, 0u64, 0u64, 0u64),
                    |(inferences, failures, latency_micros, unavailable), health| {
                        let current = health.snapshot();
                        (
                            inferences.saturating_add(current.inferences()),
                            failures.saturating_add(current.failures()),
                            latency_micros.saturating_add(current.total_latency_micros()),
                            unavailable.saturating_add(u64::from(!health.is_available())),
                        )
                    },
                );
            let status = if unavailable_monitors == 0 {
                "running"
            } else {
                "unavailable"
            };
            println!(
                "{}",
                health_status_line(
                    "ocr_inference",
                    status,
                    inferences,
                    failures,
                    latency_micros,
                    unavailable_monitors,
                )
            );
            health_tick = 0;
        }
    }
}

#[cfg(windows)]
fn main() {
    let model = match std::env::var_os("KARMA_IMAGE_MODEL_MANIFEST") {
        Some(path) => match VerifiedImageModel::load(path) {
            Ok(model) => model,
            Err(error) => {
                eprintln!(
                    "status=unavailable component=image_inference error={}",
                    error.kind()
                );
                std::process::exit(1);
            }
        },
        None => {
            eprintln!(
                "status=unavailable component=image_inference error={}",
                InferenceErrorKind::ManifestInvalid
            );
            std::process::exit(1);
        }
    };
    if let Err(error) = model.create_classifier() {
        eprintln!(
            "status=unavailable component=image_inference error={}",
            error.kind()
        );
        std::process::exit(1);
    }
    let ocr_bundles = match OcrRuntimeConfig::from_environment() {
        Ok(config) => match VerifiedOcrBundle::load(&config.lightweight_manifest) {
            Ok(lightweight) => {
                let accurate = config.accurate_manifest.as_deref().and_then(|manifest| {
                    match VerifiedOcrBundle::load(manifest) {
                        Ok(bundle) => Some(bundle),
                        Err(error) => {
                            eprintln!(
                                "status=degraded component=ocr_inference error={}",
                                error.kind()
                            );
                            None
                        }
                    }
                });
                Some((config, lightweight, accurate))
            }
            Err(error) => {
                eprintln!(
                    "status=degraded component=ocr_inference error={}",
                    error.kind()
                );
                None
            }
        },
        Err(OcrRuntimeConfigError::MissingLightweightManifest) => {
            eprintln!("status=degraded component=ocr_inference error=manifest_missing");
            None
        }
        Err(OcrRuntimeConfigError::InvalidProfile) => {
            eprintln!("status=degraded component=ocr_inference error=profile_invalid");
            None
        }
    };
    let active_display_count = enumerate_active_monitors().map_or(0, |monitors| monitors.len());
    let ocr_bundle = ocr_bundles.as_ref().map(|(config, lightweight, accurate)| {
        select_ocr_bundle(config, lightweight, accurate.as_ref(), active_display_count)
    });
    let result = start_after_ocr_preflight(
        ocr_bundle,
        |bundle| {
            bundle
                .create_engine()
                .map(|engine| drop(engine))
                .map_err(|error| error.kind())
        },
        |preflight| {
            if let Some(error) = preflight.error() {
                eprintln!("status=degraded component=ocr_inference error={error}");
            }
            run_windows(&model, preflight.into_bundle())
        },
    );
    if result.is_err() {
        eprintln!("status=unavailable component=windows_runtime");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("karma-agent-windows is supported only on Windows");
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use karma_ai::OcrModelProfile;

    use super::{
        OcrPreflight, OcrRuntimeConfig, OcrRuntimeConfigError, health_status_line,
        start_after_ocr_preflight,
    };

    #[test]
    fn preflight_completes_before_capture_starts_and_degrades_without_an_engine() {
        let events = RefCell::new(Vec::new());
        start_after_ocr_preflight(
            Some("selected bundle"),
            |bundle| {
                assert_eq!(*bundle, "selected bundle");
                events.borrow_mut().push("preflight");
                Err("ocr_contract_invalid")
            },
            |preflight| {
                events.borrow_mut().push("capture");
                assert!(matches!(preflight, OcrPreflight::Unavailable(Some(_))));
                assert!(preflight.bundle().is_none());
            },
        );
        assert_eq!(*events.borrow(), ["preflight", "capture"]);
    }

    #[test]
    fn health_status_line_is_single_line_and_has_no_escape_marker() {
        let line = health_status_line("ocr_inference", "running", 3, 1, 42, 0);

        assert!(!line.contains(['\\', '\n', '\r']));
        assert_eq!(
            line,
            "status=running component=ocr_inference inferences=3 failures=1 latency_total_us=42 unavailable_monitors=0"
        );
    }

    #[test]
    fn ocr_runtime_config_requires_lightweight_manifest_and_validates_profile() {
        assert_eq!(
            OcrRuntimeConfig::from_values(None, None, None),
            Err(OcrRuntimeConfigError::MissingLightweightManifest)
        );
        assert_eq!(
            OcrRuntimeConfig::from_values(Some("lightweight.json"), None, Some("fast")),
            Err(OcrRuntimeConfigError::InvalidProfile)
        );
    }

    #[test]
    fn ocr_runtime_config_accepts_optional_accurate_manifest_and_profile() {
        let config = OcrRuntimeConfig::from_values(
            Some("lightweight.json"),
            Some("accurate.json"),
            Some("accurate"),
        )
        .unwrap();

        assert_eq!(
            config.lightweight_manifest,
            PathBuf::from("lightweight.json")
        );
        assert_eq!(
            config.accurate_manifest,
            Some(PathBuf::from("accurate.json"))
        );
        assert_eq!(config.profile, OcrModelProfile::Accurate);
    }

    #[test]
    fn ocr_runtime_config_accepts_only_documented_profile_values() {
        let cases = [
            (None, OcrModelProfile::Lightweight),
            (Some("auto"), OcrModelProfile::Lightweight),
            (Some("lightweight"), OcrModelProfile::Lightweight),
            (Some("accurate"), OcrModelProfile::Accurate),
        ];

        for (value, expected_profile) in cases {
            let config = OcrRuntimeConfig::from_values(Some("lightweight.json"), None, value)
                .expect("documented profile value must parse");
            assert_eq!(config.profile, expected_profile);
        }
        for value in ["Auto", " lightweight", "accurate ", "other"] {
            assert_eq!(
                OcrRuntimeConfig::from_values(Some("lightweight.json"), None, Some(value)),
                Err(OcrRuntimeConfigError::InvalidProfile)
            );
        }
    }
}
