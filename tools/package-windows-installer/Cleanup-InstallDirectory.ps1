[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $InstallDirectory
)

# 安装器在“先卸载后安装”流程的最后一步调用：旧版卸载脚本可能遗留
# 运行中的进程或被占用的文件（NSIS 无法向被占用的文件解包，会弹出
# “无法打开要写入的文件”）。此脚本强制结束安装目录内的 Karma 进程，
# 并在目录仍存在时彻底删除，任何失败都以非零退出码告知安装器。

$ErrorActionPreference = 'Stop'
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw '请以管理员身份运行 Cleanup-InstallDirectory.ps1。'
}

$destination = [IO.Path]::GetFullPath($InstallDirectory).TrimEnd('\')
$expectedDefault = [IO.Path]::GetFullPath((Join-Path $env:ProgramFiles 'Karma')).TrimEnd('\')
if ($destination -ne $expectedDefault -and (Split-Path -Leaf $destination) -ne 'Karma') {
    throw '拒绝清理未明确标识为 Karma 的安装目录。'
}

$processNames = @('karma-ui', 'karma-agent-windows', 'KarmaControl')
foreach ($name in $processNames) {
    foreach ($process in @(Get-Process -Name $name -ErrorAction SilentlyContinue)) {
        $processPath = $null
        try {
            $processPath = [IO.Path]::GetFullPath($process.Path)
        } catch {
            # 路径读取失败（例如进程属于其他会话）时不做路径校验，直接按名称结束：
            # 这三个进程名是 Karma 专用的，误伤风险可以忽略。
        }
        if ($processPath -and -not $processPath.StartsWith($destination + '\', [StringComparison]::OrdinalIgnoreCase)) {
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
    throw 'Karma 相关进程未能全部退出，安装目录仍被占用。'
}

if (-not (Test-Path -LiteralPath $destination)) {
    exit 0
}

$lastRemoveError = $null
for ($attempt = 1; $attempt -le 3; $attempt++) {
    try {
        Remove-Item -LiteralPath $destination -Recurse -Force -ErrorAction Stop
        exit 0
    } catch {
        $lastRemoveError = $_
        Start-Sleep -Seconds 2
    }
}
throw "无法清理旧的安装目录 $destination：$($lastRemoveError.Exception.Message)"
