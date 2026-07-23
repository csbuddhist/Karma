# Karma Windows Agent Bootstrap Implementation Plan

> **For agentic workers:** Use the executing-plans and test-driven-development skills task by task.

**Goal:** Add the first Windows-native Agent boundary: deterministic window-to-monitor attribution, active-monitor enumeration, foreground-window metadata collection, WGC capture-item creation, and a Windows Agent startup probe.

**Architecture:** A new `karma-windows` crate separates pure geometry/attribution code from `cfg(windows)` Win32 and WinRT adapters. A small `karma-agent-windows` executable initializes WinRT, inventories active monitors, and verifies that WGC capture items can be created. This slice deliberately stops before D3D11 frame-pool and pixel readback work, which needs a Windows runtime test machine and a separate implementation plan.

**Tech stack:** Rust 1.85+, `windows` 0.62.2, Win32 `EnumDisplayMonitors`/foreground-window APIs, Windows.Graphics.Capture, Cargo cross-target checks.

## Global constraints

- No window title, screen pixel, OCR text, URL, or keyboard data is retained or logged.
- Source attribution is reliable only when one foreground-process window has a unique largest positive overlap with the observed monitor.
- Equal-overlap candidates, missing foreground process, and zero overlap remain explicitly unreliable.
- All Win32 `unsafe` calls stay inside the native adapter and carry a local safety explanation.
- Windows APIs are target-gated; geometry tests run on macOS and Windows-target compilation is a required gate.
- Creating a `GraphicsCaptureItem` proves WGC target access only; it does not claim that frame delivery is implemented.

---

### Task 1: Implement portable geometry and source attribution

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/karma-windows/Cargo.toml`
- Create: `crates/karma-windows/src/lib.rs`
- Create: `crates/karma-windows/src/attribution.rs`

**Tests first:**

- Rectangle intersection computes positive area and returns zero for touching edges.
- The unique largest overlapping window owned by the foreground PID is reliable.
- A larger window from a background PID is ignored.
- Equal best overlaps are ambiguous.
- Missing foreground PID and no overlap have distinct unreliable reasons.

**Public API:**

- `Rect { left, top, right, bottom }` with `intersection_area` using saturating 64-bit arithmetic.
- `WindowCandidate { handle, pid, bounds }` without titles or content.
- `AttributedWindow { handle, pid, overlap_area }`.
- `AttributionResult::{Reliable, Unreliable}` and `UnreliableReason`.
- `SourceAttributor::resolve(monitor, foreground_pid, candidates)`.

**Verification:** `cargo fmt`, `cargo test -p karma-windows`, and focused Clippy; then commit `feat: resolve Windows source attribution`.

---

### Task 2: Add Windows desktop inventory and WGC target creation

**Files:**

- Modify: `crates/karma-windows/Cargo.toml`
- Modify: `crates/karma-windows/src/lib.rs`
- Create: `crates/karma-windows/src/native.rs`

**Native API:**

- `MonitorHandle(isize)` and `MonitorSnapshot { id, handle, bounds }`.
- `ForegroundWindowSnapshot { handle, pid, bounds }`.
- `enumerate_active_monitors()` using `EnumDisplayMonitors` and callback-owned vector state.
- `foreground_window()` using `GetForegroundWindow`, `GetWindowRect`, and `GetWindowThreadProcessId`.
- `WgcCaptureTarget::for_monitor(MonitorHandle)` obtains the `GraphicsCaptureItem` activation factory and calls `IGraphicsCaptureItemInterop::CreateForMonitor`.
- `WgcCaptureTarget::size()` returns validated dimensions without exposing screen content.

**Failure behavior:**

- Null foreground handles return `Ok(None)`.
- Failed Win32/WinRT calls return typed `WindowsAdapterError` values without titles or sensitive payloads.
- Monitor enumeration preserves callback state lifetime and never stores raw pointers after the call.

**Verification:** install/check `x86_64-pc-windows-msvc`, run host tests and `cargo check -p karma-windows --target x86_64-pc-windows-msvc`, then Clippy; commit `feat: add Windows WGC target adapter`.

---

### Task 3: Bootstrap the Windows session Agent

**Files:**

- Modify: `Cargo.toml`
- Create: `apps/karma-agent-windows/Cargo.toml`
- Create: `apps/karma-agent-windows/src/main.rs`
- Create: `apps/karma-agent-windows/src/startup.rs`

**Tests first:**

- Startup summary reports monitor and WGC-ready counts only.
- Partial WGC failures produce degraded status instead of aborting successful monitors.
- Zero active monitors produces unavailable status.

**Behavior:**

- Portable `StartupProbe` consumes a monitor inventory and capture-target factory through small traits.
- Windows `main` initializes the multithreaded WinRT apartment, enumerates monitors, attempts a WGC target per monitor, and prints only structured health counts.
- Non-Windows `main` exits with a clear unsupported-platform message so workspace tests remain runnable on macOS.
- The executable does not process frames, enforce policy, or terminate applications in this slice.

**Verification:**

```bash
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p karma-windows --target x86_64-pc-windows-msvc
cargo check -p karma-agent-windows --target x86_64-pc-windows-msvc
git diff --check
```

Commit `feat: bootstrap Windows session agent`.

## Completion boundary

At completion the code can prove monitor discovery, foreground metadata collection, deterministic attribution, and WGC capture-item creation at compile time. Actual `Direct3D11CaptureFramePool`, frame-arrival handling, GPU resize/fingerprint, ONNX invocation, hot-plug event recovery, and Windows runtime verification remain the next Windows-machine-dependent slice.
