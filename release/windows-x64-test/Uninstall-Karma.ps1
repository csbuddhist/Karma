[CmdletBinding()]
param(
    [string] $InstallDirectory = (Join-Path $env:ProgramFiles 'Karma'),
    [switch] $PurgeData
)

$ErrorActionPreference = 'Stop'
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw '请以管理员身份运行 PowerShell，再执行 Uninstall-Karma.ps1。'
}

$destination = [IO.Path]::GetFullPath($InstallDirectory).TrimEnd('\')
$expectedDefault = [IO.Path]::GetFullPath((Join-Path $env:ProgramFiles 'Karma')).TrimEnd('\')
if ($destination -ne $expectedDefault -and (Split-Path -Leaf $destination) -ne 'Karma') {
    throw '拒绝删除未明确标识为 Karma 的安装目录。'
}

$control = Join-Path $destination 'KarmaControl.exe'
if (-not (Test-Path -LiteralPath $control -PathType Leaf)) {
    throw "缺少卸载授权程序：$control"
}
$secure = Read-Host 'Super password, or administrator password after the waiting period / 超级密码，或等待期满后的管理员密码（未设置超级密码时使用管理员密码）' -AsSecureString
$bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
$previousOutputEncoding = $OutputEncoding
try {
    $OutputEncoding = New-Object Text.UTF8Encoding($false)
    $plain = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    $plain | & $control shutdown
    if ($LASTEXITCODE -ne 0) {
        throw "KarmaControl exit code / 退出码 $LASTEXITCODE：authorization failed or service unreachable; uninstall cancelled / 授权失败或服务无法连接，卸载已取消。"
    }
} finally {
    $plain = $null
    $OutputEncoding = $previousOutputEncoding
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
}

# 密码授权通过后，把两个服务的 ACL 恢复为“管理员可停止”。安装时移除了管理员的停止权限，
# 若不恢复，下面的 Stop-Service 会因拒绝访问而失败。管理员仍保留 WRITE_DAC(WD)，因此此处
# sdset 可以成功；这一步只在密码校验通过之后执行，不会削弱日常的 GUI 防篡改。
$permissiveSddl = 'D:(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLORC;;;AU)'
foreach ($svc in @('KarmaService', 'KarmaGuardian')) {
    if ($null -ne (Get-Service -Name $svc -ErrorAction SilentlyContinue)) {
        & sc.exe sdset $svc $permissiveSddl | Out-Null
    }
}

# 必须先移除 KarmaGuardian，再停止/删除 KarmaService：否则守护服务会在卸载过程中
# 重新拉起 KarmaService。此时上面的 KarmaControl shutdown 已令 KarmaService 进入关闭，
# 其内部的 guardian 镜像看护线程随之结束，不会再把 KarmaGuardian 拉起；而镜像看护只会
# 启动已存在的服务，删除之后无法重建。
$guardian = Get-Service -Name 'KarmaGuardian' -ErrorAction SilentlyContinue
if ($null -ne $guardian) {
    try {
        if ($guardian.Status -ne 'Stopped') {
            Stop-Service -Name 'KarmaGuardian' -Force -ErrorAction SilentlyContinue
            $guardian.WaitForStatus('Stopped', [TimeSpan]::FromSeconds(20))
        }
    } catch {
    } finally {
        $guardian.Dispose()
        $guardian = $null
    }
    & sc.exe delete KarmaGuardian | Out-Null
    $guardianDeleteExitCode = $LASTEXITCODE
    if ($guardianDeleteExitCode -notin @(0, 1060, 1072)) { throw '删除 KarmaGuardian 注册失败。' }
}

function Stop-KarmaProcess {
    param(
        [string] $Name,
        [string] $Directory
    )
    foreach ($process in @(Get-Process -Name $Name -ErrorAction SilentlyContinue)) {
        $processPath = $null
        try {
            $processPath = [IO.Path]::GetFullPath($process.Path)
        } catch {
            # 路径读取失败（例如进程属于其他会话）时跳过路径校验，直接按名称结束：
            # karma-ui、karma-agent-windows、KarmaControl 都是 Karma 专用进程名。
        }
        if ($processPath -and -not $processPath.StartsWith($Directory + '\', [StringComparison]::OrdinalIgnoreCase)) {
            $process.Dispose()
            continue
        }
        try {
            if (-not $process.HasExited) {
                Stop-Process -Id $process.Id -Force -ErrorAction Stop
            }
        } catch {
        } finally {
            $process.Dispose()
        }
    }
}

# 密码验证通过后：先结束控制台、Agent 与卸载授权程序，再等待它们真正退出，
# 否则可执行文件仍被占用，目录删除会失败。
$processNames = @('karma-ui', 'karma-agent-windows', 'KarmaControl')
Stop-KarmaProcess -Name 'karma-ui' -Directory $destination
Stop-KarmaProcess -Name 'karma-agent-windows' -Directory $destination
Stop-KarmaProcess -Name 'KarmaControl' -Directory $destination
$deadline = (Get-Date).AddSeconds(10)
do {
    $remaining = @()
    foreach ($name in $processNames) {
        $remaining += @(Get-Process -Name $name -ErrorAction SilentlyContinue | Where-Object {
            try { -not $_.HasExited } catch { $false }
        })
    }
    if ($remaining.Count -eq 0) { break }
    Start-Sleep -Milliseconds 500
} while ((Get-Date) -lt $deadline)
if ($remaining.Count -gt 0) {
    throw 'Karma 相关进程未能全部退出，文件仍被占用，卸载已取消。'
}

$autoStartName = 'Karma Family Protection'
Remove-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name $autoStartName -ErrorAction SilentlyContinue
Remove-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run' -Name $autoStartName -ErrorAction SilentlyContinue

$networkCleaned = $false
$service = Get-Service -Name 'KarmaService' -ErrorAction SilentlyContinue
if ($null -ne $service) {
    try {
        # KarmaControl shutdown 通常已令服务自行停止；若因服务曾被守护拉起而仍在运行，
        # 此处显式停止（ACL 已在上方恢复，管理员此刻可停止），确保 sc delete 前达到 Stopped。
        if ($service.Status -ne 'Stopped') {
            Stop-Service -Name 'KarmaService' -Force -ErrorAction SilentlyContinue
        }
        $service.WaitForStatus('Stopped', [TimeSpan]::FromSeconds(20))
    } finally {
        $service.Dispose()
        $service = $null
    }
    # Clear only Karma-owned persistent WFP objects after the service has stopped.
    # 服务停止后仅清理 Karma 自己的持久 WFP 对象，失败时保留安装文件供恢复。
    & (Join-Path $destination 'KarmaService.exe') --clear-network-filters
    if ($LASTEXITCODE -ne 0) { throw 'WFP cleanup failed / WFP 清理失败，卸载已取消。' }
    $networkCleaned = $true
    & sc.exe delete KarmaService | Out-Null
    $deleteExitCode = $LASTEXITCODE
    if ($deleteExitCode -notin @(0, 1060, 1072)) { throw '删除 KarmaService 注册失败。' }
}

$networkCleaner = Join-Path $destination 'KarmaService.exe'
if (-not $networkCleaned -and (Test-Path -LiteralPath $networkCleaner -PathType Leaf)) {
    & $networkCleaner --clear-network-filters
    if ($LASTEXITCODE -ne 0) { throw 'WFP cleanup failed / WFP 清理失败，卸载已取消。' }
}

if (Test-Path -LiteralPath $destination) {
    # 强制结束进程后句柄释放存在延迟，重试若干次再判定失败。
    $lastRemoveError = $null
    for ($attempt = 1; $attempt -le 3; $attempt++) {
        try {
            Remove-Item -LiteralPath $destination -Recurse -Force -ErrorAction Stop
            $lastRemoveError = $null
            break
        } catch {
            $lastRemoveError = $_
            Start-Sleep -Seconds 2
        }
    }
    if ($null -ne $lastRemoveError) {
        throw "无法删除安装目录 $destination：$($lastRemoveError.Exception.Message)"
    }
}
if ($PurgeData) {
    $dataDirectory = Join-Path $env:ProgramData 'Karma'
    if (Test-Path -LiteralPath $dataDirectory) {
        Remove-Item -LiteralPath $dataDirectory -Recurse -Force
    }
    Write-Host 'Karma 程序和本地策略、审计、加密证据均已删除。' -ForegroundColor Yellow
} else {
    Write-Host 'Karma 程序已卸载；ProgramData\Karma 中的策略和加密证据已保留。' -ForegroundColor Green
}
