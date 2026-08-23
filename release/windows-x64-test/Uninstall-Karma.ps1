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
    if ($LASTEXITCODE -ne 0) { throw '管理员密码验证失败，卸载已取消。' }
} finally {
    $plain = $null
    $OutputEncoding = $previousOutputEncoding
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
}

$consolePath = [IO.Path]::GetFullPath((Join-Path $destination 'karma-ui.exe'))
$consoleProcesses = @(Get-Process -Name 'karma-ui' -ErrorAction SilentlyContinue)
foreach ($process in $consoleProcesses) {
    try {
        $processPath = [IO.Path]::GetFullPath($process.Path)
    } catch {
        continue
    }
    if ($processPath -ine $consolePath) {
        continue
    }
    try {
        if (-not $process.HasExited) {
            Stop-Process -Id $process.Id -Force -ErrorAction Stop
            $process.WaitForExit(5000) | Out-Null
        }
    } catch {
        if (-not $process.HasExited) {
            throw '无法关闭 Karma 管理控制台，卸载已取消。'
        }
    } finally {
        $process.Dispose()
    }
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
    Remove-Item -LiteralPath $destination -Recurse -Force
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
