use std::{
    env,
    fmt::Write as _,
    mem::size_of,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{SyncSender, sync_channel},
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use image::{ExtendedColorType, codecs::jpeg::JpegEncoder};
use karma_ai::{ImageInference, OcrMatchSummary, PreparedFrame};
use karma_ipc::{
    AgentHeartbeat, AgentObservation, ClientKind, ComponentState, EvidenceSubmission,
    MonitorHealth, ProcessIdentity, RequestEnvelope, ServiceRequest, ServiceResult,
};
use karma_windows::{
    AttributionResult, FrameWorkerReport, FrameWorkerStatus, MonitorSnapshot, Rect,
    SourceAttributor, WindowCandidate, foreground_window, inspect_process,
};
use karma_windows_ipc::send_request;
use rand::{RngCore, rngs::OsRng};
use serde_json::Value;
use thiserror::Error;
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree},
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY, TOKEN_USER,
            TokenUser,
        },
        System::Threading::{GetCurrentProcess, GetCurrentProcessId, OpenProcessToken},
    },
    core::PWSTR,
};
use zeroize::{Zeroize, Zeroizing};

use crate::inference_consumer::{OcrSummarySink, should_capture_evidence};

const AGENT_TOKEN_ENV: &str = "KARMA_AGENT_TOKEN";
const IPC_TIMEOUT_MS: u32 = 1500;
const EVIDENCE_COOLDOWN_MS: i64 = 5000;
const DISPOSITION_COOLDOWN_MS: i64 = 5000;
const JPEG_QUALITY: u8 = 80;

#[derive(Debug, Error)]
pub enum AgentServiceError {
    #[error("agent identity is unavailable")]
    IdentityUnavailable,
    #[error("service connection is unavailable")]
    ServiceUnavailable,
    #[error("service rejected the agent")]
    AgentRejected,
    #[error("service returned an invalid response")]
    InvalidResponse,
}

pub struct AgentServiceClient {
    token: Zeroizing<String>,
    instance_id: String,
    user_sid: String,
    request_sequence: AtomicU64,
}

pub struct PolicySnapshot {
    pub revision: u64,
    pub protection_enabled: bool,
    pub policy: Value,
}

#[derive(Debug, Clone, Copy)]
struct RecognitionPolicy {
    protection_enabled: bool,
    image_enabled: bool,
    evidence_enabled: bool,
    threshold_millis: u16,
}

impl Default for RecognitionPolicy {
    fn default() -> Self {
        Self {
            protection_enabled: true,
            image_enabled: true,
            evidence_enabled: false,
            threshold_millis: 820,
        }
    }
}

#[derive(Clone, Default)]
pub struct RecognitionPolicyHandle(Arc<RwLock<RecognitionPolicy>>);

impl RecognitionPolicyHandle {
    pub fn update(&self, policy: &Value) {
        let protection_enabled = policy
            .get("protectionEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let recognition = policy.get("recognition");
        let image_enabled = recognition
            .and_then(|value| value.get("imageEnabled"))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let evidence_enabled = recognition
            .and_then(|value| value.get("evidenceEnabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let threshold = recognition
            .and_then(|value| {
                value
                    .get("sensitivity")
                    .or_else(|| value.get("immediateThreshold"))
            })
            .and_then(Value::as_u64)
            .unwrap_or(82)
            .min(100) as u16
            * 10;
        *self
            .0
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = RecognitionPolicy {
            protection_enabled,
            image_enabled,
            evidence_enabled,
            threshold_millis: threshold,
        };
    }

    fn snapshot(&self) -> RecognitionPolicy {
        *self
            .0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub struct AgentInferenceSink {
    agent_instance_id: Option<String>,
    observation_sender: Option<SyncSender<AgentObservation>>,
    evidence_sender: Option<SyncSender<EvidenceFrame>>,
    recognition_policy: RecognitionPolicyHandle,
    source: WindowsSourceIdentityProvider,
    monitor_name: String,
    last_disposition_at_ms: i64,
    last_evidence_at_ms: i64,
    ocr_summaries: u64,
}

struct WindowsSourceIdentityProvider {
    monitor_bounds: Rect,
    cached_process: Option<ProcessIdentity>,
}

impl WindowsSourceIdentityProvider {
    fn snapshot(&mut self) -> Option<ProcessIdentity> {
        let foreground = foreground_window().ok().flatten()?;
        let candidate = WindowCandidate {
            handle: foreground.handle,
            pid: foreground.pid,
            bounds: foreground.bounds,
        };
        let AttributionResult::Reliable(attributed) =
            SourceAttributor::resolve(self.monitor_bounds, Some(foreground.pid), &[candidate])
        else {
            return None;
        };
        if self
            .cached_process
            .as_ref()
            .is_some_and(|process| process.process_id == attributed.pid)
        {
            return self.cached_process.clone();
        }
        let process = inspect_process(attributed.pid).ok().flatten()?;
        let identity = ProcessIdentity {
            process_id: process.process_id,
            started_at_ms: process.started_at_ms,
            executable_name: process.executable_name,
            executable_sha256: None,
        };
        self.cached_process = Some(identity.clone());
        Some(identity)
    }
}

struct EvidenceFrame {
    evidence_id: String,
    captured_at_ms: i64,
    monitor_name: String,
    application_name: String,
    risk_millis: u16,
    width: u32,
    height: u32,
    bgra: Zeroizing<Vec<u8>>,
}

impl AgentInferenceSink {
    pub fn new(
        client: Option<Arc<AgentServiceClient>>,
        recognition_policy: RecognitionPolicyHandle,
        monitor_name: String,
        monitor_bounds: Rect,
    ) -> Self {
        let agent_instance_id = client.as_ref().map(|client| client.instance_id.clone());
        let (observation_sender, evidence_sender) = match client {
            Some(client) => {
                let (observation_sender, observation_receiver) =
                    sync_channel::<AgentObservation>(4);
                let observation_client = Arc::clone(&client);
                thread::Builder::new()
                    .name("karma-observation-submit".into())
                    .spawn(move || {
                        while let Ok(observation) = observation_receiver.recv() {
                            let _ = observation_client.submit_observation(observation);
                        }
                    })
                    .expect("failed to start observation submitter");
                let (evidence_sender, evidence_receiver) = sync_channel::<EvidenceFrame>(1);
                thread::Builder::new()
                    .name("karma-evidence-submit".into())
                    .spawn(move || {
                        while let Ok(frame) = evidence_receiver.recv() {
                            let _ = client.submit_evidence(frame);
                        }
                    })
                    .expect("failed to start evidence submitter");
                (Some(observation_sender), Some(evidence_sender))
            }
            None => (None, None),
        };
        Self {
            agent_instance_id,
            observation_sender,
            evidence_sender,
            recognition_policy,
            source: WindowsSourceIdentityProvider {
                monitor_bounds,
                cached_process: None,
            },
            monitor_name,
            last_disposition_at_ms: i64::MIN,
            last_evidence_at_ms: i64::MIN,
            ocr_summaries: 0,
        }
    }
}

impl OcrSummarySink for AgentInferenceSink {
    type ImageContext = Option<ProcessIdentity>;

    fn consume(&mut self, _summary: OcrMatchSummary) {
        self.ocr_summaries = self.ocr_summaries.saturating_add(1);
    }

    fn image_recognition_enabled(&self) -> bool {
        self.recognition_policy.snapshot().image_enabled
    }

    fn prepare_image(&mut self) -> Self::ImageContext {
        self.recognition_policy
            .snapshot()
            .protection_enabled
            .then(|| self.source.snapshot())
            .flatten()
    }

    fn consume_image(
        &mut self,
        frame: &PreparedFrame,
        inference: &ImageInference,
        source: Self::ImageContext,
    ) {
        let policy = self.recognition_policy.snapshot();
        let occurred_at_ms = unix_time_ms();
        let evidence_pending = should_capture_evidence(
            policy.protection_enabled && policy.image_enabled && policy.evidence_enabled,
            policy.threshold_millis,
            inference.score_millis,
            frame.captured_at_ms(),
            self.last_evidence_at_ms,
            EVIDENCE_COOLDOWN_MS,
        ) && self.evidence_sender.as_ref().is_some_and(|sender| {
            let dimensions = frame.dimensions();
            let evidence = EvidenceFrame {
                evidence_id: format!("evidence-{}", random_opaque()),
                captured_at_ms: occurred_at_ms,
                monitor_name: self.monitor_name.clone(),
                application_name: source.as_ref().map_or_else(
                    || "来源应用无法可靠归属".into(),
                    |value| value.executable_name.clone(),
                ),
                risk_millis: inference.score_millis,
                width: dimensions.width(),
                height: dimensions.height(),
                bgra: Zeroizing::new(frame.pixels().to_vec()),
            };
            sender.try_send(evidence).is_ok()
        });
        if evidence_pending {
            self.last_evidence_at_ms = frame.captured_at_ms();
        }

        if !should_capture_evidence(
            policy.protection_enabled && policy.image_enabled,
            policy.threshold_millis,
            inference.score_millis,
            frame.captured_at_ms(),
            self.last_disposition_at_ms,
            DISPOSITION_COOLDOWN_MS,
        ) {
            return;
        }
        let (Some(sender), Some(agent_instance_id), Some(source)) =
            (&self.observation_sender, &self.agent_instance_id, source)
        else {
            return;
        };
        let observation = AgentObservation {
            event_id: format!("event-{}", random_opaque()),
            agent_instance_id: agent_instance_id.clone(),
            occurred_at_ms,
            monitor_id: frame.monitor_id().0.clone(),
            risk_millis: inference.score_millis,
            reason_code: "image_immediate".into(),
            source: Some(source),
            evidence_pending,
        };
        if sender.try_send(observation).is_ok() {
            self.last_disposition_at_ms = frame.captured_at_ms();
        }
    }
}

impl AgentServiceClient {
    pub fn from_environment() -> Result<Option<Self>, AgentServiceError> {
        let Some(token) = env::var_os(AGENT_TOKEN_ENV) else {
            eprintln!("status=degraded component=service_ipc error=agent_token_missing");
            return Ok(None);
        };
        let token = token
            .into_string()
            .map_err(|_| AgentServiceError::IdentityUnavailable)?;
        if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(AgentServiceError::IdentityUnavailable);
        }
        Ok(Some(Self {
            token: Zeroizing::new(token),
            instance_id: random_opaque(),
            user_sid: current_user_sid()?,
            request_sequence: AtomicU64::new(1),
        }))
    }

    pub fn publish_health(
        &self,
        monitors: Vec<MonitorHealth>,
    ) -> Result<PolicySnapshot, AgentServiceError> {
        let sent_at_ms = unix_time_ms();
        let heartbeat = AgentHeartbeat {
            agent_instance_id: self.instance_id.clone(),
            user_sid: self.user_sid.clone(),
            process_id: unsafe { GetCurrentProcessId() },
            sent_at_ms,
            monitors,
        };
        match self.request(ServiceRequest::AgentHeartbeat {
            agent_token: self.token.to_string(),
            heartbeat,
        })? {
            ServiceResult::Acknowledged => {}
            _ => return Err(AgentServiceError::InvalidResponse),
        }
        self.fetch_policy()
    }

    pub fn fetch_policy(&self) -> Result<PolicySnapshot, AgentServiceError> {
        match self.request(ServiceRequest::GetAgentPolicy {
            agent_token: self.token.to_string(),
        })? {
            ServiceResult::AgentPolicy { revision, policy } => Ok(PolicySnapshot {
                revision,
                protection_enabled: policy
                    .get("protectionEnabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                policy,
            }),
            _ => Err(AgentServiceError::InvalidResponse),
        }
    }

    fn submit_observation(&self, observation: AgentObservation) -> Result<(), AgentServiceError> {
        match self.request(ServiceRequest::AgentObservation {
            agent_token: self.token.to_string(),
            observation,
        })? {
            ServiceResult::DispositionCompleted { .. } | ServiceResult::Acknowledged => Ok(()),
            _ => Err(AgentServiceError::InvalidResponse),
        }
    }

    fn submit_evidence(&self, mut frame: EvidenceFrame) -> Result<(), AgentServiceError> {
        let pixel_count = usize::try_from(frame.width)
            .ok()
            .and_then(|width| {
                usize::try_from(frame.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or(AgentServiceError::InvalidResponse)?;
        if frame.bgra.len() != pixel_count.saturating_mul(4) {
            return Err(AgentServiceError::InvalidResponse);
        }
        let mut rgb = Zeroizing::new(Vec::with_capacity(pixel_count.saturating_mul(3)));
        for pixel in frame.bgra.chunks_exact(4) {
            rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
        frame.bgra.zeroize();
        let mut jpeg = Zeroizing::new(Vec::new());
        JpegEncoder::new_with_quality(&mut *jpeg, JPEG_QUALITY)
            .encode(&rgb, frame.width, frame.height, ExtendedColorType::Rgb8)
            .map_err(|_| AgentServiceError::InvalidResponse)?;
        rgb.zeroize();
        let submission = EvidenceSubmission {
            evidence_id: frame.evidence_id,
            captured_at_ms: frame.captured_at_ms,
            monitor_name: frame.monitor_name,
            application_name: frame.application_name,
            reason_code: "image_immediate".into(),
            risk_millis: frame.risk_millis,
            media_type: "image/jpeg".into(),
            bytes_base64: BASE64.encode(&*jpeg),
        };
        jpeg.zeroize();
        match self.request(ServiceRequest::SubmitEvidence {
            agent_token: self.token.to_string(),
            evidence: submission,
        })? {
            ServiceResult::Acknowledged => Ok(()),
            _ => Err(AgentServiceError::InvalidResponse),
        }
    }

    fn request(&self, body: ServiceRequest) -> Result<ServiceResult, AgentServiceError> {
        let sequence = self.request_sequence.fetch_add(1, Ordering::Relaxed);
        let request = RequestEnvelope::new(
            format!("{}-{sequence}", self.instance_id),
            random_opaque(),
            ClientKind::Agent,
            body,
        );
        let response = send_request(&request, IPC_TIMEOUT_MS)
            .map_err(|_| AgentServiceError::ServiceUnavailable)?;
        response.result.map_err(|failure| match failure.code {
            karma_ipc::ServiceErrorCode::AgentUnauthorized => AgentServiceError::AgentRejected,
            _ => AgentServiceError::ServiceUnavailable,
        })
    }
}

pub fn monitor_health(
    monitor: &MonitorSnapshot,
    index: usize,
    worker: FrameWorkerReport,
    image: &crate::InferenceHealthHandle,
    ocr: &crate::InferenceHealthHandle,
) -> MonitorHealth {
    let image_snapshot = image.snapshot();
    let ocr_snapshot = ocr.snapshot();
    MonitorHealth {
        monitor_id: monitor.id.0.clone(),
        name: format!("显示器 {}", index.saturating_add(1)),
        width: monitor
            .bounds
            .right
            .saturating_sub(monitor.bounds.left)
            .max(1) as u32,
        height: monitor
            .bounds
            .bottom
            .saturating_sub(monitor.bounds.top)
            .max(1) as u32,
        frame_status: frame_state(worker.status()),
        image_status: inference_state(image_snapshot.inferences(), image.is_available()),
        ocr_status: inference_state(ocr_snapshot.inferences(), ocr.is_available()),
        image_inferences: image_snapshot.inferences(),
        ocr_inferences: ocr_snapshot.inferences(),
        latency_micros: image_snapshot
            .last_latency_micros()
            .max(ocr_snapshot.last_latency_micros()),
    }
}

fn frame_state(status: FrameWorkerStatus) -> ComponentState {
    match status {
        FrameWorkerStatus::Starting => ComponentState::Starting,
        FrameWorkerStatus::Running => ComponentState::Healthy,
        FrameWorkerStatus::Stopped | FrameWorkerStatus::TargetClosed => ComponentState::Stopped,
        FrameWorkerStatus::RecreateRequired
        | FrameWorkerStatus::DeviceLost
        | FrameWorkerStatus::AccessDenied
        | FrameWorkerStatus::Failed => ComponentState::Unavailable,
    }
}

fn inference_state(inferences: u64, available: bool) -> ComponentState {
    if !available {
        ComponentState::Unavailable
    } else if inferences == 0 {
        ComponentState::Starting
    } else {
        ComponentState::Healthy
    }
}

fn current_user_sid() -> Result<String, AgentServiceError> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .map_err(|_| AgentServiceError::IdentityUnavailable)?;
    let result = (|| {
        let mut needed = 0_u32;
        let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut needed) };
        if needed < size_of::<TOKEN_USER>() as u32 {
            return Err(AgentServiceError::IdentityUnavailable);
        }
        let words = (needed as usize).div_ceil(size_of::<usize>());
        let mut buffer = vec![0_usize; words];
        unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                Some(buffer.as_mut_ptr().cast()),
                needed,
                &mut needed,
            )
        }
        .map_err(|_| AgentServiceError::IdentityUnavailable)?;
        let token_user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
        let mut sid = PWSTR::null();
        unsafe { ConvertSidToStringSidW(token_user.User.Sid, &mut sid) }
            .map_err(|_| AgentServiceError::IdentityUnavailable)?;
        let converted =
            unsafe { sid.to_string() }.map_err(|_| AgentServiceError::IdentityUnavailable);
        unsafe {
            let _ = LocalFree(Some(HLOCAL(sid.0.cast())));
        }
        converted
    })();
    unsafe {
        let _ = CloseHandle(token);
    }
    result
}

fn random_opaque() -> String {
    let mut bytes = [0_u8; 24];
    OsRng.fill_bytes(&mut bytes);
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut value, "{byte:02x}").expect("writing to a String cannot fail");
    }
    value
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
