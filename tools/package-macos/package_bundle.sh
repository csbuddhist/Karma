#!/usr/bin/env bash
# Builds the unsigned (ad-hoc signed) Karma macOS test bundle for this Mac's
# architecture into target/macos-test/Karma-<version>-macos and a zip next to it.
#
# KARMA_MODELS_DIR       verified model trees (default: release/windows-x64-test/models;
#                        the ONNX models and manifests are platform independent)
# KARMA_CODESIGN_IDENTITY a Developer ID identity; default "-" signs ad hoc
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
tools="$repo/tools/package-macos"
version="$(/usr/bin/plutil -extract version raw -o - "$repo/apps/karma-ui/src-tauri/tauri.conf.json")"
models="${KARMA_MODELS_DIR:-$repo/release/windows-x64-test/models}"
identity="${KARMA_CODESIGN_IDENTITY:--}"
console_app="Karma Family Protection.app"
out="$repo/target/macos-test/Karma-$version-macos"

for model in image/viddexa-nano ocr/pp-ocrv5-mobile; do
  if [[ ! -f "$models/$model/manifest.json" ]]; then
    echo "missing verified model tree: $models/$model" >&2
    exit 1
  fi
done

cd "$repo"
# Like the Windows release, build the console with cargo instead of the Tauri
# bundler; the frontend is embedded from apps/karma-ui/dist at compile time.
(
  cd apps/karma-ui
  npm ci --no-audit --no-fund
  npm run build
)
# Separate invocations: building the daemon with the agent would unify
# karma-macos features and link the capture frameworks into the root daemon.
cargo build --release --locked -p karma-service-macos
cargo build --release --locked -p karma-agent-macos -p karma-ui

rm -rf "$out" "$out.zip"
mkdir -p "$out/KarmaAgent.app/Contents/MacOS" "$out/$console_app/Contents/MacOS" \
  "$out/$console_app/Contents/Resources" "$out/launchd" "$out/models"
install -m 0755 target/release/KarmaService target/release/KarmaControl "$out/"
install -m 0755 target/release/karma-agent-macos "$out/KarmaAgent.app/Contents/MacOS/"
sed "s/__VERSION__/$version/g" "$tools/KarmaAgent.Info.plist" >"$out/KarmaAgent.app/Contents/Info.plist"
install -m 0755 target/release/karma-ui "$out/$console_app/Contents/MacOS/"
sed "s/__VERSION__/$version/g" "$tools/Console.Info.plist" >"$out/$console_app/Contents/Info.plist"
install -m 0644 apps/karma-ui/src-tauri/icons/icon.icns "$out/$console_app/Contents/Resources/"
for model in image/viddexa-nano ocr/pp-ocrv5-mobile; do
  mkdir -p "$out/models/$model"
  /usr/bin/ditto "$models/$model" "$out/models/$model"
done
install -m 0644 "$tools"/launchd/*.plist "$out/launchd/"
install -m 0755 "$tools/scripts/install.sh" "$tools/scripts/uninstall.sh" "$out/"
install -m 0644 "$tools/README.bundle.md" "$out/README.md"
install -m 0644 LICENSE "$out/LICENSE"

runtime=()
if [[ "$identity" != "-" ]]; then
  runtime=(--options runtime --timestamp)
fi
sign() {
  /usr/bin/codesign --force --sign "$identity" "${runtime[@]+"${runtime[@]}"}" "$@"
}
sign --identifier com.karma.service "$out/KarmaService"
sign --identifier com.karma.control "$out/KarmaControl"
sign "$out/KarmaAgent.app"
sign "$out/$console_app"

(
  cd "$out"
  find . -type f ! -name SHA256SUMS -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256 >SHA256SUMS
)
(cd "$(dirname "$out")" && /usr/bin/ditto -c -k --keepParent "$(basename "$out")" "$(basename "$out").zip")
bash "$tools/test_bundle_contract.sh" "$out"
echo "$out"
