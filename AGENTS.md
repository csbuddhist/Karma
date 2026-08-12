# Repository Guidelines

## Project Structure & Module Organization

Karma is a Rust 2024 workspace. Shared libraries live in `crates/karma-*`; Windows executables live in `apps/karma-agent-windows` and `apps/karma-service-windows`. The Tauri/React console is in `apps/karma-ui`, with TypeScript in `src/` and Rust in `src-tauri/`. Store model manifests in `assets/`, utilities in `tools/`, and design or hardware acceptance material in `docs/`. Treat `release/windows-x64-test/` as generated. Put integration tests in each crate's `tests/` directory and fixtures beside their consumers.

## Build, Test, and Development Commands

- `cargo build --workspace` — build all Rust members with the pinned Rust 1.85 toolchain.
- `cargo test --workspace` — run Rust unit and integration tests.
- `cargo fmt --all -- --check` — verify Rust formatting.
- `cargo clippy --workspace --all-targets -- -D warnings` — reject all Clippy warnings.
- `cargo check --workspace --all-targets --target x86_64-pc-windows-msvc` — validate Windows-only APIs when the target is installed.
- `cd apps/karma-ui && npm ci && npm run dev` — install locked UI dependencies and start Vite.
- `cd apps/karma-ui && npm run build` — type-check and produce the frontend bundle; use `npm run tauri -- dev` for the desktop app.
- `python3 -m unittest discover -s tools/ocr-export/tests` — run OCR exporter tests.
- `bash tools/package-windows-test/test_bundle_contract.sh` — validate the packaged Windows bundle contract.

## Coding Style & Naming Conventions

Accept `rustfmt` output and keep Clippy clean. Use `snake_case` for Rust modules/functions, `PascalCase` for types, and `SCREAMING_SNAKE_CASE` for constants. TypeScript uses two-space indentation, double quotes, semicolons, `PascalCase` React components, and `camelCase` functions. Python uses four spaces and `test_*` methods.

## Testing Guidelines

Place focused Rust unit tests in `#[cfg(test)] mod tests` and cross-module scenarios in `tests/*.rs`. Name tests after observable behavior, such as `blocked_schedule_returns_terminate`. Add regression coverage for policy, IPC, privacy, and model-contract changes. No numeric coverage threshold is configured. Windows capture, GPU, and ONNX changes require real-hardware evidence from the relevant `docs/*acceptance.md` checklist.

## Commit & Pull Request Guidelines

Follow the existing imperative Conventional Commit prefixes: `feat:`, `fix:`, `docs:`, and `build:`. Keep commits narrowly scoped. Pull requests should describe behavior and security/privacy impact, link the issue, list verification commands, and include UI screenshots or Windows acceptance evidence when applicable.

## Security & Configuration Tips

Keep `.local-models/`, credentials, raw captures, and decrypted evidence out of Git. Preserve authenticated, bounded IPC interfaces; never replace identity-checked operations with arbitrary path or PID execution.
