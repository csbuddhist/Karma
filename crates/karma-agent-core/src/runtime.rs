//! Agent runtime configuration shared by the platform agents: the bundled OCR
//! word pack, OCR model selection, and the preflight that must finish before
//! any screen capture starts.

use karma_ai::{OcrEngine, OcrMatchSummary, PreparedFrame, WordPack};
use karma_domain::OcrRisk;
use karma_onnx::{OnnxOcrEngine, VerifiedOcrBundle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrRuntimeConfig {
    pub lightweight_manifest: std::path::PathBuf,
    pub accurate_manifest: Option<std::path::PathBuf>,
    pub profile: karma_ai::OcrModelProfile,
    pub preference: crate::ocr_profile::OcrProfilePreference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrRuntimeConfigError {
    MissingLightweightManifest,
    InvalidProfile,
}

const BUNDLED_WORD_PACK: &str = include_str!("../assets/ocr-word-pack.json");
const WORD_PACK_FORMAT_VERSION: u16 = 1;
const MAXIMUM_WORD_PACK_BYTES: usize = 64 * 1024;
const MAXIMUM_WORD_PACK_RULES: usize = 128;
const MAXIMUM_WORD_PACK_FIELD_CHARACTERS: usize = 256;
const WORD_PACK_CATEGORIES: [&str; 3] = ["adult_service", "explicit_term", "medical_education"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordPackSourceError {
    InvalidJson,
    InvalidRules,
}

#[derive(Clone, Copy)]
pub struct WordPackSource<'a> {
    json: &'a str,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WordPackDocument {
    format_version: u16,
    rules: Vec<WordPackRule>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WordPackRule {
    category: String,
    pattern: String,
    kind: WordPackRuleKind,
    risk: karma_domain::OcrRisk,
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum WordPackRuleKind {
    Literal,
    Regex,
    Exemption,
}

impl<'a> WordPackSource<'a> {
    pub fn bundled() -> Result<WordPackSource<'static>, WordPackSourceError> {
        WordPackSource::<'static>::validated(BUNDLED_WORD_PACK)
    }

    pub fn validated(json: &'a str) -> Result<Self, WordPackSourceError> {
        Self::parse_and_compile(json)?;
        Ok(Self { json })
    }

    pub fn compile(&self) -> Result<karma_ai::WordPack, WordPackSourceError> {
        Self::parse_and_compile(self.json)
    }

    fn parse_and_compile(json: &str) -> Result<karma_ai::WordPack, WordPackSourceError> {
        use karma_ai::{WordPack, WordRule};
        use karma_domain::OcrRisk;

        if json.len() > MAXIMUM_WORD_PACK_BYTES {
            return Err(WordPackSourceError::InvalidRules);
        }
        let document: WordPackDocument =
            serde_json::from_str(json).map_err(|_| WordPackSourceError::InvalidJson)?;
        if document.format_version != WORD_PACK_FORMAT_VERSION
            || document.rules.is_empty()
            || document.rules.len() > MAXIMUM_WORD_PACK_RULES
        {
            return Err(WordPackSourceError::InvalidRules);
        }
        let mut present_categories = std::collections::BTreeSet::new();
        let mut rules = Vec::with_capacity(document.rules.len());
        for rule in document.rules {
            if !WORD_PACK_CATEGORIES.contains(&rule.category.as_str())
                || rule.category.chars().count() > MAXIMUM_WORD_PACK_FIELD_CHARACTERS
                || rule.pattern.is_empty()
                || rule.pattern.chars().count() > MAXIMUM_WORD_PACK_FIELD_CHARACTERS
            {
                return Err(WordPackSourceError::InvalidRules);
            }
            present_categories.insert(rule.category.clone());
            rules.push(match (rule.category.as_str(), rule.kind, rule.risk) {
                (
                    "explicit_term",
                    WordPackRuleKind::Literal | WordPackRuleKind::Regex,
                    OcrRisk::HighRiskPhrase,
                ) => {
                    if matches!(rule.kind, WordPackRuleKind::Regex) {
                        WordRule::regex(&rule.category, &rule.pattern, rule.risk)
                    } else {
                        WordRule::literal(&rule.category, &rule.pattern, rule.risk)
                    }
                }
                (
                    "adult_service",
                    WordPackRuleKind::Literal | WordPackRuleKind::Regex,
                    OcrRisk::Keyword,
                ) => {
                    if matches!(rule.kind, WordPackRuleKind::Regex) {
                        WordRule::regex(&rule.category, &rule.pattern, rule.risk)
                    } else {
                        WordRule::literal(&rule.category, &rule.pattern, rule.risk)
                    }
                }
                ("medical_education", WordPackRuleKind::Exemption, OcrRisk::None) => {
                    WordRule::exemption(&rule.category, &rule.pattern)
                }
                _ => return Err(WordPackSourceError::InvalidRules),
            });
        }
        if present_categories.len() != WORD_PACK_CATEGORIES.len()
            || !WORD_PACK_CATEGORIES
                .iter()
                .all(|category| present_categories.contains(*category))
        {
            return Err(WordPackSourceError::InvalidRules);
        }
        WordPack::compile(rules).map_err(|_| WordPackSourceError::InvalidRules)
    }
}

impl OcrRuntimeConfig {
    pub fn from_environment() -> Result<Self, OcrRuntimeConfigError> {
        Self::from_values(
            std::env::var("KARMA_OCR_LIGHTWEIGHT_MANIFEST")
                .ok()
                .as_deref(),
            std::env::var("KARMA_OCR_ACCURATE_MANIFEST").ok().as_deref(),
            std::env::var("KARMA_OCR_PROFILE").ok().as_deref(),
        )
    }

    pub fn from_values(
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
            Some(value) => crate::ocr_profile::OcrProfilePreference::from_str(value)
                .map_err(|_| OcrRuntimeConfigError::InvalidProfile)?,
            None => crate::ocr_profile::OcrProfilePreference::Auto,
        };
        let profile = match preference {
            crate::ocr_profile::OcrProfilePreference::Accurate => {
                karma_ai::OcrModelProfile::Accurate
            }
            crate::ocr_profile::OcrProfilePreference::Auto
            | crate::ocr_profile::OcrProfilePreference::Lightweight => {
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

pub enum OcrPreflight<T, E> {
    Ready(T),
    Unavailable(Option<E>),
}

impl<T, E> OcrPreflight<T, E> {
    #[cfg(test)]
    fn bundle(&self) -> Option<&T> {
        match self {
            Self::Ready(bundle) => Some(bundle),
            Self::Unavailable(_) => None,
        }
    }

    pub fn error(&self) -> Option<&E> {
        match self {
            Self::Ready(_) | Self::Unavailable(None) => None,
            Self::Unavailable(Some(error)) => Some(error),
        }
    }

    pub fn into_bundle(self) -> Option<T> {
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
pub fn start_after_ocr_preflight<T, E, R>(
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

pub fn health_status_line(
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

pub fn ocr_performance_warning(performance_budget_exceeded: bool) -> Option<&'static str> {
    performance_budget_exceeded.then_some(
        "status=warning component=ocr_profile profile=accurate \
         reason=performance_budget_exceeded",
    )
}

struct VerifiedOcrCandidateFactory<'a> {
    accurate: Option<&'a VerifiedOcrBundle>,
}

impl crate::ocr_profile::OcrCandidateFactory for VerifiedOcrCandidateFactory<'_> {
    type Engine = OnnxOcrEngine;

    fn create_accurate(&self) -> crate::ocr_profile::AccurateCandidate<Self::Engine> {
        match self.accurate {
            Some(bundle) => match bundle.create_engine() {
                Ok(engine) => crate::ocr_profile::AccurateCandidate::Ready(engine),
                Err(_) => crate::ocr_profile::AccurateCandidate::Rejected,
            },
            None => crate::ocr_profile::AccurateCandidate::Missing,
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

pub struct SelectedOcrBundle<'a> {
    pub bundle: &'a VerifiedOcrBundle,
    pub performance_budget_exceeded: bool,
}

/// Chooses the OCR bundle for this run. An accurate bundle is used only when
/// requested explicitly or when its benchmark, cached at `benchmark_cache`,
/// fits the performance budget on this machine.
pub fn select_ocr_bundle<'a>(
    config: &OcrRuntimeConfig,
    lightweight: &'a VerifiedOcrBundle,
    accurate: Option<&'a VerifiedOcrBundle>,
    active_display_count: usize,
    benchmark_cache: std::path::PathBuf,
) -> SelectedOcrBundle<'a> {
    if config.preference == crate::ocr_profile::OcrProfilePreference::Lightweight {
        return SelectedOcrBundle {
            bundle: lightweight,
            performance_budget_exceeded: false,
        };
    }
    let factory = VerifiedOcrCandidateFactory { accurate };
    let Some(accurate_bundle) = accurate else {
        return SelectedOcrBundle {
            bundle: lightweight,
            performance_budget_exceeded: false,
        };
    };
    let key = crate::ocr_profile::BenchmarkKey {
        profile: karma_ai::OcrModelProfile::Accurate,
        bundle_version: accurate_bundle.manifest().asset.version.clone(),
        cpu_architecture: std::env::consts::ARCH.into(),
        logical_cpu_cores: std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1),
        active_display_count,
    };
    let cache_path = benchmark_cache;
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut clock = crate::ocr_profile::SystemBenchmarkClock::new();
    match crate::ocr_profile::ProfileSelector::new(cache_path).select(
        config.preference,
        key,
        &factory,
        &mut clock,
    ) {
        Ok(selection) if selection.profile == karma_ai::OcrModelProfile::Accurate => {
            SelectedOcrBundle {
                bundle: accurate_bundle,
                performance_budget_exceeded: selection.performance_budget_exceeded,
            }
        }
        Ok(_) | Err(_) => SelectedOcrBundle {
            bundle: lightweight,
            performance_budget_exceeded: false,
        },
    }
}

pub enum RuntimeOcrEngine {
    Active(OnnxOcrEngine),
    Unavailable,
}

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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use karma_ai::OcrModelProfile;
    use karma_domain::OcrRisk;

    use super::{
        OcrPreflight, OcrRuntimeConfig, OcrRuntimeConfigError, WordPackSource, WordPackSourceError,
        health_status_line, ocr_performance_warning, start_after_ocr_preflight,
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

    #[test]
    fn bundled_word_pack_is_nonempty_and_classifies_configured_categories() {
        let source = WordPackSource::bundled().unwrap();
        let pack = source.compile().unwrap();

        let explicit = pack.classify(&["porn"]);
        assert_eq!(explicit.risk, OcrRisk::HighRiskPhrase);
        assert_eq!(explicit.categories, ["explicit_term"]);

        let service = pack.classify(&["成人服務"]);
        assert_eq!(service.risk, OcrRisk::Keyword);
        assert_eq!(service.categories, ["adult_service"]);

        let education = pack.classify(&["sexual health education"]);
        assert_eq!(education.risk, OcrRisk::None);
        assert_eq!(education.categories, ["medical_education"]);
        assert!(education.exemption_context);
    }

    #[test]
    fn invalid_word_pack_sources_fail_before_capture() {
        for (source, expected) in [
            ("{", WordPackSourceError::InvalidJson),
            (
                r#"{"format_version":1,"rules":[]}"#,
                WordPackSourceError::InvalidRules,
            ),
            (
                r#"{"format_version":1,"rules":[],"unexpected":true}"#,
                WordPackSourceError::InvalidJson,
            ),
        ] {
            let capture_started = std::cell::Cell::new(false);
            let result = WordPackSource::validated(source).and_then(|word_pack| {
                capture_started.set(true);
                word_pack.compile()
            });
            assert!(matches!(result, Err(error) if error == expected));
            assert!(!capture_started.get());
        }
    }

    #[test]
    fn word_pack_category_semantics_cannot_be_inverted() {
        for source in [
            r#"{"format_version":1,"rules":[
                {"category":"explicit_term","pattern":"x","kind":"exemption","risk":"none"},
                {"category":"adult_service","pattern":"x","kind":"literal","risk":"keyword"},
                {"category":"medical_education","pattern":"x","kind":"exemption","risk":"none"}
            ]}"#,
            r#"{"format_version":1,"rules":[
                {"category":"explicit_term","pattern":"x","kind":"literal","risk":"high_risk_phrase"},
                {"category":"adult_service","pattern":"x","kind":"literal","risk":"high_risk_phrase"},
                {"category":"medical_education","pattern":"x","kind":"exemption","risk":"none"}
            ]}"#,
            r#"{"format_version":1,"rules":[
                {"category":"explicit_term","pattern":"x","kind":"literal","risk":"high_risk_phrase"},
                {"category":"adult_service","pattern":"x","kind":"literal","risk":"keyword"},
                {"category":"medical_education","pattern":"x","kind":"literal","risk":"keyword"}
            ]}"#,
        ] {
            assert_eq!(
                WordPackSource::validated(source).err(),
                Some(WordPackSourceError::InvalidRules)
            );
        }
    }

    #[test]
    fn explicit_accurate_over_budget_has_a_stable_privacy_safe_warning() {
        assert_eq!(
            ocr_performance_warning(true),
            Some(
                "status=warning component=ocr_profile profile=accurate \
                 reason=performance_budget_exceeded"
            )
        );
        assert_eq!(ocr_performance_warning(false), None);
    }
}
