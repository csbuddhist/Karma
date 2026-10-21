#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
installer="$script_dir/KarmaInstaller.nsi"
builder="$script_dir/build_installer.sh"
install_script="$repo_root/release/windows-x64-test/Install-Karma.ps1"
uninstall_script="$repo_root/release/windows-x64-test/Uninstall-Karma.ps1"
cleanup_script="$script_dir/Cleanup-InstallDirectory.ps1"
update_script="$repo_root/release/windows-x64-test/Update-Karma.ps1"
runtime_script="$repo_root/release/windows-x64-test/Install-VCRuntime.ps1"
fetch_script="$script_dir/fetch_vc_redist.sh"
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

assert_before() {
  local earlier_pattern="$1"
  local later_pattern="$2"
  local file="$3"
  local message="$4"
  local earlier_line
  local later_line
  earlier_line="$(rg -n -m 1 "$earlier_pattern" "$file" | cut -d: -f1 || true)"
  later_line="$(rg -n -m 1 "$later_pattern" "$file" | cut -d: -f1 || true)"
  if [[ -z "$earlier_line" || -z "$later_line" || "$earlier_line" -ge "$later_line" ]]; then
    echo "$message" >&2
    contract_failed=1
  fi
}

test -f "$installer"
test -f "$builder"
test -f "$cleanup_script"
head -c 3 "$cleanup_script" | od -An -t x1 | tr -d ' \n' | grep -qx 'efbbbf' || {
  echo 'cleanup script must start with a UTF-8 BOM' >&2
  contract_failed=1
}
rg -q 'RequestExecutionLevel admin' "$installer"
rg -q '\$PROGRAMFILES64\\Karma' "$installer"
rg -q 'test_installer_contract\.sh' "$builder"
assert_contains 'Verify-KarmaTestBundle' "$install_script" \
  'installer must verify the Windows bundle before registering the service'
assert_contains 'if \(-not \$\?\)' "$install_script" \
  'installer must use PowerShell success state after invoking the verification script'
assert_contains "sc\.exe create KarmaService 'binPath=' .* 'start=' 'delayed-auto' 'DisplayName=' 'Karma Family Protection'" "$install_script" \
  'installer must pass sc.exe option names and values as separate arguments'
assert_contains "sc\.exe failure KarmaService 'reset=' '60' 'actions=' 'restart/1000/restart/1000/restart/1000'" "$install_script" \
  'installer must pass sc.exe recovery option names and values as separate arguments'
rg -q 'Uninstall-Karma\.ps1' "$installer"
rg -q 'KarmaControl\.exe' "$uninstall_script"
assert_contains 'MessageBox MB_YESNO.*是否先卸载现有版本并继续安装' "$installer" \
  'installer must ask before removing an existing installation'
# 升级必须直接同步运行已安装的密码授权卸载脚本：NSIS 卸载器复制到临时目录
# 异步执行，原进程总是立即退出 0，ExecWait 无法据此判断卸载是否成功。
assert_contains 'ExecWait .*-File "\$1\\Uninstall-Karma\.ps1".* \$2' "$installer" \
  'installer must run the installed password-authorized uninstall script synchronously'
if rg -q 'ExecWait .*Uninstall-Karma-Launcher\.exe.* /S' "$installer"; then
  echo 'installer must not wait on the NSIS uninstaller launcher: it always exits 0 immediately' >&2
  contract_failed=1
fi
assert_before 'MessageBox MB_YESNO.*是否先卸载现有版本并继续安装' 'ExecWait .*-File "\$1\\Uninstall-Karma\.ps1"' "$installer" \
  'installer must obtain confirmation before running the existing uninstall script'
assert_before 'ExecWait .*-File "\$1\\Uninstall-Karma\.ps1"' '\$2 != 0' "$installer" \
  'installer must wait for and validate the existing uninstall script result'
assert_contains '^wait_for_existing_service_removal:' "$installer" \
  'installer must poll for the existing service to disappear after uninstall succeeds'
assert_before '^wait_for_existing_service_removal:' '现有 KarmaService .*未能移除' "$installer" \
  'installer must wait for confirmed service removal before reporting failure'
# 卸载后、解包前必须清理遗留进程与被占用文件，否则 NSIS 会报“无法打开要写入的文件”。
assert_contains 'Karma-Cleanup\.ps1' "$installer" \
  'installer must run the leftover-process cleanup before extracting files'
assert_before 'ExecWait .*-File "\$1\\Uninstall-Karma\.ps1"' 'Karma-Cleanup\.ps1' "$installer" \
  'installer must clean up only after the existing uninstall script finishes'
assert_before 'Karma-Cleanup\.ps1' '^Section "Install"' "$installer" \
  'installer must clean up leftover processes before extracting files'
assert_contains "Stop-Process -Id" "$cleanup_script" \
  'cleanup script must force-stop Karma processes from the old install directory'
assert_contains 'Remove-Item -LiteralPath \$destination -Recurse -Force' "$cleanup_script" \
  'cleanup script must remove the old install directory when files remain'
if rg -q '请先运行现有安装目录中的 Uninstall-Karma-Launcher\.exe' "$installer"; then
  echo 'installer must not require the user to launch the existing uninstaller manually' >&2
  contract_failed=1
fi
assert_before 'Read-Host .*管理员密码' '& \$control shutdown' "$uninstall_script" \
  'uninstaller must ask for the Karma administrator password before requesting shutdown'
assert_before 'if \(\$LASTEXITCODE -ne 0\)' 'Get-Service .*KarmaService' "$uninstall_script" \
  'uninstaller must reject an incorrect password before inspecting or changing the service'
assert_before 'if \(\$LASTEXITCODE -ne 0\)' 'sc\.exe delete KarmaService' "$uninstall_script" \
  'uninstaller must authenticate successfully before deleting the service'
assert_before 'if \(\$LASTEXITCODE -ne 0\)' 'Remove-Item -LiteralPath \$destination' "$uninstall_script" \
  'uninstaller must authenticate successfully before deleting installed files'
assert_contains "Stop-KarmaProcess -Name 'karma-ui'" "$uninstall_script" \
  'uninstaller must locate the installed administration console before deleting its executable'
assert_contains "Stop-KarmaProcess -Name 'karma-agent-windows'" "$uninstall_script" \
  'uninstaller must also stop the session agent before deleting installed files'
assert_before 'if \(\$LASTEXITCODE -ne 0\)' 'Stop-Process -Id' "$uninstall_script" \
  'uninstaller must authenticate successfully before stopping the administration console'
assert_before 'Stop-Process -Id' 'Remove-Item -LiteralPath \$destination' "$uninstall_script" \
  'uninstaller must stop the installed administration console before deleting installed files'
assert_contains '未能全部退出' "$uninstall_script" \
  'uninstaller must wait for stopped processes to actually exit before deleting files'
assert_contains 'for \(\$attempt = 1; \$attempt -le 3; \$attempt\+\+\)' "$uninstall_script" \
  'uninstaller must retry the directory removal while process handles are released'
assert_before '\$service\.Dispose\(\)' 'sc\.exe delete KarmaService' "$uninstall_script" \
  'uninstaller must release its service handle before deleting KarmaService'
assert_contains '\$deleteExitCode -notin @\(0, 1060, 1072\)' "$uninstall_script" \
  'uninstaller must allow retrying a service that is absent or already marked for deletion'
assert_before "WaitForStatus\('Stopped'" 'clear-network-filters' "$uninstall_script" \
  'uninstaller must stop the service before clearing its persistent WFP policy'
assert_before 'clear-network-filters' 'sc\.exe delete KarmaService' "$uninstall_script" \
  'uninstaller must clear persistent WFP objects before deleting the service registration'
assert_contains 'WFP cleanup failed / WFP 清理失败，卸载已取消' "$uninstall_script" \
  'uninstaller must keep recovery binaries if persistent WFP cleanup fails'
# 单文件安装器必须内置微软 VC++ x64 运行库，并在注册服务前按需安装。
test -f "$runtime_script"
test -f "$fetch_script"
assert_contains 'VC_REDIST_FILE is required' "$installer" \
  'installer must refuse to build without the Microsoft VC++ redistributable'
assert_contains 'File /oname=\$PLUGINSDIR\\vc_redist\.x64\.exe' "$installer" \
  'installer must embed vc_redist.x64.exe in its temporary plug-in directory, not the install directory'
assert_contains 'Install-Karma\.ps1" -InstallDirectory "\$INSTDIR" -VCRedistPath "\$PLUGINSDIR\\vc_redist\.x64\.exe"' "$installer" \
  'installer must hand the embedded redistributable to Install-Karma.ps1'
assert_contains 'fetch_vc_redist\.sh' "$builder" \
  'installer build must fetch and verify the Microsoft redistributable'
assert_contains 'osslsigncode verify' "$fetch_script" \
  'redistributable fetch must verify the Authenticode signature'
assert_contains '847DF6A78497943F27FC72EB93F9A637320A02B561D0A91B09E87A7807ED7C61' "$fetch_script" \
  'redistributable fetch must pin the Microsoft code-signing root'
assert_before 'Install-VCRuntime\.ps1' "sc\.exe create KarmaService" "$install_script" \
  'Install-Karma.ps1 must satisfy the VC++ runtime before registering services'
assert_before 'Get-AuthenticodeSignature' 'Start-Process -FilePath \$RedistPath' "$runtime_script" \
  'runtime installer must verify the Microsoft signature before executing vc_redist'
assert_contains "'/install', '/quiet', '/norestart'" "$runtime_script" \
  'runtime installer must install silently without forcing a restart'
assert_contains '@\(0, 1638, 3010, 1641\)' "$runtime_script" \
  'runtime installer must accept already-installed and restart-required results'
assert_contains "\[version\]'14\.44\.0\.0'" "$runtime_script" \
  'runtime installer must require the 14.44 runtime the binaries are built against'
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

test -f "$update_script"
assert_contains 'KarmaControl\.exe" version' "$installer" \
  'installer must detect in-place update capability before considering legacy removal'
assert_contains 'StrCpy \$UpdateExisting 1' "$installer" \
  'new installations must use the in-place update branch'
assert_contains 'ExecWait .*Update-Karma\.ps1.*ExpectedVersion' "$installer" \
  'installer must wait for an authenticated update to the requested version'
assert_contains '\$plain \| & \$control update' "$update_script" \
  'update password must be passed via stdin, never on the command line'
assert_contains "Karma\\\\update-inbox" "$update_script" \
  'update script must copy to the fixed service inbox'
update_source="$repo_root/tools/package-windows-test/scripts/Update-Karma.ps1"
assert_before "Get-Process -Name 'karma-ui'" '\$plain \| & \$control update' "$update_source" \
  'update script must close the tray-resident console before older updaters replace karma-ui.exe'
assert_before 'while \(\(Test-KarmaServicesRunning\) -and' '\$runningVersion = & \$control version' "$update_source" \
  'update script must not run installed KarmaControl.exe while its image is being replaced'
assert_contains 'Super password.*超级密码' "$uninstall_script" \
  'uninstaller must explain super-password or delayed administrator authorization'
if rg -q '跳过 shutdown 授权|skip.*authorization' "$uninstall_script"; then
  echo 'uninstaller must fail closed when the service cannot authorize shutdown' >&2
  contract_failed=1
fi
if ! python3 - "$repo_root" <<'PY'
import json
import re
import sys
import tomllib
from pathlib import Path

root = Path(sys.argv[1])
ui = root / "apps/karma-ui"
version = json.loads((ui / "src-tauri/tauri.conf.json").read_text())["version"]
ipc = (root / "crates/karma-ipc/src/lib.rs").read_text()
assert re.search(r'PRODUCT_VERSION: &str = "([^"]+)"', ipc).group(1) == version
assert json.loads((ui / "package.json").read_text())["version"] == version
lock = json.loads((ui / "package-lock.json").read_text())
assert lock["version"] == version
assert lock["packages"][""]["version"] == version
assert tomllib.loads((ui / "src-tauri/Cargo.toml").read_text())["package"]["version"] == version
assert json.loads((root / "release/windows-x64-test/release-manifest.json").read_text())["version"] == version
print(f"product/console/manifest version contract verified: {version}")
PY
then
  echo 'product, console, and signed manifest versions must match' >&2
  contract_failed=1
fi

exit "$contract_failed"
