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
$secure = Read-Host '请输入 Karma 管理员密码以授权卸载' -AsSecureString
$bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
$previousOutputEncoding = $OutputEncoding
try {
    $OutputEncoding = New-Object Text.UTF8Encoding($false)
    $plain = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    $plain | & $control shutdown
    if ($LASTEXITCODE -ne 0) {
        if ($LASTEXITCODE -eq 4) {
            Write-Host 'KarmaService 未在运行或无法连接，跳过 shutdown 授权并继续卸载。' -ForegroundColor Yellow
        } else {
            throw "KarmaControl 退出码 $LASTEXITCODE：管理员密码验证失败或无法读取密码，卸载已取消。"
        }
    }
} finally {
    $plain = $null
    $OutputEncoding = $previousOutputEncoding
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
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

$service = Get-Service -Name 'KarmaService' -ErrorAction SilentlyContinue
if ($null -ne $service) {
    try {
        $service.WaitForStatus('Stopped', [TimeSpan]::FromSeconds(20))
    } finally {
        $service.Dispose()
        $service = $null
    }
    & sc.exe delete KarmaService | Out-Null
    $deleteExitCode = $LASTEXITCODE
    if ($deleteExitCode -notin @(0, 1060, 1072)) { throw '删除 KarmaService 注册失败。' }
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
