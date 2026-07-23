# Karma Windows WGC Session Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a Windows D3D11/WinRT capture device and a FreeThreaded Windows.Graphics.Capture session that keeps exactly the newest pending frame and reports deterministic lifecycle state.

**Architecture:** Portable lifecycle transitions remain host-testable in `karma-windows`; Windows-only modules own D3D11 device creation, WinRT bridging, WGC event registration, frame ownership, and cleanup. Captured frames remain GPU surfaces in this plan; the next plan extracts/scales them into the existing `BgraFrame` boundary.

**Tech Stack:** Rust 1.85+, `windows` 0.62.2, D3D11, DXGI, Windows.Graphics.Capture, existing `LatestFrameMailbox`.

## Global Constraints

- Target Windows versions remain Windows 10 22H2 and Windows 11.
- FramePool uses `CreateFreeThreaded`, BGRA8, and exactly two internal buffers.
- The application mailbox contains zero or one `Direct3D11CaptureFrame`; replacing a frame closes the old frame.
- Event callbacks do not run scaling, OCR, ONNX, filesystem, network, IPC, or policy code.
- Capture item names, window titles, pixels, and OCR text are never logged.
- Partial initialization cleans up every registered event and COM resource.
- `stop` and `Drop` are idempotent.
- Host tests plus Windows MSVC cross-compilation and Clippy are required; runtime acceptance remains a Windows-machine gate.

---

### Task 1: Add deterministic capture-session lifecycle state

**Files:**
- Modify: `crates/karma-windows/src/lib.rs`
- Create: `crates/karma-windows/src/capture_state.rs`

**Interfaces:**
- Produces: `CaptureSessionStatus`, `CaptureSessionEvent`, `CaptureSessionState::new/status/apply`.
- No Windows dependencies.

- [ ] **Step 1: Write failing transition tests**

```rust
#[test]
fn start_frame_resize_close_and_stop_are_explicit() {
    let mut state = CaptureSessionState::new();
    assert_eq!(state.status(), CaptureSessionStatus::Starting);
    state.apply(CaptureSessionEvent::Started);
    assert_eq!(state.status(), CaptureSessionStatus::Running);
    state.apply(CaptureSessionEvent::SizeChanged);
    assert_eq!(state.status(), CaptureSessionStatus::RecreateRequired);
    state.apply(CaptureSessionEvent::FrameArrived);
    assert_eq!(state.status(), CaptureSessionStatus::RecreateRequired);
    state.apply(CaptureSessionEvent::TargetClosed);
    assert_eq!(state.status(), CaptureSessionStatus::TargetClosed);
    state.apply(CaptureSessionEvent::FrameArrived);
    assert_eq!(state.status(), CaptureSessionStatus::TargetClosed);
    state.apply(CaptureSessionEvent::Stopped);
    assert_eq!(state.status(), CaptureSessionStatus::Stopped);
}

#[test]
fn terminal_stop_is_idempotent_and_ignores_late_callbacks() {
    let mut state = CaptureSessionState::new();
    state.apply(CaptureSessionEvent::Stopped);
    state.apply(CaptureSessionEvent::FrameArrived);
    state.apply(CaptureSessionEvent::Failed);
    assert_eq!(state.status(), CaptureSessionStatus::Stopped);
}

#[test]
fn access_and_device_failures_remain_distinct() {
    let mut access = CaptureSessionState::new();
    access.apply(CaptureSessionEvent::AccessDenied);
    assert_eq!(access.status(), CaptureSessionStatus::AccessDenied);
    let mut device = CaptureSessionState::new();
    device.apply(CaptureSessionEvent::DeviceLost);
    assert_eq!(device.status(), CaptureSessionStatus::DeviceLost);
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p karma-windows capture_state`

Expected: missing lifecycle types fail compilation.

- [ ] **Step 3: Implement the transition table**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureSessionStatus {
    Starting, Running, RecreateRequired, TargetClosed,
    DeviceLost, AccessDenied, Failed, Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureSessionEvent {
    Started, FrameArrived, SizeChanged, TargetClosed,
    DeviceLost, AccessDenied, Failed, Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureSessionState { status: CaptureSessionStatus }

impl Default for CaptureSessionState { fn default() -> Self { Self::new() } }
impl CaptureSessionState {
    pub fn new() -> Self { Self { status: CaptureSessionStatus::Starting } }
    pub fn status(&self) -> CaptureSessionStatus { self.status }
    pub fn apply(&mut self, event: CaptureSessionEvent) {
        if self.status == CaptureSessionStatus::Stopped { return; }
        if event == CaptureSessionEvent::Stopped {
            self.status = CaptureSessionStatus::Stopped;
            return;
        }
        if matches!(self.status,
            CaptureSessionStatus::RecreateRequired | CaptureSessionStatus::TargetClosed |
            CaptureSessionStatus::DeviceLost | CaptureSessionStatus::AccessDenied |
            CaptureSessionStatus::Failed) { return; }
        self.status = match event {
            CaptureSessionEvent::Started | CaptureSessionEvent::FrameArrived => CaptureSessionStatus::Running,
            CaptureSessionEvent::SizeChanged => CaptureSessionStatus::RecreateRequired,
            CaptureSessionEvent::TargetClosed => CaptureSessionStatus::TargetClosed,
            CaptureSessionEvent::DeviceLost => CaptureSessionStatus::DeviceLost,
            CaptureSessionEvent::AccessDenied => CaptureSessionStatus::AccessDenied,
            CaptureSessionEvent::Failed => CaptureSessionStatus::Failed,
            CaptureSessionEvent::Stopped => unreachable!(),
        };
    }
}
```

- [ ] **Step 4: Verify and commit**

Run: `cargo fmt && cargo test -p karma-windows capture_state && cargo clippy -p karma-windows --all-targets -- -D warnings`

Commit: `feat: model Windows capture session state`.

---

### Task 2: Create the D3D11 capture device and WinRT bridge

**Files:**
- Modify: `crates/karma-windows/Cargo.toml`
- Modify: `crates/karma-windows/src/lib.rs`
- Modify: `crates/karma-windows/src/native.rs`
- Create: `crates/karma-windows/src/d3d11_device.rs`

**Interfaces:**
- Produces on Windows: `CaptureDriver::{Hardware,Warp}`, `D3d11CaptureDevice::new`, `driver`, `winrt_device`, `native_device`, `immediate_context`.
- Extends `WindowsAdapterError` with operation-specific errors through the existing `WindowsApi` variant.

- [ ] **Step 1: Add Windows features and a compile-first test**

Add these `windows` features:

```toml
"Graphics_DirectX",
"Graphics_DirectX_Direct3D11",
"Win32_Graphics_Direct3D",
"Win32_Graphics_Direct3D11",
"Win32_Graphics_Dxgi",
"Win32_System_WinRT_Direct3D11",
```

Add Windows-only exports and this compile test inside the new module:

```rust
#[test]
fn public_device_constructor_has_stable_signature() {
    let constructor: fn() -> Result<D3d11CaptureDevice, WindowsAdapterError> =
        D3d11CaptureDevice::new;
    let _ = constructor;
}
```

- [ ] **Step 2: Run RED**

Run: `cargo check -p karma-windows --target x86_64-pc-windows-msvc --tests`

Expected: missing device types fail Windows compilation.

- [ ] **Step 3: Implement hardware-first device creation**

Change `WindowsAdapterError::api` to `pub(crate)`. Implement a private `create_for_driver(D3D_DRIVER_TYPE)` that calls `D3D11CreateDevice` with `D3D11_CREATE_DEVICE_BGRA_SUPPORT`, `D3D11_SDK_VERSION`, no explicit adapter, and returns non-null `ID3D11Device` plus `ID3D11DeviceContext`. Cast the device to `IDXGIDevice`, call `CreateDirect3D11DeviceFromDXGIDevice`, and cast the returned `IInspectable` to WinRT `IDirect3DDevice`.

```rust
pub enum CaptureDriver { Hardware, Warp }

pub struct D3d11CaptureDevice {
    driver: CaptureDriver,
    native: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt: IDirect3DDevice,
}

impl D3d11CaptureDevice {
    pub fn new() -> Result<Self, WindowsAdapterError> {
        match create_for_driver(D3D_DRIVER_TYPE_HARDWARE) {
            Ok((native, context, winrt)) => Ok(Self {
                driver: CaptureDriver::Hardware, native, context, winrt,
            }),
            Err(_) => {
                let (native, context, winrt) = create_for_driver(D3D_DRIVER_TYPE_WARP)?;
                Ok(Self { driver: CaptureDriver::Warp, native, context, winrt })
            }
        }
    }
    pub fn driver(&self) -> CaptureDriver { self.driver }
    pub fn winrt_device(&self) -> &IDirect3DDevice { &self.winrt }
    pub fn native_device(&self) -> &ID3D11Device { &self.native }
    pub fn immediate_context(&self) -> &ID3D11DeviceContext { &self.context }
}
```

Every unsafe call has a local safety comment. Missing output interfaces return a `WindowsApi` error with operation `D3D11CreateDevice output` and `core::Error::empty()`.

- [ ] **Step 4: Verify and commit**

Run host tests, Windows `cargo check --tests`, and Windows-target Clippy for `karma-windows`.

Commit: `feat: create Windows D3D11 capture device`.

---

### Task 3: Receive the newest FreeThreaded WGC frame

**Files:**
- Modify: `crates/karma-windows/Cargo.toml`
- Modify: `crates/karma-windows/src/lib.rs`
- Modify: `crates/karma-windows/src/native.rs`
- Create: `crates/karma-windows/src/wgc_session.rs`

**Interfaces:**
- Consumes: `WgcCaptureTarget`, `D3d11CaptureDevice`, `CaptureSessionState`, `LatestFrameMailbox`.
- Produces on Windows: `CapturedGpuFrame`, `WgcCaptureSession::start/status/take_latest/stop`.

- [ ] **Step 1: Add dependency and compile-first ownership tests**

Add `karma-ai = { path = "../karma-ai" }`. Expose `WgcCaptureTarget::capture_item(&self) -> &GraphicsCaptureItem` as `pub(crate)`.

Windows-only tests:

```rust
#[test]
fn captured_frame_and_session_are_sendable_to_workers() {
    fn require_send<T: Send>() {}
    require_send::<CapturedGpuFrame>();
    require_send::<WgcCaptureSession>();
}

#[test]
fn stable_session_api_compiles() {
    let start: fn(MonitorId, WgcCaptureTarget, &D3d11CaptureDevice)
        -> Result<WgcCaptureSession, WindowsAdapterError> = WgcCaptureSession::start;
    let _ = start;
}
```

- [ ] **Step 2: Run RED**

Run: `cargo check -p karma-windows --target x86_64-pc-windows-msvc --tests`

Expected: session and captured-frame types are missing.

- [ ] **Step 3: Implement frame ownership**

```rust
pub struct CapturedGpuFrame { inner: Option<Direct3D11CaptureFrame> }
impl CapturedGpuFrame {
    fn new(inner: Direct3D11CaptureFrame) -> Self { Self { inner: Some(inner) } }
    pub fn content_size(&self) -> Result<(u32, u32), WindowsAdapterError> {
        let size = self.inner().ContentSize()
            .map_err(|source| WindowsAdapterError::api("Direct3D11CaptureFrame.ContentSize", source))?;
        if size.Width <= 0 || size.Height <= 0 {
            return Err(WindowsAdapterError::InvalidCaptureSize);
        }
        Ok((size.Width as u32, size.Height as u32))
    }
    pub(crate) fn inner(&self) -> &Direct3D11CaptureFrame { self.inner.as_ref().expect("live frame") }
}
impl Drop for CapturedGpuFrame {
    fn drop(&mut self) { if let Some(frame) = self.inner.take() { let _ = frame.Close(); } }
}
```

`content_size` returns `InvalidCaptureSize` for non-positive dimensions and otherwise converts to `u32`.

- [ ] **Step 4: Implement session start and callbacks**

Create a two-buffer FreeThreaded pool using `DirectXPixelFormat::B8G8R8A8UIntNormalized`. Register `TypedEventHandler` callbacks:

```rust
let frame_mailbox = Arc::clone(&mailbox);
let frame_state = Arc::clone(&state);
let frame_handler = TypedEventHandler::new(move |sender, _| {
    let Some(pool) = sender.as_ref() else { return Ok(()); };
    match pool.TryGetNextFrame() {
        Ok(frame) => {
            frame_mailbox.push(CapturedGpuFrame::new(frame));
            lock_state(&frame_state).apply(CaptureSessionEvent::FrameArrived);
        }
        Err(_) => lock_state(&frame_state).apply(CaptureSessionEvent::Failed),
    }
    Ok(())
});
```

The item-closed callback applies `TargetClosed` and clears the mailbox. Create the capture session, call `StartCapture`, then apply `Started`.

`WgcCaptureSession` stores monitor ID, item, FramePool, capture session, both event tokens, mailbox, state, and `stopped: bool`.

- [ ] **Step 5: Implement idempotent cleanup**

`stop` returns immediately if already stopped. Otherwise it sets `stopped`, removes both event tokens, clears the mailbox, closes capture session and FramePool, then applies `Stopped`. `Drop` calls `stop` and ignores the result. Cleanup attempts all operations even if one fails; the first `WindowsAdapterError` is returned after cleanup completes.

- [ ] **Step 6: Run complete gates and commit**

Run:

```bash
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --all-targets --target x86_64-pc-windows-msvc
cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings
git diff --check
```

Commit: `feat: receive latest Windows capture frame`.

## Completion Boundary

This plan creates a real WGC/D3D11 frame source with bounded frame ownership and deterministic cleanup, verified by Windows cross-compilation. It does not read GPU pixels. The next plan uses `CapturedGpuFrame::inner().Surface()`, D3D11 video processing, a bounded staging texture, and the existing portable frame pipeline.
