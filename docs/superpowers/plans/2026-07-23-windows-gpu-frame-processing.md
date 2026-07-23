# Windows GPU Frame Processing Implementation Plan

> **Execution note:** Implement task-by-task with test-first checkpoints. Windows runtime behavior remains pending until the final manual acceptance task is run on a supported Windows machine.

**Goal:** Convert each transient WGC `CapturedGpuFrame` into a bounded, zeroizing `PreparedFrame` without persisting pixels, using D3D11 video processing for resize and a CPU reference fallback when GPU resize is unavailable.

**Architecture:** `karma-windows` owns WinRT-surface interop, D3D11 resources, mapping, and per-monitor worker control. `karma-ai` remains the portable validation, resize-reference, fingerprint, and scheduling boundary. GPU resources stay on one processing thread; only bounded CPU pixels cross into `karma-ai`.

**Tech stack:** Rust 1.85, `windows` 0.62.2, Windows.Graphics.Capture, D3D11 video processor, DXGI BGRA8, existing `karma-ai` frame pipeline.

---

## Task 1: Complete captured-frame metadata and native texture boundary

**Files:**

- Modify: `crates/karma-windows/src/wgc_session.rs`
- Create: `crates/karma-windows/src/gpu_frame.rs`
- Modify: `crates/karma-windows/src/lib.rs`
- Modify: `crates/karma-windows/Cargo.toml`

1. Add Windows-only compile tests requiring stable methods for capture-relative milliseconds and conversion of `IDirect3DSurface` to `ID3D11Texture2D`.
2. Run `cargo check -p karma-windows --target x86_64-pc-windows-msvc --tests` and confirm the new API is absent.
3. Convert WGC `SystemRelativeTime.Duration` from 100 ns units to milliseconds with checked/saturating integer behavior. Keep it monotonic-relative; do not present it as Unix time.
4. Query `IDirect3DDxgiInterfaceAccess` from the frame surface and call `GetInterface::<ID3D11Texture2D>()`. Validate the texture description is non-zero and `DXGI_FORMAT_B8G8R8A8_UNORM`.
5. Return operation-scoped `WindowsAdapterError` values without titles, pixels, or OCR text.
6. Run format, host tests, Windows check, and Windows Clippy.
7. Commit: `feat: expose native Windows capture textures`.

## Task 2: Add a bounded mapped-texture reader

**Files:**

- Create: `crates/karma-windows/src/staging_reader.rs`
- Modify: `crates/karma-windows/src/lib.rs`
- Modify: `crates/karma-windows/src/native.rs`

1. Add portable unit tests for checked row-copy planning: zero dimensions, short pitch, overflow, and valid padded rows.
2. Run host tests and confirm the row-copy API is missing.
3. Implement a pure `MappedBgraLayout` validator that computes tight stride and total bytes with checked arithmetic and enforces a maximum edge of 640 for production mapped output.
4. Add the Windows `StagingTextureReader`: create/reuse a `D3D11_USAGE_STAGING` BGRA8 texture with `D3D11_CPU_ACCESS_READ`, copy the bounded GPU output into it, `Map` for read, copy only active bytes per row into a zeroizing `BgraFrame`, and always `Unmap` through a guard.
5. Distinguish staging creation, copy, map, pitch, and buffer-validation failures by stable operation name.
6. Run all host and Windows gates.
7. Commit: `feat: map bounded Windows staging frames`.

## Task 3: Implement D3D11 video-processor resize

**Files:**

- Create: `crates/karma-windows/src/gpu_scaler.rs`
- Modify: `crates/karma-windows/src/lib.rs`
- Modify: `crates/karma-windows/Cargo.toml`

1. Add Windows compile tests for `GpuFrameScaler::new` and `GpuFrameScaler::scale` signatures plus portable target-size tests shared with `FramePreparationConfig`.
2. Run the Windows check and confirm the scaler types are missing.
3. Cast the D3D11 device/context to `ID3D11VideoDevice` and `ID3D11VideoContext` and create a video-processor enumerator for the current source/target dimensions.
4. Create/reuse bounded BGRA8 default-usage output textures, input/output video-processor views, and a video processor. Configure full source and destination rectangles and progressive frame format, then invoke `VideoProcessorBlt`.
5. Recreate only dimension-dependent resources when source or target changes. Never map the full-resolution WGC texture on the GPU-success path.
6. Report `Unsupported` separately from runtime GPU failure so the caller can choose the reference fallback and increment a privacy-safe health counter.
7. Run all host and Windows gates.
8. Commit: `feat: resize Windows frames on the GPU`.

## Task 4: Add the CPU correctness fallback

**Files:**

- Modify: `crates/karma-windows/src/staging_reader.rs`
- Create: `crates/karma-windows/src/frame_processor.rs`
- Modify: `crates/karma-windows/src/lib.rs`

1. Add host-testable fallback-selection tests: GPU success, unsupported GPU, GPU runtime failure, and failure of both paths. The tests use fake backends and contain no COM objects.
2. Run tests and confirm the processing coordinator is absent.
3. Implement `WindowsFrameProcessor`: compute the bounded target dimensions; prefer GPU resize plus bounded staging read; on unsupported/runtime failure, stage the source texture, build a transient zeroizing `BgraFrame`, and use the existing portable `FramePreparer`.
4. Expose counters only for processed frames, GPU fallback use, and failures. Do not expose pixels, hashes, window titles, or OCR text.
5. Ensure full-resolution fallback pixels are dropped and zeroized immediately after portable resize.
6. Run all gates and commit: `feat: prepare Windows capture frames`.

## Task 5: Wire a per-monitor processing worker

**Files:**

- Create: `crates/karma-windows/src/frame_worker.rs`
- Modify: `crates/karma-windows/src/lib.rs`
- Modify: `apps/karma-agent-windows/src/startup.rs`

1. Add host-testable worker-control tests using a fake latest-frame source and fake processor: latest-only behavior, stop, recreate-required, target-closed, and processor failure.
2. Run tests and confirm the worker API is absent.
3. Own device context, scaler, staging resources, and scheduler on the processing thread. Poll/take the capacity-one mailbox without a busy loop, process while the captured frame is alive, then drop it before the next iteration.
4. Feed `PreparedFrame` into the existing `FramePipeline` scheduling boundary and a no-op inference consumer. Do not add ONNX or OCR in this slice.
5. Stop and join workers deterministically before closing their WGC sessions.
6. Run all gates and commit: `feat: process Windows capture frames`.

## Task 6: Verification and Windows handoff

**Files:**

- Modify: `README.md`
- Create: `docs/windows-frame-pipeline-acceptance.md`

1. Run `cargo fmt --check`, `cargo test --workspace`, host Clippy, Windows workspace check, Windows Clippy, and `git diff --check`.
2. Document that cross-compilation validates API/type correctness but not GPU driver behavior.
3. Provide a Windows checklist for single/multi-monitor frame arrival, video load, bounded memory, resolution/rotation changes, WARP/fallback health, stop cleanup, and SDR color correctness.
4. On a Windows 10 22H2/Windows 11 machine, record OS/build, adapter/driver, monitor topology, and pass/fail results. Do not collect screenshots.
5. Commit documentation and only then mark cross-platform development verification complete; runtime acceptance remains explicitly pending until Windows evidence exists.

## Completion Boundary

This plan ends with bounded `PreparedFrame` production and scheduler delivery. ONNX Runtime model execution, OCR, hot-plug recovery, enforcement, and service watchdog integration remain separate slices.
