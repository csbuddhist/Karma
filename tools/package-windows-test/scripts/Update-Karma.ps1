[CmdletBinding()]
param(
    [string] $BundleDirectory = $PSScriptRoot,
    [string] $InstallDirectory = (Join-Path $env:ProgramFiles 'Karma'),
    [Parameter(Mandatory)] [string] $ExpectedVersion
)

$ErrorActionPreference = 'Stop'
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run as administrator / 请以管理员身份运行。'
}
if ($ExpectedVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid version / 版本格式无效。' }
$control = Join-Path $InstallDirectory 'KarmaControl.exe'
$installedVersion = & $control version
if ($LASTEXITCODE -ne 0) { throw 'This installation does not support in-place updates / 此安装不支持原地升级。' }
if ([version] $ExpectedVersion -le [version] $installedVersion) {
    throw 'Only a newer release can be installed / 只允许安装更高版本。'
}

# The service creates the inbox with an administrator-writable ACL. Never
# change the ACL of the service data root or its private staging directory.
# 服务创建管理员可写的收件箱；不得改变数据根目录或私有暂存区的权限。
$inbox = Join-Path $env:ProgramData 'Karma\update-inbox'
if (-not (Test-Path -LiteralPath $inbox -PathType Container)) {
    throw 'Update inbox unavailable; start KarmaService and retry / 升级收件箱不可用，请启动服务后重试。'
}
$inboxItem = Get-Item -LiteralPath $inbox -Force
if (($inboxItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'Unsafe update inbox / 升级收件箱不能是链接。'
}
foreach ($file in @('release-manifest.json', 'release-manifest.json.sig')) {
    if (-not (Test-Path -LiteralPath (Join-Path $BundleDirectory $file) -PathType Leaf)) {
        throw "Missing signed release file / 缺少签名文件：$file"
    }
}
Get-ChildItem -LiteralPath $inbox -Force | Remove-Item -Recurse -Force
Get-ChildItem -LiteralPath $BundleDirectory -Force | Copy-Item -Destination $inbox -Recurse -Force

# Password never enters a command-line argument. The service verifies signed
# copies in its SYSTEM-only staging area before touching installed files.
# 密码不进入命令行参数；服务先在仅 SYSTEM 可访问的暂存区验签和复验哈希。
$secure = Read-Host 'Karma administrator password / Karma 管理员密码' -AsSecureString

# The console hides to the tray instead of exiting, and its running image
# blocks replacement of karma-ui.exe. Installed 0.2.4-0.3.0 updaters only ask
# it to close gracefully, so close it here first (it holds no data).
# 控制台关闭时只会隐藏到托盘，其运行中的映像会阻止替换 karma-ui.exe；
# 0.2.4-0.3.0 的服务端升级器只会请求正常关闭，因此这里先结束控制台（不持有数据）。
# Setup runs this script in 32-bit PowerShell, where Get-Process cannot read a
# 64-bit process path; CIM reports ExecutablePath for either bitness.
# 安装器以 32 位 PowerShell 运行本脚本，Get-Process 读不到 64 位进程路径；CIM 可以。
$installRoot = [IO.Path]::GetFullPath($InstallDirectory).TrimEnd('\') + '\'
function Get-InstalledConsole {
    @(Get-CimInstance -ClassName Win32_Process -Filter "Name = 'karma-ui.exe'" -ErrorAction SilentlyContinue |
        Where-Object {
            $_.ExecutablePath -and
            [IO.Path]::GetFullPath($_.ExecutablePath).StartsWith($installRoot, [StringComparison]::OrdinalIgnoreCase)
        })
}
foreach ($console in Get-InstalledConsole) {
    Stop-Process -Id $console.ProcessId -Force -ErrorAction SilentlyContinue
}
$consoleDeadline = (Get-Date).AddSeconds(10)
while ((Get-InstalledConsole).Count -gt 0 -and (Get-Date) -lt $consoleDeadline) {
    Start-Sleep -Milliseconds 500
}
# Never request the update while the console still runs: older updaters would
# then fail to roll back and keep the service in recovery.
# 控制台仍在运行时绝不提交升级：旧版升级器会回滚失败并让服务反复进入恢复。
if ((Get-InstalledConsole).Count -gt 0) {
    throw 'The Karma console is still running; close it and retry. Nothing was changed / Karma 控制台仍在运行，请关闭后重试，未做任何更改。'
}

function Get-KarmaServiceProcessId {
    $service = Get-CimInstance -ClassName Win32_Service -Filter "Name = 'KarmaService'" -ErrorAction SilentlyContinue
    if ($null -eq $service) { return 0 }
    return [int] $service.ProcessId
}
$originalServiceProcessId = Get-KarmaServiceProcessId

$bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
$previousOutputEncoding = $OutputEncoding
try {
    $OutputEncoding = New-Object Text.UTF8Encoding($false)
    $plain = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    $plain | & $control update
    if ($LASTEXITCODE -ne 0) { throw 'Update rejected / 升级请求被拒绝。' }
} finally {
    $plain = $null
    $OutputEncoding = $previousOutputEncoding
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
}

# Do not execute installed files while the service replaces them: a running
# KarmaControl.exe would lock its own image and make the update roll back.
# Watch SCM state only, then probe the version once both services are stable.
# 替换期间不能运行已安装的程序：运行中的 KarmaControl.exe 会锁定自身文件导致升级回滚。
# 先只观察服务状态，待两个服务稳定运行后再读取版本。
function Test-KarmaServicesRunning {
    foreach ($name in @('KarmaService', 'KarmaGuardian')) {
        $service = Get-Service -Name $name -ErrorAction SilentlyContinue
        if ($null -eq $service -or $service.Status -ne 'Running') { return $false }
    }
    return $true
}

# The service restarts within seconds, so a stop between polls is easy to
# miss; a new KarmaService process id also proves the handoff happened.
# 服务几秒内即重启，轮询容易错过停止状态；服务进程号变化同样说明已开始升级。
$stopDeadline = (Get-Date).AddSeconds(180)
while ((Test-KarmaServicesRunning) -and
    (Get-KarmaServiceProcessId) -eq $originalServiceProcessId -and
    (Get-Date) -lt $stopDeadline) {
    Start-Sleep -Seconds 1
}
$deadline = (Get-Date).AddSeconds(240)
do {
    Start-Sleep -Seconds 3
    if (-not (Test-KarmaServicesRunning)) { continue }
    Start-Sleep -Seconds 5
    if (-not (Test-KarmaServicesRunning)) { continue }
    $runningVersion = $null
    try {
        $runningVersion = & $control version
        if ($LASTEXITCODE -ne 0) { $runningVersion = $null }
    } catch {
        # Restart may briefly make the executable unavailable. 重启期间可能短暂不可用。
    }
    if ($runningVersion -eq $ExpectedVersion) {
        Write-Host "Updated / 已升级：$ExpectedVersion" -ForegroundColor Green
        exit 0
    }
    if ($runningVersion -eq $installedVersion) {
        throw 'Update was not applied or was rolled back; protection remains on the previous version. See the console update status / 升级未应用或已回滚，保护仍运行旧版本，请查看控制台升级状态。'
    }
} while ((Get-Date) -lt $deadline)
throw 'Update not confirmed; inspect the console update status. Protection remains registered / 升级尚未确认，请查看控制台状态，保护服务仍保持注册。'
