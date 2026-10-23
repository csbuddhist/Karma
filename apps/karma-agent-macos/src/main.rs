//! KarmaAgent for macOS: captures every active display with ScreenCaptureKit,
//! runs the shared image and OCR inference, attributes frames to the
//! foreground window, and reports to KarmaService over the local socket.
//!
//! launchd starts one agent per graphical session from the KarmaAgent.app
//! LaunchAgent. KarmaService identifies the agent by its executable, so the
//! agent sends no secret of its own.

#[cfg(target_os = "macos")]
mod attribution;
#[cfg(target_os = "macos")]
mod worker;

#[cfg(target_os = "macos")]
mod agent {
    use std::{
        collections::{BTreeMap, HashMap},
        env,
        path::{Path, PathBuf},
        sync::Arc,
        thread,
        time::{Duration, Instant},
    };

    use karma_agent_core::{
        inference::{InferenceHealthHandle, ScheduledInferenceConsumer},
        runtime::{
            OcrPreflight, OcrRuntimeConfig, OcrRuntimeConfigError, RuntimeOcrEngine,
            WordPackSource, health_status_line, ocr_performance_warning, select_ocr_bundle,
            start_after_ocr_preflight,
        },
        service::{
            AgentInferenceSink, AgentServiceClient, ApplicationPolicyHandle, ContextPolicyHandle,
            RecognitionPolicyHandle, inference_state, start_context_monitor,
        },
    };
    use karma_ipc::{ComponentState, MonitorHealth, RequestEnvelope, ResponseEnvelope};
    use karma_macos::{
        CaptureError,
        capture::DisplayStream,
        display::{self, DisplaySnapshot},
    };
    use karma_onnx::{InferenceErrorKind, VerifiedImageModel, VerifiedOcrBundle};
    use zeroize::Zeroizing;

    use crate::{
        attribution::{MacosForegroundContext, MacosSourceIdentityProvider},
        worker::DisplayWorker,
    };

    const IMAGE_MANIFEST_ENV: &str = "KARMA_IMAGE_MODEL_MANIFEST";
    const OCR_LIGHTWEIGHT_MANIFEST_ENV: &str = "KARMA_OCR_LIGHTWEIGHT_MANIFEST";
    const OCR_ACCURATE_MANIFEST_ENV: &str = "KARMA_OCR_ACCURATE_MANIFEST";
    const OCR_PROFILE_ENV: &str = "KARMA_OCR_PROFILE";
    /// Model locations under the install directory that holds KarmaAgent.app.
    const INSTALLED_IMAGE_MANIFEST: &str = "models/image/viddexa-nano/manifest.json";
    const INSTALLED_OCR_MANIFEST: &str = "models/ocr/pp-ocrv5-mobile/manifest.json";
    /// KarmaService replaces this with the core token once it has verified
    /// that the connection comes from the installed agent.
    const PEER_VERIFIED_TOKEN: &str = "peer-verified";
    const TICK: Duration = Duration::from_secs(1);
    const DISPLAY_CHECK_TICKS: u32 = 2;
    const HEARTBEAT_TICKS: u32 = 10;
    const FIRST_RETRY: Duration = Duration::from_secs(2);
    const MAXIMUM_RETRY: Duration = Duration::from_secs(30);
    const ACCESS_DENIED_RETRY: Duration = Duration::from_secs(10);

    pub fn run() {
        let word_pack_source = match WordPackSource::bundled() {
            Ok(source) => source,
            Err(_) => {
                eprintln!("status=unavailable component=ocr_word_pack error=configuration_invalid");
                std::process::exit(1);
            }
        };
        let model = match manifest_path(IMAGE_MANIFEST_ENV, INSTALLED_IMAGE_MANIFEST) {
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
        let ocr_bundles = match ocr_config() {
            Ok(config) => match VerifiedOcrBundle::load(&config.lightweight_manifest) {
                Ok(lightweight) => {
                    let accurate = config.accurate_manifest.as_deref().and_then(|manifest| {
                        VerifiedOcrBundle::load(manifest)
                            .map_err(|error| {
                                eprintln!(
                                    "status=degraded component=ocr_inference error={}",
                                    error.kind()
                                )
                            })
                            .ok()
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
        let active_display_count = display::active_displays().map_or(0, |displays| displays.len());
        let ocr_bundle = ocr_bundles.as_ref().map(|(config, lightweight, accurate)| {
            select_ocr_bundle(
                config,
                lightweight,
                accurate.as_ref(),
                active_display_count,
                benchmark_cache_path(),
            )
        });
        start_after_ocr_preflight(
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
                Supervisor::new(
                    &model,
                    preflight.into_bundle().map(|selection| selection.bundle),
                    word_pack_source,
                )
                .run()
            },
        );
    }

    /// The manifest named by `variable`, or the installed one next to the
    /// application bundle this agent runs from.
    fn manifest_path(variable: &str, installed: &str) -> Option<PathBuf> {
        env::var_os(variable)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| install_directory().map(|directory| directory.join(installed)))
    }

    fn ocr_config() -> Result<OcrRuntimeConfig, OcrRuntimeConfigError> {
        let lightweight = manifest_path(OCR_LIGHTWEIGHT_MANIFEST_ENV, INSTALLED_OCR_MANIFEST);
        OcrRuntimeConfig::from_values(
            lightweight.as_deref().and_then(Path::to_str),
            env::var(OCR_ACCURATE_MANIFEST_ENV).ok().as_deref(),
            env::var(OCR_PROFILE_ENV).ok().as_deref(),
        )
    }

    /// `<install>/KarmaAgent.app/Contents/MacOS/karma-agent-macos` gives
    /// `<install>`; a development build outside a bundle has none.
    fn install_directory() -> Option<PathBuf> {
        let executable = env::current_exe().ok()?.canonicalize().ok()?;
        let bundle = executable.ancestors().nth(3)?;
        bundle
            .extension()
            .is_some_and(|extension| extension == "app")
            .then(|| bundle.parent().map(Path::to_path_buf))
            .flatten()
    }

    fn benchmark_cache_path() -> PathBuf {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Caches/Karma/ocr-profile-benchmark.json"))
            .unwrap_or_else(|| PathBuf::from("ocr-profile-benchmark.json"))
    }

    fn socket_transport(
        request: &RequestEnvelope,
        timeout_ms: u32,
    ) -> Result<ResponseEnvelope, ()> {
        karma_macos_ipc::send_request(request, timeout_ms).map_err(drop)
    }

    struct ActiveDisplay {
        worker: DisplayWorker,
        stream: DisplayStream,
        image_health: InferenceHealthHandle,
        ocr_health: InferenceHealthHandle,
    }

    impl ActiveDisplay {
        fn stop(mut self) {
            self.worker.stop();
            self.stream.stop();
        }
    }

    struct RetryingDisplay {
        retry_at: Instant,
        delay: Duration,
        error: CaptureError,
    }

    /// Keeps one capture stream and worker running for every active display.
    struct Supervisor<'a> {
        model: &'a VerifiedImageModel,
        ocr_bundle: Option<&'a VerifiedOcrBundle>,
        word_pack_source: WordPackSource<'static>,
        client: Arc<AgentServiceClient>,
        recognition_policy: RecognitionPolicyHandle,
        context_policy: ContextPolicyHandle,
        application_policy: ApplicationPolicyHandle,
        displays: Vec<DisplaySnapshot>,
        active: BTreeMap<u32, ActiveDisplay>,
        retrying: HashMap<u32, RetryingDisplay>,
    }

    impl<'a> Supervisor<'a> {
        fn new(
            model: &'a VerifiedImageModel,
            ocr_bundle: Option<&'a VerifiedOcrBundle>,
            word_pack_source: WordPackSource<'static>,
        ) -> Self {
            // SAFETY: getuid has no preconditions.
            let uid = unsafe { libc::getuid() };
            Self {
                model,
                ocr_bundle,
                word_pack_source,
                client: Arc::new(AgentServiceClient::new(
                    Zeroizing::new(PEER_VERIFIED_TOKEN.into()),
                    format!("uid-{uid}"),
                    socket_transport,
                )),
                recognition_policy: RecognitionPolicyHandle::default(),
                context_policy: ContextPolicyHandle::default(),
                application_policy: ApplicationPolicyHandle::default(),
                displays: Vec::new(),
                active: BTreeMap::new(),
                retrying: HashMap::new(),
            }
        }

        fn run(mut self) {
            if !display::screen_capture_allowed() && running_from_bundle() {
                // First launch: macOS shows its consent prompt once and the
                // user allows KarmaAgent in System Settings.
                display::request_screen_capture();
            }
            match self.client.fetch_policy() {
                Ok(snapshot) => self.apply_policy(&snapshot.policy),
                Err(error) => eprintln!("status=degraded component=service_ipc error={error}"),
            }
            let _context_monitor = start_context_monitor(
                Arc::clone(&self.client),
                self.context_policy.clone(),
                self.application_policy.clone(),
                || Some(MacosForegroundContext),
            );
            let mut tick = 0_u32;
            loop {
                if tick % DISPLAY_CHECK_TICKS == 0 {
                    self.reconcile_displays();
                }
                if tick % HEARTBEAT_TICKS == 0 {
                    self.publish_health();
                }
                tick = tick.wrapping_add(1);
                thread::sleep(TICK);
            }
        }

        fn apply_policy(&self, policy: &serde_json::Value) {
            self.recognition_policy.update(policy);
            self.context_policy.update(policy);
            self.application_policy.update(policy);
        }

        /// Starts streams for new displays, retires streams for removed or
        /// failed ones, and retries failures with backoff.
        fn reconcile_displays(&mut self) {
            let Ok(current) = display::active_displays() else {
                eprintln!("status=degraded component=frame_pipeline error=displays_unavailable");
                return;
            };
            let ended: Vec<u32> = self
                .active
                .iter()
                .filter(|(id, active)| {
                    !current.iter().any(|display| display.id == **id)
                        || !active.worker.is_active()
                        || !active.stream.status().is_active()
                })
                .map(|(id, _)| *id)
                .collect();
            for id in ended {
                if let Some(active) = self.active.remove(&id) {
                    let status = active.stream.status();
                    active.stop();
                    eprintln!(
                        "status=stopped component=frame_pipeline monitor=display-{id} reason={status:?}"
                    );
                }
            }
            self.retrying
                .retain(|id, _| current.iter().any(|display| display.id == *id));
            let now = Instant::now();
            for snapshot in &current {
                if self.active.contains_key(&snapshot.id)
                    || self
                        .retrying
                        .get(&snapshot.id)
                        .is_some_and(|retry| retry.retry_at > now)
                {
                    continue;
                }
                let index = current
                    .iter()
                    .position(|display| display.id == snapshot.id)
                    .unwrap_or(0);
                match self.start_display(snapshot, index) {
                    Ok(active) => {
                        self.retrying.remove(&snapshot.id);
                        self.active.insert(snapshot.id, active);
                        eprintln!(
                            "status=running component=frame_pipeline monitor={}",
                            snapshot.monitor_id()
                        );
                    }
                    Err(error) => {
                        let delay = match (error, self.retrying.get(&snapshot.id)) {
                            (CaptureError::AccessDenied, _) => ACCESS_DENIED_RETRY,
                            (_, Some(previous)) => (previous.delay * 2).min(MAXIMUM_RETRY),
                            (_, None) => FIRST_RETRY,
                        };
                        if self
                            .retrying
                            .get(&snapshot.id)
                            .is_none_or(|previous| previous.error != error)
                        {
                            eprintln!(
                                "status=degraded component=frame_pipeline monitor={} error={error}",
                                snapshot.monitor_id()
                            );
                        }
                        self.retrying.insert(
                            snapshot.id,
                            RetryingDisplay {
                                retry_at: now + delay,
                                delay,
                                error,
                            },
                        );
                    }
                }
            }
            self.displays = current;
        }

        fn start_display(
            &self,
            snapshot: &DisplaySnapshot,
            index: usize,
        ) -> Result<ActiveDisplay, CaptureError> {
            let stream = DisplayStream::start(snapshot)?;
            let (ocr_engine, ocr_initialized) = match self.ocr_bundle {
                Some(bundle) => match bundle.create_engine() {
                    Ok(engine) => (RuntimeOcrEngine::Active(engine), true),
                    Err(error) => {
                        eprintln!(
                            "status=degraded component=ocr_inference monitor={} error={}",
                            snapshot.monitor_id(),
                            error.kind()
                        );
                        (RuntimeOcrEngine::Unavailable, false)
                    }
                },
                None => (RuntimeOcrEngine::Unavailable, false),
            };
            let classifier = self
                .model
                .create_classifier()
                .map_err(|_| CaptureError::StreamFailed)?;
            let word_pack = self
                .word_pack_source
                .compile()
                .map_err(|_| CaptureError::StreamFailed)?;
            let mut consumer = ScheduledInferenceConsumer::new(
                classifier,
                ocr_engine,
                word_pack,
                AgentInferenceSink::new(
                    Some(Arc::clone(&self.client)),
                    self.recognition_policy.clone(),
                    monitor_name(index),
                    MacosSourceIdentityProvider::new(snapshot.bounds),
                ),
            );
            if !ocr_initialized {
                consumer.mark_ocr_unavailable();
            }
            let image_health = consumer.image_health_handle();
            let ocr_health = consumer.ocr_health_handle();
            let worker = DisplayWorker::start(stream.frames(), consumer)
                .map_err(|_| CaptureError::StreamFailed)?;
            Ok(ActiveDisplay {
                worker,
                stream,
                image_health,
                ocr_health,
            })
        }

        fn publish_health(&self) {
            let monitors = self
                .displays
                .iter()
                .enumerate()
                .map(|(index, snapshot)| self.monitor_health(snapshot, index))
                .collect();
            match self.client.publish_health(monitors) {
                Ok(snapshot) => {
                    self.apply_policy(&snapshot.policy);
                    println!(
                        "status=running component=service_ipc policy_revision={} protection_enabled={}",
                        snapshot.revision, snapshot.protection_enabled
                    );
                }
                Err(error) => eprintln!("status=degraded component=service_ipc error={error}"),
            }
            for (component, handles) in [
                (
                    "image_inference",
                    self.active
                        .values()
                        .map(|active| &active.image_health)
                        .collect::<Vec<_>>(),
                ),
                (
                    "ocr_inference",
                    self.active
                        .values()
                        .map(|active| &active.ocr_health)
                        .collect::<Vec<_>>(),
                ),
            ] {
                let (inferences, failures, latency_micros, unavailable) = handles.iter().fold(
                    (0_u64, 0_u64, 0_u64, 0_u64),
                    |(inferences, failures, latency, unavailable), health| {
                        let current = health.snapshot();
                        (
                            inferences.saturating_add(current.inferences()),
                            failures.saturating_add(current.failures()),
                            latency.saturating_add(current.total_latency_micros()),
                            unavailable.saturating_add(u64::from(!health.is_available())),
                        )
                    },
                );
                let unavailable = unavailable
                    .saturating_add(self.displays.len().saturating_sub(self.active.len()) as u64);
                println!(
                    "{}",
                    health_status_line(
                        component,
                        if unavailable == 0 {
                            "running"
                        } else {
                            "unavailable"
                        },
                        inferences,
                        failures,
                        latency_micros,
                        unavailable,
                    )
                );
            }
        }

        fn monitor_health(&self, snapshot: &DisplaySnapshot, index: usize) -> MonitorHealth {
            let width = snapshot
                .bounds
                .right
                .saturating_sub(snapshot.bounds.left)
                .max(1) as u32;
            let height = snapshot
                .bounds
                .bottom
                .saturating_sub(snapshot.bounds.top)
                .max(1) as u32;
            let Some(active) = self.active.get(&snapshot.id) else {
                return MonitorHealth {
                    monitor_id: snapshot.monitor_id(),
                    name: monitor_name(index),
                    width,
                    height,
                    frame_status: ComponentState::Unavailable,
                    image_status: ComponentState::Unavailable,
                    ocr_status: ComponentState::Unavailable,
                    image_inferences: 0,
                    ocr_inferences: 0,
                    latency_micros: 0,
                };
            };
            let image = active.image_health.snapshot();
            let ocr = active.ocr_health.snapshot();
            MonitorHealth {
                monitor_id: snapshot.monitor_id(),
                name: monitor_name(index),
                width,
                height,
                frame_status: active.worker.report().frame_state(),
                image_status: inference_state(
                    image.inferences(),
                    active.image_health.is_available(),
                ),
                ocr_status: inference_state(ocr.inferences(), active.ocr_health.is_available()),
                image_inferences: image.inferences(),
                ocr_inferences: ocr.inferences(),
                latency_micros: image.last_latency_micros().max(ocr.last_latency_micros()),
            }
        }
    }

    fn monitor_name(index: usize) -> String {
        format!("显示器 {}", index.saturating_add(1))
    }

    /// Development builds run outside KarmaAgent.app and never request the
    /// permission themselves.
    fn running_from_bundle() -> bool {
        install_directory().is_some()
    }
}

#[cfg(target_os = "macos")]
fn main() {
    agent::run();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("karma-agent-macos is supported only on macOS");
}
