#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
installer="$script_dir/KarmaInstaller.nsi"
builder="$script_dir/build_installer.sh"
install_script="$repo_root/release/windows-x64-test/Install-Karma.ps1"
contract_failed=0

assert_contains() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if ! rg -q "$pattern" "$file"; then
    echo "$message" >&2
    contract_failed=1
  fi
}

test -f "$installer"
test -f "$builder"
rg -q 'RequestExecutionLevel admin' "$installer"
rg -q '\$PROGRAMFILES64\\Karma' "$installer"
rg -q 'test_installer_contract\.sh' "$builder"
assert_contains 'Verify-KarmaTestBundle' "$install_script" \
  'installer must verify the Windows bundle before registering the service'
assert_contains 'if \(-not \$\?\)' "$install_script" \
  'installer must use PowerShell success state after invoking the verification script'
assert_contains "sc\.exe create KarmaService 'binPath=' .* 'start=' 'delayed-auto' 'DisplayName=' 'Karma Family Protection'" "$install_script" \
  'installer must pass sc.exe option names and values as separate arguments'
assert_contains "sc\.exe failure KarmaService 'reset=' '0' 'actions=' 'restart/1000/restart/3000/restart/10000'" "$install_script" \
  'installer must pass sc.exe recovery option names and values as separate arguments'
rg -q 'Uninstall-Karma\.ps1' "$installer"
rg -q 'KarmaControl\.exe' "$repo_root/release/windows-x64-test/Uninstall-Karma.ps1"
rg -q 'Uninstall-Karma-Launcher\.exe' "$installer"
rg -q 'SYSTEM\\CurrentControlSet\\Services\\KarmaService' "$installer"
rg -q -- '-NonInteractive -ExecutionPolicy Bypass' "$installer"
rg -q 'ExecWait.*Uninstall-Karma\.ps1' "$installer"
if rg -q 'MUI_FINISHPAGE_RUN ' "$installer"; then
  echo "the elevated installer must not launch the console directly" >&2
  exit 1
fi

if ! cargo_tree="$(cd "$repo_root" && cargo tree -p karma-ui -e features)"; then
  echo "failed to resolve karma-ui release features" >&2
  contract_failed=1
elif ! rg -q 'tauri feature "custom-protocol"' <<<"$cargo_tree"; then
  echo "karma-ui must enable Tauri custom-protocol so release builds embed frontend assets" >&2
  contract_failed=1
fi

exit "$contract_failed"
