# Task 7 Report — Deterministic OCR Profile Selection

## Status

Implemented deterministic OCR profile preference parsing, selection policy, accurate-candidate benchmarking, and a bounded privacy-safe cache.

## Policy coverage

- `lightweight` always selects the bundled profile.
- A missing explicit `accurate` bundle selects lightweight and sets `download_required`; `auto` selects lightweight without a benchmark claim.
- An accurate candidate can be `Ready`, `Missing`, or `Rejected`; rejected runtime/reference candidates always fall back to lightweight.
- Accurate benchmarking performs exactly three unmeasured warmups and ten timed end-to-end calls. P95 is the sorted tenth sample.
- Automatic accurate selection requires successful warmups and ten successful measured calls, no warmup or measured resource-limit event, a matching in-memory reference summary, and P95 at or below 800 ms.
- Explicit accurate selection overrides only the performance budget and reports `performance_budget_exceeded`; it never overrides a runtime, resource, reference, or rejected-candidate failure.
- Cache identity is exactly profile, bundle version, CPU architecture, logical core count, and active display count. It contains only that key, the selected profile, P95, and approved boolean outcomes.
- Cache decoding denies unknown fields and reads at most 16 KiB plus one byte from an opened file, returns stable redacted errors, and writes a flushed same-directory temporary file before rename.
- The 640x360 in-memory fixture renders embedded bitmap glyphs for the five approved fixed UI strings (English, simplified Chinese, traditional Chinese), with an in-memory fixed word pack used to derive the reference summary; no fixture frame is written to disk.

## TDD evidence

- RED: `cargo test -p karma-agent-windows ocr_profile` failed before the profile-selection API existed.
- RED: `cargo test -p karma-agent-windows ocr_profile::tests::rejected_accurate_candidate_falls_back_without_requesting_a_download` failed before candidate rejection was represented.
- RED: `cargo test -p karma-agent-windows ocr_profile::tests::accurate_selection_rejects_a_resource_limit_reached_during_warmup` failed before warmup resource events were included.
- RED: `cargo test -p karma-agent-windows ocr_profile::tests::accurate_selection_rejects_a_runtime_failure_during_warmup` failed before warmup failures were included.
- GREEN: focused policy suite passes with 17 tests, including glyph coverage for every approved character.

## Verification

Commands run successfully after the implementation:

```text
cargo fmt --all -- --check
cargo test -p karma-agent-windows ocr_profile
cargo clippy -p karma-agent-windows --all-targets -- -D warnings
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
git diff --check
```

## Concerns

This task deliberately supplies policy and injectable interfaces only. Download orchestration and Windows worker integration remain for Task 8. The benchmark renderer is deterministic and in-memory; it does not write screenshots or recognized text to cache or logs.

## Final review remediation

- Cache replacement now uses a unique same-directory `create_new` temporary name built from a process-local atomic sequence. It retries intentional collision failures up to 128 times and returns only the stable cache-unavailable error when it cannot allocate a name.
- Unix keeps same-directory `rename`; Windows compiles a replace-existing path using `ReplaceFileW` with write-through, with `MoveFileExW` fallback only when the destination is absent. The temporary file is flushed and closed before replacement.
- The fixture preserves its declared title case with separate lower-case bitmap glyphs rather than uppercasing the source characters.
- Final focused suite contains 20 OCR profile tests. `cargo check -p karma-agent-windows --target x86_64-pc-windows-msvc` compiled the Windows replacement branch successfully (the binary has expected dead-code warnings until Task 8 wires the policy into startup).
