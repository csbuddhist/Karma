#[cfg(windows)]
mod service_client;

#[cfg(windows)]
use karma_agent_core::{
    inference::{InferenceHealthHandle, ScheduledInferenceConsumer},
    runtime::{
        OcrPreflight, OcrRuntimeConfig, OcrRuntimeConfigError, RuntimeOcrEngine, WordPackSource,
        health_status_line, ocr_performance_warning, select_ocr_bundle, start_after_ocr_preflight,
    },
    service::{
        AgentInferenceSink, ApplicationPolicyHandle, ContextPolicyHandle, RecognitionPolicyHandle,
    },
    startup::{CaptureTargetFactory, MonitorInventory, StartupProbe},
};
#[cfg(windows)]
use karma_onnx::{InferenceErrorKind, VerifiedImageModel, VerifiedOcrBundle};
#[cfg(windows)]
use karma_windows::{
    D3d11CaptureDevice, FrameWorkerStatus, MonitorSnapshot, WgcCaptureSession, WgcCaptureTarget,
    WindowsAdapterError, WindowsFrameProcessor, WindowsFrameWorker, WindowsRuntimeApartment,
    enumerate_active_monitors,
};
#[cfg(windows)]
use service_client::WindowsSourceIdentityProvider;

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
    word_pack_source: WordPackSource<'static>,
) -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = WindowsRuntimeApartment::initialize_mta()?;
    let summary = StartupProbe::run(&WindowsMonitorInventory, &WindowsCaptureTargetFactory);
    println!(
        "status={} monitors={} wgc_ready={} wgc_failed={}",
        summary.status, summary.monitor_count, summary.wgc_ready_count, summary.wgc_failed_count
    );

    let service_client = match service_client::client_from_environment() {
        Ok(client) => client.map(std::sync::Arc::new),
        Err(error) => {
            eprintln!("status=degraded component=service_ipc error={error}");
            None
        }
    };
    let recognition_policy = RecognitionPolicyHandle::default();
    let context_policy = ContextPolicyHandle::default();
    let application_policy = ApplicationPolicyHandle::default();
    if let Some(client) = &service_client {
        if let Ok(snapshot) = client.fetch_policy() {
            recognition_policy.update(&snapshot.policy);
            context_policy.update(&snapshot.policy);
            application_policy.update(&snapshot.policy);
        }
    }
    let _context_monitor = service_client.as_ref().map(|client| {
        service_client::start_context_monitor(
            client.clone(),
            context_policy.clone(),
            application_policy.clone(),
        )
    });

    let mut workers = Vec::new();
    let mut image_health = Vec::<InferenceHealthHandle>::new();
    let mut ocr_health = Vec::<InferenceHealthHandle>::new();
    let mut active_monitors = Vec::<MonitorSnapshot>::new();
    for (monitor_index, monitor) in enumerate_active_monitors()?.into_iter().enumerate() {
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
                word_pack_source
                    .compile()
                    .map_err(|_| std::io::Error::other("word pack invalid"))?,
                AgentInferenceSink::new(
                    service_client.clone(),
                    recognition_policy.clone(),
                    format!("显示器 {}", monitor_index.saturating_add(1)),
                    WindowsSourceIdentityProvider::new(monitor.bounds),
                ),
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
                active_monitors.push(monitor);
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
            if let Some(client) = &service_client {
                let monitors = active_monitors
                    .iter()
                    .zip(&workers)
                    .zip(image_health.iter().zip(&ocr_health))
                    .enumerate()
                    .map(|(index, ((monitor, worker), (image, ocr)))| {
                        service_client::monitor_health(monitor, index, worker.report(), image, ocr)
                    })
                    .collect();
                match client.publish_health(monitors) {
                    Ok(snapshot) => {
                        recognition_policy.update(&snapshot.policy);
                        context_policy.update(&snapshot.policy);
                        application_policy.update(&snapshot.policy);
                        println!(
                            "status=running component=service_ipc policy_revision={} protection_enabled={}",
                            snapshot.revision, snapshot.protection_enabled
                        )
                    }
                    Err(error) => {
                        eprintln!("status=degraded component=service_ipc error={error}")
                    }
                }
            }
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

/// Where the accurate OCR profile's benchmark result is cached per user.
#[cfg(windows)]
fn benchmark_cache_path() -> std::path::PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|directory| directory.join("Karma").join("ocr-profile-benchmark.json"))
        .unwrap_or_else(|| std::path::PathBuf::from("ocr-profile-benchmark.json"))
}

#[cfg(windows)]
fn main() {
    let word_pack_source = match WordPackSource::bundled() {
        Ok(source) => source,
        Err(_) => {
            eprintln!("status=unavailable component=ocr_word_pack error=configuration_invalid");
            std::process::exit(1);
        }
    };
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
        select_ocr_bundle(
            config,
            lightweight,
            accurate.as_ref(),
            active_display_count,
            benchmark_cache_path(),
        )
    });
    let result = start_after_ocr_preflight(
        ocr_bundle,
        |selection| {
            selection
                .bundle
                .create_engine()
                .map(drop)
                .map_err(|error| error.kind())
        },
        |preflight| {
            if let OcrPreflight::Ready(selection) = &preflight {
                if let Some(warning) =
                    ocr_performance_warning(selection.performance_budget_exceeded)
                {
                    eprintln!("{warning}");
                }
            }
            if let Some(error) = preflight.error() {
                eprintln!("status=degraded component=ocr_inference error={error}");
            }
            run_windows(
                &model,
                preflight.into_bundle().map(|selection| selection.bundle),
                word_pack_source,
            )
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
