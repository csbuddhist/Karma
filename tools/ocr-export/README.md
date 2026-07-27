# PP-OCRv5 export

This tool converts the four pinned official PaddleOCR PP-OCRv5 inference archives into the
dynamic-shape ONNX bundles consumed by `karma-onnx`. Production model archives, converted weights,
and generated references are local build artifacts and must never be committed.

## Provenance and integrity

`models.toml` pins PaddleOCR release `v3.5.0` at immutable commit
`33cbdd9deb2e00f61e7966db70669b249c005a37`, the two mobile models, and the two server models.
Every URL uses Paddle's official `paddle-model-ecology.bj.bcebos.com` HTTPS host.

Paddle's model table publishes the archive URLs and model sizes but not cryptographic checksums.
The byte lengths and SHA-256 values in `models.toml` were established by a reviewed download from
those exact official HTTPS URLs on 2026-07-27. They make future exports reproducible and detect
changed responses; they are not publisher-signed or independently authenticated checksums. See
`assets/ocr/pp-ocrv5-mobile/NOTICE.md` for the complete record.

The downloader honors the standard `HTTP_PROXY`, `HTTPS_PROXY`, and `NO_PROXY` environment
variables through Python's standard HTTPS client. It verifies the pinned byte length and SHA-256
before extraction and rejects redirects away from the exact pinned URL. Extraction rejects
absolute and traversing paths, backslash paths, duplicate members, file/directory prefix
collisions, links, devices, every pax and legacy-GNU sparse representation, unexpected member
counts, and oversized payloads before writing any member.

## Locked environment

The production export and verification CLIs fail closed unless they run under exactly CPython
3.11. Install only the exact, hashed wheels from the official PyPI index:

```bash
/opt/homebrew/bin/uv venv --python 3.11 .venv-ocr-export
.venv-ocr-export/bin/python -m pip install \
  --require-hashes \
  -r tools/ocr-export/requirements.lock
```

`requirements.in` records the direct dependency choices. `requirements.lock` pins the complete
transitive graph, carries hashes for published wheels, specifies `https://pypi.org/simple`, and
disables source-distribution builds. `models.toml` independently records the reviewed Python,
PaddlePaddle, Paddle2ONNX, ONNX, and ONNX Runtime versions. Before any download or bundle work,
the tools require the config, lock, installed distributions, verifier constants, and manifest to
agree exactly.

## Export and verification

The output path is mandatory and must not already exist:

```bash
.venv-ocr-export/bin/python tools/ocr-export/export.py \
  --profile lightweight \
  --output .local-models/pp-ocrv5-mobile
```

For a proxy at the user-approved local endpoint:

```bash
HTTPS_PROXY=http://127.0.0.1:7897 \
HTTP_PROXY=http://127.0.0.1:7897 \
.venv-ocr-export/bin/python tools/ocr-export/export.py \
  --profile lightweight \
  --output .local-models/pp-ocrv5-mobile
```

The exporter:

1. downloads and verifies the selected detector and recognizer archives;
2. safely extracts only the three expected Paddle inference files per model;
3. invokes the installed `paddle2onnx` executable as a checked subprocess with opset 18 and its
   checker enabled; the executable must be a non-symlinked regular executable inside the active
   venv's `bin`/`Scripts` directory, PATH fallback is forbidden, and optional optimization is
   disabled so the converter cannot auto-install undeclared packages from a secondary index;
4. fixes only the detector's single-image batch metadata, adds a final `[0, 1]` clamp for harmless
   sigmoid roundoff, runs the ONNX checker, requires float32 graph inputs and outputs, and enforces
   dynamic spatial shapes;
5. rasterizes fixed, non-sensitive simplified-Chinese (`安全`), traditional-Chinese (`繁體`), and
   English (`SAFE`) samples in memory without loading user data;
6. compares Paddle and ONNX outputs with `3e-4` relative and `1e-4` absolute tolerance while
   keeping ONNX-reference replay at a strict `1e-4` absolute tolerance;
7. writes exact per-file length/SHA-256 metadata and an aggregate digest into `manifest.json`; and
8. launches `verify.py` as a checked subprocess before publishing the new directory.

Reference inputs use `KOR1`, followed by a little-endian `u32` rank, little-endian `u32`
dimensions, and little-endian finite `f32` values. Reference outputs are strict JSON objects with
exactly `shape` and `values`.

Re-run both independent verifiers before packaging:

```bash
.venv-ocr-export/bin/python tools/ocr-export/verify.py \
  .local-models/pp-ocrv5-mobile/manifest.json
cargo run -p karma-onnx --example verify_ocr_bundle -- \
  .local-models/pp-ocrv5-mobile/manifest.json
```

The Python verifier rechecks every file and aggregate digest, the current Rust manifest contract,
ONNX graph contracts, and reference inference. The Rust verifier loads the complete
`VerifiedOcrBundle` and creates the runtime engine, which repeats reference inference through the
production ONNX Runtime path. The Python verifier first stats and bounded-reads the manifest, then
requires the exact directory tree and rejects every non-regular entry before touching declared
artifacts. It stats all assets against the Rust runtime's 1 MiB manifest, 256 MiB
per-model/reference, 4 MiB dictionary, and 64 KiB license caps before hashing, parsing reference
JSON, loading ONNX, or starting ORT. Artifact reads are bounded and hashes stream with exact
declared-length enforcement.

`assets/ocr/pp-ocrv5-mobile/manifest.example.json` is a schema-review example only. Its zero
digests, one-byte lengths, and one-entry dictionary are deliberately non-production sample values;
the exporter writes all production values from the verified local artifacts.

Archives are cached under `.local-models/.cache/ocr-export` when the documented output layout is
used. `.local-models`, `.venv-ocr-export`, Python caches, archives, `.part` files, ONNX weights, and
generated reference files are ignored by Git.
