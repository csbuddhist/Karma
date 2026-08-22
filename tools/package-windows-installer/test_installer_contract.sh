#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
installer="$script_dir/KarmaInstaller.nsi"
builder="$script_dir/build_installer.sh"

test -f "$installer"
test -f "$builder"
rg -q 'RequestExecutionLevel admin' "$installer"
rg -q '\$PROGRAMFILES64\\Karma' "$installer"
rg -q 'Verify-KarmaTestBundle' "$script_dir/../../release/windows-x64-test/Install-Karma.ps1"
rg -q 'Uninstall-Karma\.ps1' "$installer"
rg -q 'KarmaControl\.exe' "$script_dir/../../release/windows-x64-test/Uninstall-Karma.ps1"
rg -q 'Uninstall-Karma-Launcher\.exe' "$installer"
rg -q 'SYSTEM\\CurrentControlSet\\Services\\KarmaService' "$installer"
rg -q -- '-NonInteractive -ExecutionPolicy Bypass' "$installer"
rg -q 'ExecWait.*Uninstall-Karma\.ps1' "$installer"
if rg -q 'MUI_FINISHPAGE_RUN ' "$installer"; then
  echo "the elevated installer must not launch the console directly" >&2
  exit 1
fi
