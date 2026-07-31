#!/usr/bin/env bash
set -euo pipefail

bundle_dir="release/windows-x64-test"

test -f "$bundle_dir/Start-KarmaTest.ps1"
test -f "$bundle_dir/Verify-KarmaTestBundle.ps1"
test -f "$bundle_dir/README.md"
rg -q 'KARMA_IMAGE_MODEL_MANIFEST' "$bundle_dir/Start-KarmaTest.ps1"
rg -q 'Get-FileHash' "$bundle_dir/Verify-KarmaTestBundle.ps1"

for artifact in model.onnx manifest.json reference-output.json LICENSE; do
  test -f "$bundle_dir/models/image/viddexa-nano/$artifact"
done

test -f "$bundle_dir/karma-agent-windows.exe"
test -f "$bundle_dir/DirectML.dll"
test -f "$bundle_dir/SHA256SUMS"
for artifact in manifest.json detector.onnx recognizer.onnx dictionary.txt LICENSE NOTICE.md; do
  test -f "$bundle_dir/models/ocr/pp-ocrv5-mobile/$artifact"
done

rg -q 'release/windows-x64-test/Start-KarmaTest.ps1' docs/windows-installation-guide.md
rg -q 'cloneable Windows test bundle' README.md
