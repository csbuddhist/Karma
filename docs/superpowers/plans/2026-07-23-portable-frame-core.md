# Karma Portable Frame Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the privacy-safe, memory-bounded BGRA frame preparation layer that validates captured buffers, scales them to at most 640 pixels on the longest edge, computes a 64-bit difference hash, keeps only the newest pending frame, and feeds the existing per-monitor scheduler.

**Architecture:** New focused modules in `karma-ai` own frame values, deterministic scaling/fingerprinting, a capacity-one mailbox, and scheduler composition. Windows capture remains outside this plan and will supply validated BGRA buffers through these portable interfaces in the next plan.

**Tech Stack:** Rust 1.85+, `zeroize` 1.8, existing `karma-domain`, Cargo tests and Clippy.

## Global Constraints

- Input and output pixels use BGRA8.
- Default target longest edge is exactly 640 pixels; small images are never enlarged.
- Width, height, stride, and allocation calculations use checked arithmetic.
- Pixel-bearing values do not implement serde and their `Debug` output never includes pixels.
- Owned pixel buffers are zeroized on drop.
- No filesystem, network, Windows API, async runtime, ONNX Runtime, OCR engine, or UI dependency enters this plan.
- Every task follows RED → GREEN → focused commit.

---

### Task 1: Add validated privacy-safe BGRA frame values

**Files:**
- Modify: `crates/karma-ai/Cargo.toml`
- Modify: `crates/karma-ai/src/lib.rs`
- Create: `crates/karma-ai/src/frame.rs`

**Interfaces:**
- Produces: `FrameDimensions`, `BgraFrame`, `PreparedFrame`, `FrameError`.
- Consumes: `karma_domain::MonitorId`, `zeroize::Zeroizing<Vec<u8>>`.

- [ ] **Step 1: Add `zeroize`, exports, and failing tests**

Add `zeroize = "1.8"` to `karma-ai` dependencies. Export the four types from `lib.rs`. Create `frame.rs` with tests proving:

```rust
#[test]
fn dimensions_reject_zero_and_compute_tight_layout() {
    assert_eq!(FrameDimensions::new(0, 10), Err(FrameError::InvalidDimensions));
    let value = FrameDimensions::new(3, 2).unwrap();
    assert_eq!(value.tight_stride().unwrap(), 12);
    assert_eq!(value.tight_byte_len().unwrap(), 24);
}

#[test]
fn bgra_frame_validates_stride_and_buffer_length() {
    let dimensions = FrameDimensions::new(2, 2).unwrap();
    assert!(matches!(
        BgraFrame::new(MonitorId("m".into()), 1, dimensions, 7, vec![0; 14]),
        Err(FrameError::StrideTooSmall { .. })
    ));
    assert!(matches!(
        BgraFrame::new(MonitorId("m".into()), 1, dimensions, 8, vec![0; 15]),
        Err(FrameError::BufferLengthMismatch { .. })
    ));
}

#[test]
fn debug_redacts_owned_pixels() {
    let frame = BgraFrame::new(
        MonitorId("m".into()), 1, FrameDimensions::new(1, 1).unwrap(), 4,
        vec![11, 22, 33, 255],
    ).unwrap();
    let debug = format!("{frame:?}");
    assert!(debug.contains("pixel_bytes: 4"));
    assert!(!debug.contains("11"));
    assert!(!debug.contains("22"));
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-ai frame`

Expected: compilation fails because the frame types do not exist.

- [ ] **Step 3: Implement the values**

Implement:

```rust
use std::fmt;
use karma_domain::MonitorId;
use thiserror::Error;
use zeroize::Zeroizing;

const BYTES_PER_PIXEL: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameDimensions { width: u32, height: u32 }

impl FrameDimensions {
    pub fn new(width: u32, height: u32) -> Result<Self, FrameError> {
        if width == 0 || height == 0 { return Err(FrameError::InvalidDimensions); }
        Ok(Self { width, height })
    }
    pub fn width(self) -> u32 { self.width }
    pub fn height(self) -> u32 { self.height }
    pub fn tight_stride(self) -> Result<usize, FrameError> {
        usize::try_from(self.width).map_err(|_| FrameError::ArithmeticOverflow)?
            .checked_mul(BYTES_PER_PIXEL).ok_or(FrameError::ArithmeticOverflow)
    }
    pub fn tight_byte_len(self) -> Result<usize, FrameError> {
        self.tight_stride()?.checked_mul(
            usize::try_from(self.height).map_err(|_| FrameError::ArithmeticOverflow)?
        ).ok_or(FrameError::ArithmeticOverflow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FrameError {
    #[error("frame dimensions must be non-zero")] InvalidDimensions,
    #[error("frame arithmetic overflow")] ArithmeticOverflow,
    #[error("frame stride {actual} is smaller than {minimum}")]
    StrideTooSmall { minimum: usize, actual: usize },
    #[error("frame buffer length {actual} does not equal {expected}")]
    BufferLengthMismatch { expected: usize, actual: usize },
    #[error("maximum frame edge must be non-zero")] InvalidMaximumEdge,
}

pub struct BgraFrame {
    monitor_id: MonitorId, captured_at_ms: i64, dimensions: FrameDimensions,
    stride: usize, pixels: Zeroizing<Vec<u8>>,
}

impl BgraFrame {
    pub fn new(monitor_id: MonitorId, captured_at_ms: i64, dimensions: FrameDimensions,
               stride: usize, pixels: Vec<u8>) -> Result<Self, FrameError> {
        let minimum = dimensions.tight_stride()?;
        if stride < minimum { return Err(FrameError::StrideTooSmall { minimum, actual: stride }); }
        let expected = stride.checked_mul(usize::try_from(dimensions.height())
            .map_err(|_| FrameError::ArithmeticOverflow)?).ok_or(FrameError::ArithmeticOverflow)?;
        if pixels.len() != expected {
            return Err(FrameError::BufferLengthMismatch { expected, actual: pixels.len() });
        }
        Ok(Self { monitor_id, captured_at_ms, dimensions, stride, pixels: Zeroizing::new(pixels) })
    }
    pub fn monitor_id(&self) -> &MonitorId { &self.monitor_id }
    pub fn captured_at_ms(&self) -> i64 { self.captured_at_ms }
    pub fn dimensions(&self) -> FrameDimensions { self.dimensions }
    pub fn stride(&self) -> usize { self.stride }
    pub fn pixels(&self) -> &[u8] { &self.pixels }
}

impl fmt::Debug for BgraFrame {
    fn fmt(&self, value: &mut fmt::Formatter<'_>) -> fmt::Result {
        value.debug_struct("BgraFrame").field("monitor_id", &self.monitor_id)
            .field("captured_at_ms", &self.captured_at_ms).field("dimensions", &self.dimensions)
            .field("stride", &self.stride).field("pixel_bytes", &self.pixels.len()).finish()
    }
}

pub struct PreparedFrame {
    monitor_id: MonitorId, captured_at_ms: i64, dimensions: FrameDimensions,
    pixels: Zeroizing<Vec<u8>>, fingerprint: u64,
}

impl PreparedFrame {
    pub(crate) fn new(monitor_id: MonitorId, captured_at_ms: i64, dimensions: FrameDimensions,
                      pixels: Vec<u8>, fingerprint: u64) -> Self {
        Self { monitor_id, captured_at_ms, dimensions, pixels: Zeroizing::new(pixels), fingerprint }
    }
    pub fn monitor_id(&self) -> &MonitorId { &self.monitor_id }
    pub fn captured_at_ms(&self) -> i64 { self.captured_at_ms }
    pub fn dimensions(&self) -> FrameDimensions { self.dimensions }
    pub fn pixels(&self) -> &[u8] { &self.pixels }
    pub fn fingerprint(&self) -> u64 { self.fingerprint }
}

impl fmt::Debug for PreparedFrame {
    fn fmt(&self, value: &mut fmt::Formatter<'_>) -> fmt::Result {
        value.debug_struct("PreparedFrame").field("monitor_id", &self.monitor_id)
            .field("captured_at_ms", &self.captured_at_ms).field("dimensions", &self.dimensions)
            .field("pixel_bytes", &self.pixels.len()).finish()
    }
}
```

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt && cargo test -p karma-ai frame && cargo clippy -p karma-ai --all-targets -- -D warnings`

Commit: `feat: add privacy-safe BGRA frame values`.

---

### Task 2: Scale frames and compute difference hashes

**Files:**
- Modify: `crates/karma-ai/src/lib.rs`
- Create: `crates/karma-ai/src/preparation.rs`

**Interfaces:**
- Consumes: `BgraFrame`, `FrameDimensions`, `PreparedFrame`, `FrameError`.
- Produces: `FramePreparationConfig::new(u32)`, `FramePreparationConfig::default()`, `FramePreparer::prepare(BgraFrame)`.

- [ ] **Step 1: Write failing tests**

Tests must cover these exact cases:

```rust
fn horizontal_gradient(reversed: bool) -> BgraFrame {
    let dimensions = FrameDimensions::new(18, 8).unwrap();
    let mut pixels = Vec::with_capacity(dimensions.tight_byte_len().unwrap());
    for _ in 0..dimensions.height() {
        for x in 0..dimensions.width() {
            let value = if reversed { 255 - (x * 15) as u8 } else { (x * 15) as u8 };
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
    }
    BgraFrame::new(
        MonitorId("m".into()), 1, dimensions, dimensions.tight_stride().unwrap(), pixels,
    ).unwrap()
}

#[test]
fn target_dimensions_preserve_aspect_and_do_not_enlarge() {
    let config = FramePreparationConfig::default();
    assert_eq!(config.target(FrameDimensions::new(1920, 1080).unwrap()).unwrap(),
               FrameDimensions::new(640, 360).unwrap());
    assert_eq!(config.target(FrameDimensions::new(1080, 1920).unwrap()).unwrap(),
               FrameDimensions::new(360, 640).unwrap());
    assert_eq!(config.target(FrameDimensions::new(320, 200).unwrap()).unwrap(),
               FrameDimensions::new(320, 200).unwrap());
}

#[test]
fn one_pixel_and_padded_rows_are_prepared_tightly() {
    let input = BgraFrame::new(
        MonitorId("m".into()), 7, FrameDimensions::new(1, 1).unwrap(), 8,
        vec![10, 20, 30, 255, 99, 99, 99, 99],
    ).unwrap();
    let output = FramePreparer::default().prepare(input).unwrap();
    assert_eq!(output.dimensions(), FrameDimensions::new(1, 1).unwrap());
    assert_eq!(output.pixels(), &[10, 20, 30, 255]);
}

#[test]
fn fingerprint_is_stable_and_changes_with_structure() {
    let first = horizontal_gradient(false);
    let same = horizontal_gradient(false);
    let reversed = horizontal_gradient(true);
    let preparer = FramePreparer::default();
    assert_eq!(preparer.prepare(first).unwrap().fingerprint(),
               preparer.prepare(same).unwrap().fingerprint());
    assert_ne!(preparer.prepare(horizontal_gradient(false)).unwrap().fingerprint(),
               preparer.prepare(reversed).unwrap().fingerprint());
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-ai preparation`

Expected: compilation fails because configuration and preparer types are absent.

- [ ] **Step 3: Implement deterministic preparation**

Implement the module as follows. Horizontal and vertical interpolation are performed separately so intermediate multiplication remains bounded even for large source dimensions.

```rust
use crate::{BgraFrame, FrameDimensions, FrameError, PreparedFrame};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramePreparationConfig { maximum_edge: u32 }

impl Default for FramePreparationConfig {
    fn default() -> Self { Self { maximum_edge: 640 } }
}

impl FramePreparationConfig {
    pub fn new(maximum_edge: u32) -> Result<Self, FrameError> {
        if maximum_edge == 0 { return Err(FrameError::InvalidMaximumEdge); }
        Ok(Self { maximum_edge })
    }

    pub fn target(self, source: FrameDimensions) -> Result<FrameDimensions, FrameError> {
        let width = source.width();
        let height = source.height();
        if width.max(height) <= self.maximum_edge { return Ok(source); }
        let maximum = u64::from(self.maximum_edge);
        if width >= height {
            let scaled_height = (u64::from(height) * maximum + u64::from(width) / 2)
                / u64::from(width);
            FrameDimensions::new(self.maximum_edge, u32::try_from(scaled_height.max(1))
                .map_err(|_| FrameError::ArithmeticOverflow)?)
        } else {
            let scaled_width = (u64::from(width) * maximum + u64::from(height) / 2)
                / u64::from(height);
            FrameDimensions::new(u32::try_from(scaled_width.max(1))
                .map_err(|_| FrameError::ArithmeticOverflow)?, self.maximum_edge)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AxisSample { low: usize, high: usize, high_weight: u64, denominator: u64 }

fn axis_sample(source: u32, target: u32, index: u32) -> AxisSample {
    if source == 1 || target == 1 {
        return AxisSample { low: 0, high: 0, high_weight: 0, denominator: 1 };
    }
    let denominator = u64::from(target - 1);
    let numerator = u64::from(index) * u64::from(source - 1);
    let low = numerator / denominator;
    AxisSample {
        low: low as usize,
        high: (low + 1).min(u64::from(source - 1)) as usize,
        high_weight: numerator % denominator,
        denominator,
    }
}

fn lerp(first: u8, second: u8, weight: u64, denominator: u64) -> u8 {
    let inverse = denominator - weight;
    ((u64::from(first) * inverse + u64::from(second) * weight + denominator / 2)
        / denominator) as u8
}

fn offset(x: usize, y: usize, stride: usize, channel: usize) -> Result<usize, FrameError> {
    y.checked_mul(stride).and_then(|row| x.checked_mul(4)
        .and_then(|pixel| row.checked_add(pixel)).and_then(|pixel| pixel.checked_add(channel)))
        .ok_or(FrameError::ArithmeticOverflow)
}

fn sample_channel(pixels: &[u8], stride: usize, x: AxisSample, y: AxisSample,
                  channel: usize) -> Result<u8, FrameError> {
    let top_left = pixels[offset(x.low, y.low, stride, channel)?];
    let top_right = pixels[offset(x.high, y.low, stride, channel)?];
    let bottom_left = pixels[offset(x.low, y.high, stride, channel)?];
    let bottom_right = pixels[offset(x.high, y.high, stride, channel)?];
    let top = lerp(top_left, top_right, x.high_weight, x.denominator);
    let bottom = lerp(bottom_left, bottom_right, x.high_weight, x.denominator);
    Ok(lerp(top, bottom, y.high_weight, y.denominator))
}

fn scale_bgra(input: &BgraFrame, target: FrameDimensions) -> Result<Vec<u8>, FrameError> {
    let source = input.dimensions();
    let target_stride = target.tight_stride()?;
    let mut output = vec![0; target.tight_byte_len()?];
    for target_y in 0..target.height() {
        let y = axis_sample(source.height(), target.height(), target_y);
        for target_x in 0..target.width() {
            let x = axis_sample(source.width(), target.width(), target_x);
            for channel in 0..4 {
                let destination = offset(target_x as usize, target_y as usize,
                                         target_stride, channel)?;
                output[destination] = sample_channel(input.pixels(), input.stride(), x, y, channel)?;
            }
        }
    }
    Ok(output)
}

fn difference_hash(pixels: &[u8], dimensions: FrameDimensions) -> Result<u64, FrameError> {
    let stride = dimensions.tight_stride()?;
    let mut hash = 0u64;
    for row in 0..8u32 {
        let y = axis_sample(dimensions.height(), 8, row);
        let mut lumas = [0u16; 9];
        for column in 0..9u32 {
            let x = axis_sample(dimensions.width(), 9, column);
            let blue = u16::from(sample_channel(pixels, stride, x, y, 0)?);
            let green = u16::from(sample_channel(pixels, stride, x, y, 1)?);
            let red = u16::from(sample_channel(pixels, stride, x, y, 2)?);
            lumas[column as usize] = (77 * red + 150 * green + 29 * blue) >> 8;
        }
        for column in 0..8usize {
            if lumas[column] > lumas[column + 1] { hash |= 1 << (row * 8 + column as u32); }
        }
    }
    Ok(hash)
}

#[derive(Debug, Default)]
pub struct FramePreparer { config: FramePreparationConfig }

impl FramePreparer {
    pub fn new(config: FramePreparationConfig) -> Self { Self { config } }

pub fn prepare(&self, input: BgraFrame) -> Result<PreparedFrame, FrameError> {
        let target = self.config.target(input.dimensions())?;
        let pixels = scale_bgra(&input, target)?;
        let fingerprint = difference_hash(&pixels, target)?;
        Ok(PreparedFrame::new(
            input.monitor_id().clone(), input.captured_at_ms(), target, pixels, fingerprint,
        ))
    }
}
```

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt && cargo test -p karma-ai preparation && cargo clippy -p karma-ai --all-targets -- -D warnings`

Commit: `feat: prepare bounded inference frames`.

---

### Task 3: Add a capacity-one latest-frame mailbox

**Files:**
- Modify: `crates/karma-ai/src/lib.rs`
- Create: `crates/karma-ai/src/mailbox.rs`

**Interfaces:**
- Produces: `LatestFrameMailbox<T>::new`, `push`, `take`, `is_empty`.
- Uses only `std::sync::Mutex`; poisoning is recovered with `into_inner` because the stored value has no cross-field invariant.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn newest_value_replaces_and_returns_previous() {
    let mailbox = LatestFrameMailbox::new();
    assert_eq!(mailbox.push(1), None);
    assert_eq!(mailbox.push(2), Some(1));
    assert_eq!(mailbox.take(), Some(2));
    assert!(mailbox.is_empty());
}

#[test]
fn concurrent_producer_never_builds_a_queue() {
    let mailbox = Arc::new(LatestFrameMailbox::new());
    let producer = Arc::clone(&mailbox);
    std::thread::spawn(move || for value in 0..1_000 { producer.push(value); }).join().unwrap();
    assert_eq!(mailbox.take(), Some(999));
    assert!(mailbox.take().is_none());
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-ai mailbox`

Expected: compilation fails because `LatestFrameMailbox` is absent.

- [ ] **Step 3: Implement the mailbox**

```rust
use std::sync::{Mutex, MutexGuard};

#[derive(Debug)]
pub struct LatestFrameMailbox<T> { value: Mutex<Option<T>> }

impl<T> Default for LatestFrameMailbox<T> { fn default() -> Self { Self::new() } }
impl<T> LatestFrameMailbox<T> {
    pub fn new() -> Self { Self { value: Mutex::new(None) } }
    fn lock(&self) -> MutexGuard<'_, Option<T>> {
        self.value.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    pub fn push(&self, value: T) -> Option<T> { self.lock().replace(value) }
    pub fn take(&self) -> Option<T> { self.lock().take() }
    pub fn is_empty(&self) -> bool { self.lock().is_none() }
}
```

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt && cargo test -p karma-ai mailbox && cargo clippy -p karma-ai --all-targets -- -D warnings`

Commit: `feat: keep only the latest captured frame`.

---

### Task 4: Compose preparation with per-monitor scheduling

**Files:**
- Modify: `crates/karma-ai/src/lib.rs`
- Modify: `crates/karma-ai/src/scheduler.rs`
- Create: `crates/karma-ai/src/frame_pipeline.rs`
- Create: `crates/karma-ai/tests/frame_pipeline.rs`

**Interfaces:**
- Consumes: `BgraFrame`, `FramePreparer`, `FrameScheduler`, `FrameMetadata`, `FrameWork`.
- Produces: `FramePipeline::default`, `FramePipeline::process`, `ScheduledFrame { frame, work }`.

- [ ] **Step 1: Write failing integration tests**

```rust
#[test]
fn first_prepared_frame_requests_image_and_ocr() {
    let output = FramePipeline::default().process(frame("m", 1_000, 10)).unwrap();
    assert!(output.work.run_image);
    assert!(output.work.run_ocr);
    assert_eq!(output.frame.dimensions(), FrameDimensions::new(1, 1).unwrap());
}

#[test]
fn unchanged_frame_respects_scheduler_limits() {
    let mut pipeline = FramePipeline::default();
    pipeline.process(frame("m", 1_000, 10)).unwrap();
    let output = pipeline.process(frame("m", 2_000, 10)).unwrap();
    assert!(output.work.run_image);
    assert!(!output.work.run_ocr);
}

#[test]
fn backwards_timestamp_does_not_run_or_overflow() {
    let mut scheduler = FrameScheduler::default();
    scheduler.select(FrameMetadata { monitor_id: MonitorId("m".into()), captured_at_ms: i64::MAX, fingerprint: 1 });
    let output = scheduler.select(FrameMetadata { monitor_id: MonitorId("m".into()), captured_at_ms: i64::MIN, fingerprint: 2 });
    assert_eq!(output, FrameWork { run_image: false, run_ocr: false });
}
```

The helper `frame` creates a 1×1 tight BGRA frame with all RGB channels set to the supplied value.

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-ai --test frame_pipeline`

Expected: compilation fails because `FramePipeline` and `ScheduledFrame` are absent.

- [ ] **Step 3: Implement composition and monotonic-safe intervals**

In `scheduler.rs`, replace direct subtraction with:

```rust
let image_elapsed = frame.captured_at_ms.saturating_sub(previous.image_at_ms);
let ocr_elapsed = frame.captured_at_ms.saturating_sub(previous.ocr_at_ms);
let run_image = image_elapsed >= 500;
let run_ocr = ocr_elapsed >= 1000 && frame.fingerprint != previous.ocr_fingerprint;
```

Implement:

```rust
pub struct ScheduledFrame { pub frame: PreparedFrame, pub work: FrameWork }
#[derive(Default)]
pub struct FramePipeline { preparer: FramePreparer, scheduler: FrameScheduler }
impl FramePipeline {
    pub fn process(&mut self, input: BgraFrame) -> Result<ScheduledFrame, FrameError> {
        let frame = self.preparer.prepare(input)?;
        let work = self.scheduler.select(FrameMetadata {
            monitor_id: frame.monitor_id().clone(),
            captured_at_ms: frame.captured_at_ms(),
            fingerprint: frame.fingerprint(),
        });
        Ok(ScheduledFrame { frame, work })
    }
}
```

- [ ] **Step 4: Run full gates and privacy scan**

Run:

```bash
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --all-targets --target x86_64-pc-windows-msvc
rg -n "derive.*Serialize|pixels.*Debug|raw_text|recognized_text|screenshot" crates/karma-ai
git diff --check
```

Expected: all tests and both host/Windows compilation pass. Pixel values are not serializable and sensitive terms occur only in negative privacy assertions.

- [ ] **Step 5: Commit**

Commit: `feat: schedule prepared inference frames`.

## Completion Boundary

After this plan, `karma-ai` accepts transient validated BGRA frames and returns bounded, zeroizing, scheduled frames. The next plan adds the Windows D3D11 device, FreeThreaded WGC FramePool, surface extraction, GPU scaler, and capture-session lifecycle using this boundary.
