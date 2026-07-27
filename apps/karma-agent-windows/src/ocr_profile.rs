use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use karma_ai::{
    BgraFrame, FrameDimensions, FramePreparer, OcrEngine, OcrMatchSummary, OcrModelProfile,
    WordPack, WordRule,
};
use karma_domain::{MonitorId, OcrRisk};
use serde::{Deserialize, Serialize};

const BENCHMARK_WARMUPS: usize = 3;
const BENCHMARK_ITERATIONS: usize = 10;
const PERFORMANCE_BUDGET_MILLIS: u64 = 800;
const MAX_CACHE_BYTES: u64 = 16 * 1024;
const TEMP_FILE_ATTEMPTS: usize = 128;

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub const APPROVED_BENCHMARK_STRINGS: &[&str] =
    &["System Status", "Ready", "简体中文", "繁體中文", "English"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrProfilePreference {
    Auto,
    Lightweight,
    Accurate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OcrProfilePreferenceParseError;

impl fmt::Display for OcrProfilePreferenceParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid OCR profile preference")
    }
}

impl std::error::Error for OcrProfilePreferenceParseError {}

impl FromStr for OcrProfilePreference {
    type Err = OcrProfilePreferenceParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "lightweight" => Ok(Self::Lightweight),
            "accurate" => Ok(Self::Accurate),
            _ => Err(OcrProfilePreferenceParseError),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkKey {
    pub profile: OcrModelProfile,
    pub bundle_version: String,
    pub cpu_architecture: String,
    pub logical_cpu_cores: usize,
    pub active_display_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BenchmarkResult {
    pub p95_millis: u64,
    pub success: bool,
    pub resource_limit_reached: bool,
    pub reference_summary_matches: bool,
    pub performance_budget_exceeded: bool,
}

impl BenchmarkResult {
    fn accepts_automatic_selection(self) -> bool {
        self.success
            && !self.resource_limit_reached
            && self.reference_summary_matches
            && !self.performance_budget_exceeded
    }

    fn accepts_explicit_accurate_selection(self) -> bool {
        self.success && !self.resource_limit_reached && self.reference_summary_matches
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSelection {
    pub profile: OcrModelProfile,
    pub download_required: bool,
    pub performance_budget_exceeded: bool,
    pub benchmark: Option<BenchmarkResult>,
}

pub trait BenchmarkClock {
    fn now(&mut self) -> Duration;
}

pub struct SystemBenchmarkClock {
    started: Instant,
}

impl SystemBenchmarkClock {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

impl Default for SystemBenchmarkClock {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchmarkClock for SystemBenchmarkClock {
    fn now(&mut self) -> Duration {
        self.started.elapsed()
    }
}

pub trait OcrCandidateFactory {
    type Engine: OcrEngine;

    /// Produces a verified accurate engine, reports an absent optional bundle, or rejects a
    /// candidate whose runtime/reference contract could not be established.
    fn create_accurate(&self) -> AccurateCandidate<Self::Engine>;

    /// Compares an in-memory benchmark result with the verified bundle reference.
    /// Implementations must not retain the summary.
    fn reference_summary_matches(&self, summary: &OcrMatchSummary) -> bool;
}

pub enum AccurateCandidate<E> {
    Ready(E),
    Missing,
    Rejected,
}

pub struct ProfileSelector {
    cache_path: PathBuf,
}

impl ProfileSelector {
    pub fn new(cache_path: impl Into<PathBuf>) -> Self {
        Self {
            cache_path: cache_path.into(),
        }
    }

    pub fn select<F, C>(
        &self,
        preference: OcrProfilePreference,
        key: BenchmarkKey,
        factory: &F,
        clock: &mut C,
    ) -> Result<ProfileSelection, ProfileSelectionError>
    where
        F: OcrCandidateFactory,
        C: BenchmarkClock,
    {
        if preference == OcrProfilePreference::Lightweight {
            return Ok(lightweight_selection(false, None));
        }

        let mut engine = match factory.create_accurate() {
            AccurateCandidate::Ready(engine) => engine,
            AccurateCandidate::Missing => {
                return Ok(lightweight_selection(
                    preference == OcrProfilePreference::Accurate,
                    None,
                ));
            }
            AccurateCandidate::Rejected => return Ok(lightweight_selection(false, None)),
        };

        let benchmark = self.read_cache(&key).unwrap_or(None).unwrap_or_else(|| {
            let measured = benchmark_accurate(&mut engine, factory, clock);
            let selected = if measured.accepts_automatic_selection() {
                OcrModelProfile::Accurate
            } else {
                OcrModelProfile::Lightweight
            };
            let _ = self.write_cache(&BenchmarkCache {
                key: key.clone(),
                selected_profile: selected,
                p95_millis: measured.p95_millis,
                success: measured.success,
                resource_limit_reached: measured.resource_limit_reached,
                reference_summary_matches: measured.reference_summary_matches,
                performance_budget_exceeded: measured.performance_budget_exceeded,
            });
            measured
        });

        let profile = match preference {
            OcrProfilePreference::Auto if benchmark.accepts_automatic_selection() => {
                OcrModelProfile::Accurate
            }
            OcrProfilePreference::Accurate if benchmark.accepts_explicit_accurate_selection() => {
                OcrModelProfile::Accurate
            }
            OcrProfilePreference::Auto | OcrProfilePreference::Accurate => {
                OcrModelProfile::Lightweight
            }
            OcrProfilePreference::Lightweight => unreachable!("handled before benchmarking"),
        };
        Ok(ProfileSelection {
            profile,
            download_required: false,
            performance_budget_exceeded: profile == OcrModelProfile::Accurate
                && benchmark.performance_budget_exceeded,
            benchmark: Some(benchmark),
        })
    }

    fn read_cache(
        &self,
        expected_key: &BenchmarkKey,
    ) -> Result<Option<BenchmarkResult>, ProfileSelectionError> {
        let mut file = match File::open(&self.cache_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(ProfileSelectionError::CacheUnavailable),
        };
        let mut bytes = Vec::with_capacity(MAX_CACHE_BYTES as usize + 1);
        Read::by_ref(&mut file)
            .take(MAX_CACHE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ProfileSelectionError::CacheUnavailable)?;
        if bytes.len() > MAX_CACHE_BYTES as usize {
            return Err(ProfileSelectionError::CacheInvalid);
        }
        let cache: BenchmarkCache =
            serde_json::from_slice(&bytes).map_err(|_| ProfileSelectionError::CacheInvalid)?;
        if cache.key != *expected_key {
            return Ok(None);
        }
        Ok(Some(BenchmarkResult {
            p95_millis: cache.p95_millis,
            success: cache.success,
            resource_limit_reached: cache.resource_limit_reached,
            reference_summary_matches: cache.reference_summary_matches,
            performance_budget_exceeded: cache.performance_budget_exceeded,
        }))
    }

    fn write_cache(&self, cache: &BenchmarkCache) -> Result<(), ProfileSelectionError> {
        let bytes = serde_json::to_vec(cache).map_err(|_| ProfileSelectionError::CacheInvalid)?;
        if bytes.len() > MAX_CACHE_BYTES as usize {
            return Err(ProfileSelectionError::CacheInvalid);
        }
        let parent = self
            .cache_path
            .parent()
            .ok_or(ProfileSelectionError::CacheUnavailable)?;
        let file_name = self
            .cache_path
            .file_name()
            .ok_or(ProfileSelectionError::CacheUnavailable)?;
        let (temporary, mut file) = create_cache_temp(parent, file_name)?;
        let write_result = (|| {
            file.write_all(&bytes)
                .map_err(|_| ProfileSelectionError::CacheUnavailable)?;
            file.sync_all()
                .map_err(|_| ProfileSelectionError::CacheUnavailable)?;
            drop(file);
            replace_cache_file(&temporary, &self.cache_path)
                .map_err(|_| ProfileSelectionError::CacheUnavailable)
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result
    }
}

fn create_cache_temp(
    parent: &std::path::Path,
    file_name: &std::ffi::OsStr,
) -> Result<(PathBuf, File), ProfileSelectionError> {
    for _ in 0..TEMP_FILE_ATTEMPTS {
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".{}.{}.{}.tmp",
            file_name.to_string_lossy(),
            std::process::id(),
            sequence
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(ProfileSelectionError::CacheUnavailable),
        }
    }
    Err(ProfileSelectionError::CacheUnavailable)
}

#[cfg(not(windows))]
fn replace_cache_file(
    temporary: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    fs::rename(temporary, destination)
}

#[cfg(windows)]
fn replace_cache_file(
    temporary: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    use std::{iter, os::windows::ffi::OsStrExt, ptr};

    const REPLACEFILE_WRITE_THROUGH: u32 = 0x0000_0001;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn ReplaceFileW(
            replaced_file_name: *const u16,
            replacement_file_name: *const u16,
            backup_file_name: *const u16,
            replace_flags: u32,
            exclude: *mut core::ffi::c_void,
            reserved: *mut core::ffi::c_void,
        ) -> i32;
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let temporary_wide: Vec<u16> = temporary
        .as_os_str()
        .encode_wide()
        .chain(iter::once(0))
        .collect();
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(iter::once(0))
        .collect();
    // ReplaceFileW provides replacement semantics when the destination exists. If it was
    // concurrently removed, MoveFileExW atomically installs the same-directory temporary file.
    let replaced = unsafe {
        ReplaceFileW(
            destination_wide.as_ptr(),
            temporary_wide.as_ptr(),
            ptr::null(),
            REPLACEFILE_WRITE_THROUGH,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    if replaced != 0 {
        return Ok(());
    }
    let replace_error = std::io::Error::last_os_error();
    if !matches!(replace_error.raw_os_error(), Some(2 | 3)) {
        return Err(replace_error);
    }
    let moved = unsafe {
        MoveFileExW(
            temporary_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSelectionError {
    CacheUnavailable,
    CacheInvalid,
}

impl fmt::Display for ProfileSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::CacheUnavailable => "OCR profile cache is unavailable",
            Self::CacheInvalid => "OCR profile cache is invalid",
        };
        formatter.write_str(value)
    }
}

impl std::error::Error for ProfileSelectionError {}

fn lightweight_selection(
    download_required: bool,
    benchmark: Option<BenchmarkResult>,
) -> ProfileSelection {
    ProfileSelection {
        profile: OcrModelProfile::Lightweight,
        download_required,
        performance_budget_exceeded: false,
        benchmark,
    }
}

fn benchmark_accurate<E, F, C>(engine: &mut E, factory: &F, clock: &mut C) -> BenchmarkResult
where
    E: OcrEngine,
    F: OcrCandidateFactory<Engine = E>,
    C: BenchmarkClock,
{
    let fixture = benchmark_fixture();
    let word_pack = benchmark_word_pack();
    let initial_resource_limit_events = engine.resource_limit_events();
    let mut success = true;
    for _ in 0..BENCHMARK_WARMUPS {
        if engine.classify(fixture.frame(), &word_pack).is_err() {
            success = false;
        }
    }

    let mut durations = Vec::with_capacity(BENCHMARK_ITERATIONS);
    let mut resource_limit_reached = engine.resource_limit_events() > initial_resource_limit_events;
    let mut reference_summary_matches = true;
    for _ in 0..BENCHMARK_ITERATIONS {
        let before_limits = engine.resource_limit_events();
        let started = clock.now();
        let output = engine.classify(fixture.frame(), &word_pack);
        let elapsed = clock.now().saturating_sub(started);
        durations.push(duration_millis(elapsed));
        resource_limit_reached |= engine.resource_limit_events() > before_limits;
        match output {
            Ok(summary) => reference_summary_matches &= factory.reference_summary_matches(&summary),
            Err(_) => success = false,
        }
    }
    durations.sort_unstable();
    let p95_millis = durations[BENCHMARK_ITERATIONS - 1];
    BenchmarkResult {
        p95_millis,
        success,
        resource_limit_reached,
        reference_summary_matches,
        performance_budget_exceeded: p95_millis > PERFORMANCE_BUDGET_MILLIS,
    }
}

fn duration_millis(value: Duration) -> u64 {
    value.as_millis().min(u128::from(u64::MAX)) as u64
}

fn benchmark_word_pack() -> WordPack {
    WordPack::compile(
        APPROVED_BENCHMARK_STRINGS
            .iter()
            .enumerate()
            .map(|(index, text)| {
                WordRule::literal(&format!("benchmark_ui_{index}"), text, OcrRisk::Keyword)
            })
            .collect(),
    )
    .expect("fixed benchmark word pack is valid")
}

struct BenchmarkFixture {
    frame: karma_ai::PreparedFrame,
}

impl BenchmarkFixture {
    fn dimensions(&self) -> (u32, u32) {
        (
            self.frame.dimensions().width(),
            self.frame.dimensions().height(),
        )
    }

    fn strings(&self) -> &'static [&'static str] {
        APPROVED_BENCHMARK_STRINGS
    }

    fn frame(&self) -> &karma_ai::PreparedFrame {
        &self.frame
    }
}

fn benchmark_fixture() -> BenchmarkFixture {
    let dimensions = FrameDimensions::new(640, 360).expect("benchmark dimensions are fixed");
    let mut pixels = vec![
        0_u8;
        dimensions
            .tight_byte_len()
            .expect("benchmark buffer is bounded")
    ];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[32, 32, 32, 255]);
    }
    for (index, text) in APPROVED_BENCHMARK_STRINGS.iter().enumerate() {
        draw_fixed_text(&mut pixels, dimensions, 32, 28 + index * 60, text);
    }
    let frame = BgraFrame::new(
        MonitorId("benchmark".into()),
        0,
        dimensions,
        dimensions
            .tight_stride()
            .expect("benchmark stride is bounded"),
        pixels,
    )
    .expect("benchmark pixels match dimensions");
    BenchmarkFixture {
        frame: FramePreparer::default()
            .prepare(frame)
            .expect("benchmark frame is valid"),
    }
}

fn draw_fixed_text(pixels: &mut [u8], dimensions: FrameDimensions, x: usize, y: usize, text: &str) {
    let mut cursor = x;
    for character in text.chars() {
        let rows = glyph_rows(character);
        let scale = if character.is_ascii() { 2 } else { 1 };
        for (row, pattern) in rows.iter().enumerate() {
            for (column, bit) in pattern.bytes().enumerate() {
                if bit != b'1' {
                    continue;
                }
                for y_scale in 0..scale {
                    for x_scale in 0..scale {
                        let pixel_x = cursor + column * scale + x_scale;
                        let pixel_y = y + row * scale + y_scale;
                        if pixel_x >= dimensions.width() as usize
                            || pixel_y >= dimensions.height() as usize
                        {
                            continue;
                        }
                        let offset = pixel_y * dimensions.tight_stride().expect("fixed stride")
                            + pixel_x * 4;
                        pixels[offset..offset + 4].copy_from_slice(&[232, 232, 232, 255]);
                    }
                }
            }
        }
        cursor += rows.first().map_or(0, |row| row.len()) * scale + scale * 2;
    }
}

fn glyph_rows(character: char) -> &'static [&'static str] {
    match character {
        'A' => &[
            "01110", "10001", "10001", "11111", "10001", "10001", "10001",
        ],
        'D' => &[
            "11110", "10001", "10001", "10001", "10001", "10001", "11110",
        ],
        'E' => &[
            "11111", "10000", "10000", "11110", "10000", "10000", "11111",
        ],
        'G' => &[
            "01110", "10001", "10000", "10111", "10001", "10001", "01110",
        ],
        'H' => &[
            "10001", "10001", "10001", "11111", "10001", "10001", "10001",
        ],
        'I' => &[
            "11111", "00100", "00100", "00100", "00100", "00100", "11111",
        ],
        'L' => &[
            "10000", "10000", "10000", "10000", "10000", "10000", "11111",
        ],
        'M' => &[
            "10001", "11011", "10101", "10101", "10001", "10001", "10001",
        ],
        'N' => &[
            "10001", "11001", "10101", "10011", "10001", "10001", "10001",
        ],
        'R' => &[
            "11110", "10001", "10001", "11110", "10100", "10010", "10001",
        ],
        'S' => &[
            "01111", "10000", "10000", "01110", "00001", "00001", "11110",
        ],
        'T' => &[
            "11111", "00100", "00100", "00100", "00100", "00100", "00100",
        ],
        'U' => &[
            "10001", "10001", "10001", "10001", "10001", "10001", "01110",
        ],
        'Y' => &[
            "10001", "10001", "01010", "00100", "00100", "00100", "00100",
        ],
        'a' => &[
            "00000", "01110", "00001", "01111", "10001", "10011", "01101",
        ],
        'd' => &[
            "00001", "00001", "01111", "10001", "10001", "10011", "01101",
        ],
        'e' => &[
            "00000", "01110", "10001", "11111", "10000", "10001", "01110",
        ],
        'g' => &[
            "00000", "01101", "10011", "10001", "01111", "00001", "01110",
        ],
        'h' => &[
            "10000", "10000", "10110", "11001", "10001", "10001", "10001",
        ],
        'i' => &[
            "00100", "00000", "01100", "00100", "00100", "00100", "01110",
        ],
        'l' => &[
            "01100", "00100", "00100", "00100", "00100", "00100", "01110",
        ],
        'm' => &[
            "00000", "11010", "10101", "10101", "10101", "10101", "10101",
        ],
        'n' => &[
            "00000", "10110", "11001", "10001", "10001", "10001", "10001",
        ],
        'r' => &[
            "00000", "10110", "11001", "10000", "10000", "10000", "10000",
        ],
        's' => &[
            "00000", "01111", "10000", "01110", "00001", "00001", "11110",
        ],
        't' => &[
            "00100", "00100", "11111", "00100", "00100", "00101", "00010",
        ],
        'u' => &[
            "00000", "10001", "10001", "10001", "10001", "10011", "01101",
        ],
        'y' => &[
            "00000", "10001", "10001", "10011", "01101", "00001", "01110",
        ],
        ' ' => &["000"],
        '简' => &[
            "01000100010",
            "11101110111",
            "00000000000",
            "00111111100",
            "00100000100",
            "00111111100",
            "00100000100",
            "00111111100",
            "00010001000",
            "00100000100",
            "01000000010",
            "00011111000",
            "00000000000",
        ],
        '体' => &[
            "00100000000",
            "00100111110",
            "01100001000",
            "00100011110",
            "11111101000",
            "00100001000",
            "00100011110",
            "00100001000",
            "00100001000",
            "00100001000",
            "01110011110",
            "00000000000",
            "00000000000",
        ],
        '中' => &[
            "00111111100",
            "00100000100",
            "00100000100",
            "00100000100",
            "00111111100",
            "00000100000",
            "00000100000",
            "00000100000",
            "00000100000",
            "00000100000",
            "00000100000",
            "00000000000",
            "00000000000",
        ],
        '文' => &[
            "00011111000",
            "00000100000",
            "00000100000",
            "00111111100",
            "00001000000",
            "00010010000",
            "00100001000",
            "01000000100",
            "00000000000",
            "00000000000",
            "00000000000",
            "00000000000",
            "00000000000",
        ],
        '繁' => &[
            "00001111000",
            "11111111111",
            "00100100100",
            "00111111100",
            "00100100100",
            "11111111111",
            "00001000000",
            "01111111110",
            "00001000000",
            "00101010100",
            "01010101010",
            "10001000100",
            "00000000000",
        ],
        '體' => &[
            "00100000000",
            "00101111110",
            "01100010000",
            "00101111110",
            "11111010000",
            "00100111110",
            "00100100100",
            "00101111110",
            "00100001000",
            "00100111110",
            "01110100100",
            "00000000000",
            "00000000000",
        ],
        _ => &[],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BenchmarkCache {
    key: BenchmarkKey,
    selected_profile: OcrModelProfile,
    p95_millis: u64,
    success: bool,
    resource_limit_reached: bool,
    reference_summary_matches: bool,
    performance_budget_exceeded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lightweight_preference_always_selects_the_bundled_profile() {
        let selector = ProfileSelector::new("unused.json");
        let mut clock = FakeClock::default();

        let selected = selector
            .select(
                OcrProfilePreference::Lightweight,
                key("accurate-v1"),
                &FakeFactory::accurate(),
                &mut clock,
            )
            .unwrap();

        assert_eq!(selected.profile, OcrModelProfile::Lightweight);
        assert!(!selected.download_required);
        assert_eq!(clock.calls, 0);
    }

    #[test]
    fn missing_explicit_accurate_profile_falls_back_and_requests_download() {
        let selector = ProfileSelector::new("unused.json");
        let mut clock = FakeClock::default();

        let selected = selector
            .select(
                OcrProfilePreference::Accurate,
                key("accurate-v1"),
                &FakeFactory::missing(),
                &mut clock,
            )
            .unwrap();

        assert_eq!(selected.profile, OcrModelProfile::Lightweight);
        assert!(selected.download_required);
        assert!(!selected.performance_budget_exceeded);
    }

    #[test]
    fn preference_parser_accepts_only_documented_values() {
        assert_eq!("auto".parse(), Ok(OcrProfilePreference::Auto));
        assert_eq!("lightweight".parse(), Ok(OcrProfilePreference::Lightweight));
        assert_eq!("accurate".parse(), Ok(OcrProfilePreference::Accurate));
        assert_eq!(
            "accurate "
                .parse::<OcrProfilePreference>()
                .unwrap_err()
                .to_string(),
            "invalid OCR profile preference"
        );
    }

    #[test]
    fn explicit_accurate_overrides_only_the_performance_budget() {
        let directory = tempfile::tempdir().unwrap();
        let selector = ProfileSelector::new(directory.path().join("profile.json"));
        let mut clock = FakeClock::with_millis([900; 10]);

        let selected = selector
            .select(
                OcrProfilePreference::Accurate,
                key("accurate-v1"),
                &FakeFactory::accurate(),
                &mut clock,
            )
            .unwrap();

        assert_eq!(selected.profile, OcrModelProfile::Accurate);
        assert!(selected.performance_budget_exceeded);
    }

    #[test]
    fn explicit_accurate_never_overrides_a_reference_failure() {
        let directory = tempfile::tempdir().unwrap();
        let selector = ProfileSelector::new(directory.path().join("profile.json"));
        let mut clock = FakeClock::with_millis([100; 10]);

        let selected = selector
            .select(
                OcrProfilePreference::Accurate,
                key("accurate-v1"),
                &FakeFactory::reference_mismatch(),
                &mut clock,
            )
            .unwrap();

        assert_eq!(selected.profile, OcrModelProfile::Lightweight);
    }

    #[test]
    fn explicit_accurate_never_overrides_a_runtime_or_resource_failure() {
        let directory = tempfile::tempdir().unwrap();
        let selector = ProfileSelector::new(directory.path().join("profile.json"));

        for (index, factory) in [
            FakeFactory::runtime_failure(),
            FakeFactory::resource_limited(),
        ]
        .into_iter()
        .enumerate()
        {
            let mut clock = FakeClock::with_millis([100; 10]);
            let selected = selector
                .select(
                    OcrProfilePreference::Accurate,
                    key(&format!("accurate-v{index}")),
                    &factory,
                    &mut clock,
                )
                .unwrap();
            assert_eq!(selected.profile, OcrModelProfile::Lightweight);
        }
    }

    #[test]
    fn accurate_selection_rejects_a_resource_limit_reached_during_warmup() {
        let directory = tempfile::tempdir().unwrap();
        let selector = ProfileSelector::new(directory.path().join("profile.json"));
        let mut clock = FakeClock::with_millis([100; 10]);

        let selected = selector
            .select(
                OcrProfilePreference::Auto,
                key("accurate-v1"),
                &FakeFactory::warmup_resource_limited(),
                &mut clock,
            )
            .unwrap();

        assert_eq!(selected.profile, OcrModelProfile::Lightweight);
        assert!(selected.benchmark.unwrap().resource_limit_reached);
    }

    #[test]
    fn accurate_selection_rejects_a_runtime_failure_during_warmup() {
        let directory = tempfile::tempdir().unwrap();
        let selector = ProfileSelector::new(directory.path().join("profile.json"));
        let mut clock = FakeClock::with_millis([100; 10]);

        let selected = selector
            .select(
                OcrProfilePreference::Auto,
                key("accurate-v1"),
                &FakeFactory::warmup_failure(),
                &mut clock,
            )
            .unwrap();

        assert_eq!(selected.profile, OcrModelProfile::Lightweight);
        assert!(!selected.benchmark.unwrap().success);
    }

    #[test]
    fn rejected_accurate_candidate_falls_back_without_requesting_a_download() {
        let selector = ProfileSelector::new("unused.json");
        let mut clock = FakeClock::default();

        let selected = selector
            .select(
                OcrProfilePreference::Accurate,
                key("accurate-v1"),
                &FakeFactory::rejected(),
                &mut clock,
            )
            .unwrap();

        assert_eq!(selected.profile, OcrModelProfile::Lightweight);
        assert!(!selected.download_required);
    }

    #[test]
    fn auto_selects_accurate_only_after_ten_successes_without_limits_or_reference_failure() {
        let directory = tempfile::tempdir().unwrap();
        let selector = ProfileSelector::new(directory.path().join("profile.json"));
        let mut clock = FakeClock::with_millis([100; 10]);

        let selected = selector
            .select(
                OcrProfilePreference::Auto,
                key("accurate-v1"),
                &FakeFactory::accurate(),
                &mut clock,
            )
            .unwrap();

        assert_eq!(selected.profile, OcrModelProfile::Accurate);
        assert_eq!(selected.benchmark.unwrap().p95_millis, 100);
    }

    #[test]
    fn auto_uses_sorted_tenth_sample_as_p95_and_rejects_over_budget_accuracy() {
        let directory = tempfile::tempdir().unwrap();
        let selector = ProfileSelector::new(directory.path().join("profile.json"));
        let mut clock = FakeClock::with_millis([10, 20, 30, 40, 50, 60, 70, 80, 800, 801]);

        let selected = selector
            .select(
                OcrProfilePreference::Auto,
                key("accurate-v1"),
                &FakeFactory::accurate(),
                &mut clock,
            )
            .unwrap();

        let benchmark = selected.benchmark.unwrap();
        assert_eq!(benchmark.p95_millis, 801);
        assert!(benchmark.performance_budget_exceeded);
        assert_eq!(selected.profile, OcrModelProfile::Lightweight);
    }

    #[test]
    fn cache_key_changes_invalidate_a_saved_benchmark() {
        let directory = tempfile::tempdir().unwrap();
        let selector = ProfileSelector::new(directory.path().join("profile.json"));
        let mut first_clock = FakeClock::with_millis([100; 10]);
        selector
            .select(
                OcrProfilePreference::Auto,
                key("accurate-v1"),
                &FakeFactory::accurate(),
                &mut first_clock,
            )
            .unwrap();

        for changed in [
            BenchmarkKey {
                bundle_version: "accurate-v2".into(),
                ..key("accurate-v1")
            },
            BenchmarkKey {
                cpu_architecture: "aarch64".into(),
                ..key("accurate-v1")
            },
            BenchmarkKey {
                logical_cpu_cores: 12,
                ..key("accurate-v1")
            },
            BenchmarkKey {
                active_display_count: 2,
                ..key("accurate-v1")
            },
        ] {
            let mut clock = FakeClock::with_millis([100; 10]);
            selector
                .select(
                    OcrProfilePreference::Auto,
                    changed,
                    &FakeFactory::accurate(),
                    &mut clock,
                )
                .unwrap();
            assert_eq!(clock.calls, 20);
        }
    }

    #[test]
    fn cache_is_strict_and_contains_only_the_approved_privacy_safe_fields() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("profile.json");
        let selector = ProfileSelector::new(&path);
        let mut clock = FakeClock::with_millis([100; 10]);
        selector
            .select(
                OcrProfilePreference::Auto,
                key("accurate-v1"),
                &FakeFactory::accurate(),
                &mut clock,
            )
            .unwrap();

        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            value
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec![
                "key",
                "p95_millis",
                "performance_budget_exceeded",
                "reference_summary_matches",
                "resource_limit_reached",
                "selected_profile",
                "success",
            ]
        );
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("categories")
        );
        let mut object = value.as_object().unwrap().clone();
        object.insert("unexpected".into(), serde_json::Value::Bool(true));
        std::fs::write(&path, serde_json::to_vec(&object).unwrap()).unwrap();
        assert_eq!(
            selector.read_cache(&key("accurate-v1")),
            Err(ProfileSelectionError::CacheInvalid)
        );
    }

    #[test]
    fn benchmark_fixture_only_uses_approved_fixed_ui_strings() {
        let fixture = benchmark_fixture();
        let spec: BenchmarkFixtureSpec =
            serde_json::from_str(include_str!("../tests/fixtures/ocr-benchmark-spec.json"))
                .unwrap();

        assert_eq!(fixture.dimensions(), (640, 360));
        assert_eq!(fixture.strings(), APPROVED_BENCHMARK_STRINGS);
        assert_eq!(spec.width, 640);
        assert_eq!(spec.height, 360);
        assert_eq!(spec.strings, APPROVED_BENCHMARK_STRINGS);
        assert_eq!(fixture.frame().dimensions().width(), 640);
        assert_eq!(fixture.frame().dimensions().height(), 360);
    }

    #[test]
    fn benchmark_word_pack_matches_only_the_fixed_disclosed_ui_strings() {
        let pack = benchmark_word_pack();
        let summary = pack.classify(APPROVED_BENCHMARK_STRINGS);

        assert_eq!(summary.risk, OcrRisk::Keyword);
        assert_eq!(summary.categories.len(), APPROVED_BENCHMARK_STRINGS.len());
        assert!(
            summary
                .categories
                .iter()
                .all(|category| category.starts_with("benchmark_ui_"))
        );
    }

    #[test]
    fn benchmark_renderer_has_a_fixed_glyph_for_every_approved_character() {
        for character in APPROVED_BENCHMARK_STRINGS
            .iter()
            .flat_map(|text| text.chars())
        {
            assert!(
                !glyph_rows(character).is_empty(),
                "missing glyph for {character}"
            );
        }
    }

    #[test]
    fn benchmark_renderer_preserves_title_case_glyphs() {
        assert_ne!(glyph_rows('S'), glyph_rows('s'));
        assert_ne!(glyph_rows('R'), glyph_rows('r'));
        assert_ne!(glyph_rows('E'), glyph_rows('e'));
    }

    #[test]
    fn oversized_cache_is_rejected_without_deserializing_its_contents() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("profile.json");
        std::fs::write(&path, vec![b'x'; MAX_CACHE_BYTES as usize + 1]).unwrap();
        let selector = ProfileSelector::new(path);

        assert_eq!(
            selector.read_cache(&key("accurate-v1")),
            Err(ProfileSelectionError::CacheInvalid)
        );
    }

    #[test]
    fn cache_replacement_overwrites_an_existing_destination_from_the_same_directory() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("profile.json");
        std::fs::write(&destination, b"old").unwrap();
        let (temporary, mut file) =
            create_cache_temp(directory.path(), destination.file_name().unwrap()).unwrap();
        file.write_all(b"new").unwrap();
        file.sync_all().unwrap();
        drop(file);

        replace_cache_file(&temporary, &destination).unwrap();

        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        assert!(!temporary.exists());
    }

    #[test]
    fn cache_temp_creation_is_unique_within_one_process() {
        let directory = tempfile::tempdir().unwrap();
        let name = std::ffi::OsStr::new("profile.json");
        let (first_path, first_file) = create_cache_temp(directory.path(), name).unwrap();
        let (second_path, second_file) = create_cache_temp(directory.path(), name).unwrap();

        assert_ne!(first_path, second_path);
        drop(first_file);
        drop(second_file);
        std::fs::remove_file(first_path).unwrap();
        std::fs::remove_file(second_path).unwrap();
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct BenchmarkFixtureSpec {
        width: u32,
        height: u32,
        strings: Vec<String>,
    }

    fn key(bundle_version: &str) -> BenchmarkKey {
        BenchmarkKey {
            profile: OcrModelProfile::Accurate,
            bundle_version: bundle_version.into(),
            cpu_architecture: "x86_64".into(),
            logical_cpu_cores: 8,
            active_display_count: 1,
        }
    }

    #[derive(Default)]
    struct FakeClock {
        values: Vec<u64>,
        calls: usize,
    }

    impl FakeClock {
        fn with_millis(values: [u64; BENCHMARK_ITERATIONS]) -> Self {
            Self {
                values: values.to_vec(),
                calls: 0,
            }
        }
    }

    impl BenchmarkClock for FakeClock {
        fn now(&mut self) -> Duration {
            let call = self.calls;
            self.calls += 1;
            if call % 2 == 0 {
                Duration::ZERO
            } else {
                Duration::from_millis(self.values[call / 2])
            }
        }
    }

    enum FakeFactory {
        Accurate,
        Missing,
        Rejected,
        ReferenceMismatch,
        RuntimeFailure,
        ResourceLimited,
        WarmupResourceLimited,
        WarmupFailure,
    }

    impl FakeFactory {
        fn accurate() -> Self {
            Self::Accurate
        }

        fn missing() -> Self {
            Self::Missing
        }

        fn rejected() -> Self {
            Self::Rejected
        }

        fn reference_mismatch() -> Self {
            Self::ReferenceMismatch
        }

        fn runtime_failure() -> Self {
            Self::RuntimeFailure
        }

        fn resource_limited() -> Self {
            Self::ResourceLimited
        }

        fn warmup_resource_limited() -> Self {
            Self::WarmupResourceLimited
        }

        fn warmup_failure() -> Self {
            Self::WarmupFailure
        }
    }

    enum FakeEngine {
        Accurate,
        RuntimeFailure,
        ResourceLimited {
            resource_limit_events: u64,
        },
        WarmupResourceLimited {
            classifications: usize,
            resource_limit_events: u64,
        },
        WarmupFailure {
            classifications: usize,
        },
    }

    impl OcrEngine for FakeEngine {
        type Error = ();

        fn classify(
            &mut self,
            _frame: &karma_ai::PreparedFrame,
            _word_pack: &WordPack,
        ) -> Result<OcrMatchSummary, Self::Error> {
            if matches!(self, Self::RuntimeFailure) {
                return Err(());
            }
            if let Self::WarmupFailure { classifications } = self {
                *classifications += 1;
                if *classifications <= BENCHMARK_WARMUPS {
                    return Err(());
                }
            }
            if let Self::ResourceLimited {
                resource_limit_events,
            } = self
            {
                *resource_limit_events += 1;
            }
            if let Self::WarmupResourceLimited {
                classifications,
                resource_limit_events,
            } = self
            {
                *classifications += 1;
                if *classifications <= BENCHMARK_WARMUPS {
                    *resource_limit_events += 1;
                }
            }
            Ok(OcrMatchSummary {
                risk: karma_domain::OcrRisk::None,
                categories: Vec::new(),
                exemption_context: false,
            })
        }

        fn resource_limit_events(&self) -> u64 {
            match self {
                Self::ResourceLimited {
                    resource_limit_events,
                } => *resource_limit_events,
                Self::WarmupResourceLimited {
                    resource_limit_events,
                    ..
                } => *resource_limit_events,
                Self::Accurate | Self::RuntimeFailure | Self::WarmupFailure { .. } => 0,
            }
        }
    }

    impl OcrCandidateFactory for FakeFactory {
        type Engine = FakeEngine;

        fn create_accurate(&self) -> AccurateCandidate<Self::Engine> {
            match self {
                Self::Missing => AccurateCandidate::Missing,
                Self::Rejected => AccurateCandidate::Rejected,
                Self::Accurate | Self::ReferenceMismatch => {
                    AccurateCandidate::Ready(FakeEngine::Accurate)
                }
                Self::RuntimeFailure => AccurateCandidate::Ready(FakeEngine::RuntimeFailure),
                Self::ResourceLimited => AccurateCandidate::Ready(FakeEngine::ResourceLimited {
                    resource_limit_events: 0,
                }),
                Self::WarmupResourceLimited => {
                    AccurateCandidate::Ready(FakeEngine::WarmupResourceLimited {
                        classifications: 0,
                        resource_limit_events: 0,
                    })
                }
                Self::WarmupFailure => {
                    AccurateCandidate::Ready(FakeEngine::WarmupFailure { classifications: 0 })
                }
            }
        }

        fn reference_summary_matches(&self, _summary: &OcrMatchSummary) -> bool {
            !matches!(self, Self::ReferenceMismatch)
        }
    }
}
