//! The agent side of the service protocol, shared by the platform agents:
//! heartbeats and policy snapshots, image observations and evidence, and the
//! foreground-context monitor. Platform agents supply the transport, the
//! source window identity, and the foreground window.

use std::{
    collections::HashSet,
    fmt::Write as _,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{SyncSender, sync_channel},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use image::{ExtendedColorType, codecs::jpeg::JpegEncoder};
use karma_ai::{ImageInference, OcrMatchSummary, PreparedFrame};
use karma_ipc::{
    AgentContextObservation, AgentEnforcement, AgentHeartbeat, AgentObservation, ClientKind,
    ComponentState, EvidenceSubmission, MAX_WINDOW_TITLE_CHARS, MonitorHealth, ProcessIdentity,
    RequestEnvelope, ResponseEnvelope, ServiceRequest, ServiceResult,
};
use karma_policy::{
    ApplicationEffect, ApplicationPolicy, ContextPolicy, normalize_executable, title_digest,
};
use rand::{RngCore, rngs::OsRng};
use serde_json::Value;
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use crate::inference::{OcrSummarySink, should_capture_evidence};

const IPC_TIMEOUT_MS: u32 = 1500;
const EVIDENCE_COOLDOWN_MS: i64 = 5000;
const DISPOSITION_COOLDOWN_MS: i64 = 5000;
const CONTEXT_POLL_INTERVAL: Duration = Duration::from_millis(750);
const CONTEXT_RETRY_COOLDOWN_MS: i64 = 5000;
const JPEG_QUALITY: u8 = 80;

/// Sends one request to the local service and waits for its response.
pub type Transport = fn(&RequestEnvelope, u32) -> Result<ResponseEnvelope, ()>;

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
    transport: Transport,
    request_sequence: AtomicU64,
    /// Bans and remembered titles from the latest policy fetch.
    enforcement: RwLock<Arc<LearnedEnforcement>>,
}

/// `AgentEnforcement` indexed for the foreground poll.
#[derive(Debug, Default)]
struct LearnedEnforcement {
    banned_executables: HashSet<String>,
    remembered_title_digests: HashSet<String>,
}

impl From<AgentEnforcement> for LearnedEnforcement {
    fn from(enforcement: AgentEnforcement) -> Self {
        Self {
            banned_executables: enforcement.banned_executables.into_iter().collect(),
            remembered_title_digests: enforcement.remembered_title_digests.into_iter().collect(),
        }
    }
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

#[derive(Clone, Default)]
pub struct ContextPolicyHandle(Arc<RwLock<ContextPolicy>>);

impl ContextPolicyHandle {
    pub fn update(&self, policy: &Value) {
        if let Ok(policy) = ContextPolicy::from_value(policy) {
            *self
                .0
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = policy;
        }
    }

    fn snapshot(&self) -> ContextPolicy {
        self.0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[derive(Clone, Default)]
pub struct ApplicationPolicyHandle(Arc<RwLock<ApplicationPolicy>>);

impl ApplicationPolicyHandle {
    pub fn update(&self, policy: &Value) {
        if let Ok(policy) = ApplicationPolicy::from_value(policy) {
            *self
                .0
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = policy;
        }
    }

    fn snapshot(&self) -> ApplicationPolicy {
        self.0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

/// The process that owns the window an image frame is attributed to.
#[derive(Clone)]
pub struct SourceContext {
    pub identity: ProcessIdentity,
    /// The platform's window identifier, for later lookups such as the
    /// browser address bar.
    pub window_handle: isize,
    /// Title of the attributed window when the frame was prepared.
    pub window_title: String,
}

/// Attributes one monitor's frames to the window covering it.
pub trait SourceIdentityProvider: Send + 'static {
    /// The reliably attributed source window and process, if any.
    fn snapshot(&mut self) -> Option<SourceContext>;

    /// The canonical host a supported browser window shows.
    fn browser_host(&mut self, source: &SourceContext) -> Option<String>;
}

pub struct AgentInferenceSink<P> {
    agent_instance_id: Option<String>,
    observation_sender: Option<SyncSender<AgentObservation>>,
    evidence_sender: Option<SyncSender<EvidenceFrame>>,
    recognition_policy: RecognitionPolicyHandle,
    source: P,
    monitor_name: String,
    last_disposition_at_ms: i64,
    last_evidence_at_ms: i64,
    ocr_summaries: u64,
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

impl<P: SourceIdentityProvider> AgentInferenceSink<P> {
    pub fn new(
        client: Option<Arc<AgentServiceClient>>,
        recognition_policy: RecognitionPolicyHandle,
        monitor_name: String,
        source: P,
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
            source,
            monitor_name,
            last_disposition_at_ms: i64::MIN,
            last_evidence_at_ms: i64::MIN,
            ocr_summaries: 0,
        }
    }
}

impl<P: SourceIdentityProvider> OcrSummarySink for AgentInferenceSink<P> {
    type ImageContext = Option<SourceContext>;

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
                    |value| value.identity.executable_name.clone(),
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
            browser_host: self.source.browser_host(&source),
            source: Some(source.identity),
            evidence_pending,
            window_title: sendable_title(source.window_title),
        };
        if sender.try_send(observation).is_ok() {
            self.last_disposition_at_ms = frame.captured_at_ms();
        }
    }
}

impl AgentServiceClient {
    /// A client that authenticates with `token` and reports `user_sid` as
    /// the signed-in user.
    pub fn new(token: Zeroizing<String>, user_sid: String, transport: Transport) -> Self {
        Self {
            token,
            instance_id: random_opaque(),
            user_sid,
            transport,
            request_sequence: AtomicU64::new(1),
            enforcement: RwLock::new(Arc::default()),
        }
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub fn publish_health(
        &self,
        monitors: Vec<MonitorHealth>,
    ) -> Result<PolicySnapshot, AgentServiceError> {
        let sent_at_ms = unix_time_ms();
        let heartbeat = AgentHeartbeat {
            agent_instance_id: self.instance_id.clone(),
            user_sid: self.user_sid.clone(),
            process_id: std::process::id(),
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
            ServiceResult::AgentPolicy {
                revision,
                policy,
                enforcement,
            } => {
                *self
                    .enforcement
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    Arc::new(enforcement.into());
                Ok(PolicySnapshot {
                    revision,
                    protection_enabled: policy
                        .get("protectionEnabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    policy,
                })
            }
            _ => Err(AgentServiceError::InvalidResponse),
        }
    }

    fn enforcement(&self) -> Arc<LearnedEnforcement> {
        Arc::clone(
            &self
                .enforcement
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    }

    fn submit_observation(&self, observation: AgentObservation) -> Result<(), AgentServiceError> {
        let result = self.request(ServiceRequest::AgentObservation {
            agent_token: self.token.to_string(),
            observation,
        })?;
        self.after_submission(result)
    }

    fn submit_context_observation(
        &self,
        observation: AgentContextObservation,
    ) -> Result<(), AgentServiceError> {
        let result = self.request(ServiceRequest::AgentContextObservation {
            agent_token: self.token.to_string(),
            observation,
        })?;
        self.after_submission(result)
    }

    /// A completed closure may have remembered a title or started a ban;
    /// refresh enforcement now instead of waiting for the next heartbeat.
    fn after_submission(&self, result: ServiceResult) -> Result<(), AgentServiceError> {
        match result {
            ServiceResult::DispositionCompleted { .. } => {
                let _ = self.fetch_policy();
                Ok(())
            }
            ServiceResult::Acknowledged => Ok(()),
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
        let response = (self.transport)(&request, IPC_TIMEOUT_MS)
            .map_err(|_| AgentServiceError::ServiceUnavailable)?;
        response.result.map_err(|failure| match failure.code {
            karma_ipc::ServiceErrorCode::AgentUnauthorized => AgentServiceError::AgentRejected,
            _ => AgentServiceError::ServiceUnavailable,
        })
    }
}

/// Reads the foreground window on the context monitor's thread.
pub trait ForegroundContext {
    /// The foreground window's process, title, and browser host, if it has a
    /// title or host to evaluate.
    fn observe(&mut self, agent_instance_id: &str) -> Option<AgentContextObservation>;
}

/// Polls the foreground window and submits it whenever policy, a ban, or a
/// remembered title says it must be closed. `create` runs on the monitor
/// thread, so platforms can initialize thread-bound state there; returning
/// `None` ends the monitor.
pub fn start_context_monitor<C: ForegroundContext>(
    client: Arc<AgentServiceClient>,
    policy: ContextPolicyHandle,
    applications: ApplicationPolicyHandle,
    create: impl FnOnce() -> Option<C> + Send + 'static,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("karma-foreground-context".into())
        .spawn(move || {
            let Some(mut foreground) = create() else {
                return;
            };
            let mut last_fingerprint = None;
            let mut last_submitted_at_ms = i64::MIN;
            loop {
                thread::sleep(CONTEXT_POLL_INTERVAL);
                let Some(observation) = foreground.observe(&client.instance_id) else {
                    continue;
                };
                let context_policy = policy.snapshot();
                let effect = applications
                    .snapshot()
                    .effect_for(&observation.source.executable_name);
                let verdict = context_policy.evaluate(
                    observation.browser_host.as_deref(),
                    &observation.window_title,
                );
                if !learned_enforcement_applies(
                    &client.enforcement(),
                    &context_policy,
                    effect,
                    &observation,
                ) && !karma_policy::context_enforcement(verdict, effect)
                {
                    continue;
                }
                let fingerprint = context_fingerprint(&observation);
                if last_fingerprint == Some(fingerprint)
                    && observation
                        .occurred_at_ms
                        .saturating_sub(last_submitted_at_ms)
                        < CONTEXT_RETRY_COOLDOWN_MS
                {
                    continue;
                }
                if client.submit_context_observation(observation).is_ok() {
                    last_fingerprint = Some(fingerprint);
                    last_submitted_at_ms = unix_time_ms();
                }
            }
        })
        .expect("failed to start foreground context monitor")
}

/// Mirrors the service: banned executables are closed unconditionally, and
/// remembered titles yield to allowlisted hosts and allowed applications.
fn learned_enforcement_applies(
    enforcement: &LearnedEnforcement,
    context_policy: &ContextPolicy,
    effect: ApplicationEffect,
    observation: &AgentContextObservation,
) -> bool {
    if enforcement.banned_executables.is_empty() && enforcement.remembered_title_digests.is_empty()
    {
        return false;
    }
    let executable = normalize_executable(&observation.source.executable_name);
    if enforcement.banned_executables.contains(&executable) {
        return true;
    }
    if context_policy.allows_host(observation.browser_host.as_deref())
        || effect == ApplicationEffect::Allow
    {
        return false;
    }
    title_digest(&executable, &observation.window_title)
        .is_some_and(|digest| enforcement.remembered_title_digests.contains(&digest))
}

/// The title to report with an image observation, or `None` when it would
/// fail protocol validation and cost the observation itself.
fn sendable_title(title: String) -> Option<String> {
    (!title.is_empty()
        && title.chars().count() <= MAX_WINDOW_TITLE_CHARS
        && !title
            .chars()
            .any(|character| character.is_control() && !character.is_whitespace()))
    .then_some(title)
}

fn context_fingerprint(observation: &AgentContextObservation) -> u64 {
    let mut hasher = DefaultHasher::new();
    observation.source.process_id.hash(&mut hasher);
    observation.source.started_at_ms.hash(&mut hasher);
    observation.window_title.hash(&mut hasher);
    observation.browser_host.hash(&mut hasher);
    hasher.finish()
}

/// The health state of one monitor's image or OCR inference.
pub fn inference_state(inferences: u64, available: bool) -> ComponentState {
    if !available {
        ComponentState::Unavailable
    } else if inferences == 0 {
        ComponentState::Starting
    } else {
        ComponentState::Healthy
    }
}

/// A random 48-character identifier for events and requests.
pub fn random_opaque() -> String {
    let mut bytes = [0_u8; 24];
    OsRng.fill_bytes(&mut bytes);
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut value, "{byte:02x}").expect("writing to a String cannot fail");
    }
    value
}

pub fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn observation(executable: &str, title: &str, host: Option<&str>) -> AgentContextObservation {
        AgentContextObservation {
            event_id: "context-1".into(),
            agent_instance_id: "agent-1".into(),
            occurred_at_ms: 1,
            source: ProcessIdentity {
                process_id: 42,
                started_at_ms: 1,
                executable_name: executable.into(),
                executable_sha256: None,
            },
            window_title: title.into(),
            browser_host: host.map(str::to_owned),
        }
    }

    fn enforcement() -> LearnedEnforcement {
        AgentEnforcement {
            banned_executables: vec![r"d:\games\game.exe".into()],
            remembered_title_digests: vec![
                title_digest(r"c:\browser\chrome.exe", "Hot Video - Google Chrome").unwrap(),
            ],
        }
        .into()
    }

    #[test]
    fn banned_executables_and_remembered_titles_are_submitted() {
        let context = ContextPolicy::default();
        assert!(learned_enforcement_applies(
            &enforcement(),
            &context,
            ApplicationEffect::Allow,
            &observation(r"D:\Games\Game.exe", "Main menu", None),
        ));
        assert!(learned_enforcement_applies(
            &enforcement(),
            &context,
            ApplicationEffect::ContentOnly,
            &observation(r"C:\Browser\chrome.exe", "Hot  Video - Google Chrome", None),
        ));
        assert!(!learned_enforcement_applies(
            &enforcement(),
            &context,
            ApplicationEffect::ContentOnly,
            &observation(r"C:\Browser\chrome.exe", "Homework - Google Chrome", None),
        ));
        assert!(!learned_enforcement_applies(
            &enforcement(),
            &context,
            ApplicationEffect::Allow,
            &observation(r"C:\Browser\chrome.exe", "Hot Video - Google Chrome", None),
        ));
        let allowlist = ContextPolicy::from_value(&json!({
            "websites": [{"id":"a","pattern":"safe.example","action":"allow","enabled":true}]
        }))
        .unwrap();
        assert!(!learned_enforcement_applies(
            &enforcement(),
            &allowlist,
            ApplicationEffect::ContentOnly,
            &observation(
                r"C:\Browser\chrome.exe",
                "Hot Video - Google Chrome",
                Some("safe.example")
            ),
        ));
    }

    #[test]
    fn only_protocol_valid_titles_accompany_image_observations() {
        assert_eq!(sendable_title(String::new()), None);
        assert_eq!(sendable_title("bad\u{7}title".into()), None);
        assert_eq!(sendable_title("x".repeat(MAX_WINDOW_TITLE_CHARS + 1)), None);
        assert_eq!(
            sendable_title("视频 - Google Chrome".into()).as_deref(),
            Some("视频 - Google Chrome")
        );
    }

    #[test]
    fn requests_travel_through_the_platform_transport() {
        fn reject(_: &RequestEnvelope, _: u32) -> Result<ResponseEnvelope, ()> {
            Err(())
        }
        fn unauthorized(request: &RequestEnvelope, _: u32) -> Result<ResponseEnvelope, ()> {
            Ok(ResponseEnvelope::failure(
                request.request_id.clone(),
                karma_ipc::ServiceFailure::new(karma_ipc::ServiceErrorCode::AgentUnauthorized),
            ))
        }
        let offline =
            AgentServiceClient::new(Zeroizing::new("token".into()), "user".into(), reject);
        assert!(matches!(
            offline.fetch_policy(),
            Err(AgentServiceError::ServiceUnavailable)
        ));
        let rejected =
            AgentServiceClient::new(Zeroizing::new("token".into()), "user".into(), unauthorized);
        assert!(matches!(
            rejected.fetch_policy(),
            Err(AgentServiceError::AgentRejected)
        ));
    }
}
