#[cfg(any(windows, test))]
mod image_consumer;
#[cfg(any(windows, test))]
mod startup;

#[cfg(windows)]
use image_consumer::{InferenceHealthHandle, ScheduledImageConsumer};
#[cfg(windows)]
use karma_onnx::{InferenceErrorKind, VerifiedImageModel};
#[cfg(windows)]
use karma_windows::{
    D3d11CaptureDevice, FrameWorkerStatus, MonitorSnapshot, WgcCaptureSession, WgcCaptureTarget,
    WindowsAdapterError, WindowsFrameProcessor, WindowsFrameWorker, WindowsRuntimeApartment,
    enumerate_active_monitors,
};
#[cfg(windows)]
use startup::{CaptureTargetFactory, MonitorInventory, StartupProbe};

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
fn run_windows(model: &VerifiedImageModel) -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = WindowsRuntimeApartment::initialize_mta()?;
    let summary = StartupProbe::run(&WindowsMonitorInventory, &WindowsCaptureTargetFactory);
    println!(
        "status={} monitors={} wgc_ready={} wgc_failed={}",
        summary.status, summary.monitor_count, summary.wgc_ready_count, summary.wgc_failed_count
    );

    let mut workers = Vec::new();
    let mut inference_health = Vec::<InferenceHealthHandle>::new();
    for monitor in enumerate_active_monitors()? {
        let result: Result<_, Box<dyn std::error::Error>> = (|| {
            // Each monitor owns a separate immediate context because D3D11
            // immediate contexts must not be used concurrently by workers.
            let device = D3d11CaptureDevice::new()?;
            let target = WgcCaptureTarget::for_monitor(monitor.handle)?;
            let session = WgcCaptureSession::start(monitor.id.clone(), target, &device)?;
            let processor =
                WindowsFrameProcessor::new(&device, karma_ai::FramePreparationConfig::default());
            let consumer = ScheduledImageConsumer::new(model.create_classifier()?);
            let health = consumer.health_handle();
            let worker = WindowsFrameWorker::start(session, processor, consumer)?;
            Ok((worker, health))
        })();
        match result {
            Ok((worker, health)) => {
                workers.push(worker);
                inference_health.push(health);
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
                inference_health.iter().fold(
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
                "status={status} component=image_inference inferences={inferences} \
                 failures={failures} latency_total_us={latency_micros} \
                 unavailable_monitors={unavailable_monitors}"
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
    if run_windows(&model).is_err() {
        eprintln!("status=unavailable component=windows_runtime");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("karma-agent-windows is supported only on Windows");
}
