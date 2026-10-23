#!/usr/bin/env bash
# Checks that the macOS packaging sources agree with the Rust constants they
# depend on and, given a built bundle directory, that the bundle is complete.
# Usage: bash tools/package-macos/test_bundle_contract.sh [bundle-directory]
set -euo pipefail

tools="tools/package-macos"
service="apps/karma-service-macos/src"

/usr/bin/plutil -lint "$tools"/launchd/*.plist "$tools/KarmaAgent.Info.plist" "$tools/Console.Info.plist" >/dev/null
for script in "$tools"/scripts/*.sh "$tools"/*.sh; do
  bash -n "$script"
done

plist_value() {
  /usr/bin/plutil -extract "$2" raw -o - "$1"
}

# launchd jobs must start exactly what KarmaService trusts and restarts.
test "$(plist_value "$tools/launchd/com.karma.service.plist" Label)" = "com.karma.service"
test "$(plist_value "$tools/launchd/com.karma.service.plist" ProgramArguments.0)" = "/Library/Karma/KarmaService"
test "$(plist_value "$tools/launchd/com.karma.service.plist" KeepAlive.SuccessfulExit)" = "false"
test "$(plist_value "$tools/launchd/com.karma.agent.plist" Label)" = "com.karma.agent"
test "$(plist_value "$tools/launchd/com.karma.agent.plist" ProgramArguments.0)" = \
  "/Library/Karma/KarmaAgent.app/Contents/MacOS/karma-agent-macos"
test "$(plist_value "$tools/launchd/com.karma.agent.plist" LimitLoadToSessionType)" = "Aqua"
test "$(plist_value "$tools/KarmaAgent.Info.plist" CFBundleExecutable)" = "karma-agent-macos"
test "$(plist_value "$tools/Console.Info.plist" CFBundleIdentifier)" = \
  "$(/usr/bin/plutil -extract identifier raw -o - apps/karma-ui/src-tauri/tauri.conf.json)"
grep -q 'const AGENT_LABEL: &str = "com.karma.agent";' "$service/watchdog.rs"
grep -q 'const LAUNCH_AGENT_PLIST: &str = "/Library/LaunchAgents/com.karma.agent.plist";' "$service/main.rs"
grep -q 'const AGENT_IN_INSTALL: &str = "KarmaAgent.app/Contents/MacOS/karma-agent-macos";' "$service/main.rs"
grep -q 'const DATA_DIRECTORY: &str = "/Library/Application Support/Karma";' "$service/main.rs"
grep -q 'pub const SERVICE_SOCKET_PATH: &str = "/var/run/karma/service.sock";' crates/karma-macos-ipc/src/lib.rs
grep -q 'sudo /Library/Karma/uninstall.sh' apps/karma-ui/src/i18n.ts

# Install verifies the bundle, never overwrites a running installation, and
# leaves only root able to change what launchd runs.
grep -q 'EUID -ne 0' "$tools/scripts/install.sh"
grep -q 'shasum -a 256 -c SHA256SUMS' "$tools/scripts/install.sh"
grep -q 'Karma is already installed' "$tools/scripts/install.sh"
grep -q 'chown -R root:wheel' "$tools/scripts/install.sh"
# Uninstall asks KarmaService for authorization before touching launchd.
grep -q 'EUID -ne 0' "$tools/scripts/uninstall.sh"
shutdown_line="$(grep -n '"$control" shutdown' "$tools/scripts/uninstall.sh" | cut -d: -f1)"
bootout_line="$(grep -n 'launchctl bootout' "$tools/scripts/uninstall.sh" | head -1 | cut -d: -f1)"
test -n "$shutdown_line" && test "$shutdown_line" -lt "$bootout_line"

if [[ $# -eq 0 ]]; then
  exit 0
fi

bundle="$1"
for item in KarmaService KarmaControl install.sh uninstall.sh; do
  test -x "$bundle/$item"
done
for item in README.md LICENSE SHA256SUMS launchd/com.karma.service.plist launchd/com.karma.agent.plist \
  KarmaAgent.app/Contents/Info.plist "Karma Family Protection.app/Contents/Info.plist"; do
  test -f "$bundle/$item"
done
test -x "$bundle/KarmaAgent.app/Contents/MacOS/karma-agent-macos"
test -x "$bundle/Karma Family Protection.app/Contents/MacOS/karma-ui"
if ! grep -a -F -q '../dist' "$bundle/Karma Family Protection.app/Contents/MacOS/karma-ui"; then
  echo "karma-ui does not contain the production frontendDist marker" >&2
  exit 1
fi
test "$(plist_value "$bundle/KarmaAgent.app/Contents/Info.plist" CFBundleIdentifier)" = "com.karma.agent"
version="$(/usr/bin/plutil -extract version raw -o - apps/karma-ui/src-tauri/tauri.conf.json)"
test "$(plist_value "$bundle/KarmaAgent.app/Contents/Info.plist" CFBundleShortVersionString)" = "$version"
test "$("$bundle/KarmaControl" version)" = "$version"
for artifact in model.onnx manifest.json reference-output.json LICENSE; do
  test -f "$bundle/models/image/viddexa-nano/$artifact"
done
for artifact in manifest.json detector.onnx recognizer.onnx dictionary.txt LICENSE NOTICE.md; do
  test -f "$bundle/models/ocr/pp-ocrv5-mobile/$artifact"
done
cmp "$bundle/install.sh" "$tools/scripts/install.sh"
cmp "$bundle/uninstall.sh" "$tools/scripts/uninstall.sh"
cmp "$bundle/README.md" "$tools/README.bundle.md"
for code in KarmaService KarmaControl KarmaAgent.app "Karma Family Protection.app"; do
  /usr/bin/codesign --verify --strict "$bundle/$code"
done
# The root daemon must not link the capture frameworks.
if /usr/bin/otool -L "$bundle/KarmaService" | grep -q ScreenCaptureKit; then
  echo "KarmaService links ScreenCaptureKit" >&2
  exit 1
fi
(cd "$bundle" && /usr/bin/shasum -a 256 -c SHA256SUMS >/dev/null)
