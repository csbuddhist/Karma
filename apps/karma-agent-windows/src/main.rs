#[cfg(any(windows, test))]
mod startup;

#[cfg(windows)]
use karma_windows::{
    D3d11CaptureDevice, FrameWorkerStatus, MonitorSnapshot, NoopFrameConsumer, WgcCaptureSession,
    WgcCaptureTarget, WindowsAdapterError, WindowsFrameProcessor, WindowsFrameWorker,
    WindowsRuntimeApartment, enumerate_active_monitors,
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
fn run_windows() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = WindowsRuntimeApartment::initialize_mta()?;
    let summary = StartupProbe::run(&WindowsMonitorInventory, &WindowsCaptureTargetFactory);
    println!(
        "status={} monitors={} wgc_ready={} wgc_failed={}",
        summary.status, summary.monitor_count, summary.wgc_ready_count, summary.wgc_failed_count
    );

    let mut workers = Vec::new();
    for monitor in enumerate_active_monitors()? {
        let result = (|| {
            // Each monitor owns a separate immediate context because D3D11
            // immediate contexts must not be used concurrently by workers.
            let device = D3d11CaptureDevice::new()?;
            let target = WgcCaptureTarget::for_monitor(monitor.handle)?;
            let session = WgcCaptureSession::start(monitor.id.clone(), target, &device)?;
            let processor =
                WindowsFrameProcessor::new(&device, karma_ai::FramePreparationConfig::default());
            WindowsFrameWorker::start(session, processor, NoopFrameConsumer)
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
        })();
        match result {
            Ok(worker) => workers.push(worker),
            Err(error) => eprintln!(
                "status=degraded component=frame_pipeline monitor={} error={}",
                monitor.id.0, error
            ),
        }
    }
    if workers.is_empty() {
        return Err(std::io::Error::other("no monitor frame workers started").into());
    }

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
    }
}

#[cfg(windows)]
fn main() {
    if run_windows().is_err() {
        eprintln!("status=unavailable component=windows_runtime");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("karma-agent-windows is supported only on Windows");
}
