#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
installer="$script_dir/KarmaInstaller.nsi"
builder="$script_dir/build_installer.sh"
install_script="$repo_root/release/windows-x64-test/Install-Karma.ps1"
uninstall_script="$repo_root/release/windows-x64-test/Uninstall-Karma.ps1"
cleanup_script="$script_dir/Cleanup-InstallDirectory.ps1"
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
assert_contains "sc\.exe failure KarmaService 'reset=' '0' 'actions=' 'restart/1000/restart/3000/restart/10000'" "$install_script" \
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
