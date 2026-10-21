[CmdletBinding()]
param(
    # Official Microsoft vc_redist.x64.exe. Optional: without it, the script
    # only checks that a sufficient runtime is already installed.
    [string] $RedistPath
)

# Karma's binaries link the Microsoft Visual C++ runtime dynamically
# (VCRUNTIME140*.dll, MSVCP140*.dll) and were built with the 14.44 toolset,
# so the installed runtime must be at least that version.
$ErrorActionPreference = 'Stop'
$minimumVersion = [version]'14.44.0.0'
$requiredDlls = @('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll', 'msvcp140_1.dll')
$downloadUrl = 'https://aka.ms/vs/17/release/vc_redist.x64.exe'

function Get-NativeSystemDirectory {
    # NSIS runs 32-bit PowerShell, where System32 is redirected to SysWOW64.
    if ([Environment]::Is64BitProcess) { return [Environment]::SystemDirectory }
    return (Join-Path $env:windir 'Sysnative')
}

function Get-InstalledVCRuntimeVersion {
    try {
        $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
            [Microsoft.Win32.RegistryHive]::LocalMachine,
            [Microsoft.Win32.RegistryView]::Registry64)
    } catch {
        return $null
    }
    try {
        $key = $base.OpenSubKey('SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64')
        if ($null -eq $key) { return $null }
        try {
            if ($key.GetValue('Installed') -ne 1) { return $null }
            return [version]::new(
                [int]$key.GetValue('Major'), [int]$key.GetValue('Minor'),
                [int]$key.GetValue('Bld'), [int]$key.GetValue('Rbld'))
        } finally {
            $key.Close()
        }
    } catch {
        return $null
    } finally {
        $base.Close()
    }
}

function Test-VCRuntime {
    $version = Get-InstalledVCRuntimeVersion
    if ($null -eq $version -or $version -lt $minimumVersion) { return $false }
    $systemDirectory = Get-NativeSystemDirectory
    foreach ($dll in $requiredDlls) {
        if (-not (Test-Path -LiteralPath (Join-Path $systemDirectory $dll) -PathType Leaf)) { return $false }
    }
    return $true
}

if (Test-VCRuntime) {
    Write-Host "Microsoft Visual C++ x64 运行库已满足要求 / runtime OK: $(Get-InstalledVCRuntimeVersion)"
    exit 0
}

if (-not $RedistPath -or -not (Test-Path -LiteralPath $RedistPath -PathType Leaf)) {
    throw "缺少 Microsoft Visual C++ 2015–2022 x64 运行库（需要 $minimumVersion 或更新版本）。请从 $downloadUrl 下载并安装后重试，或把 vc_redist.x64.exe 放在 Install-Karma.ps1 同一目录。 / The Microsoft Visual C++ 2015–2022 x64 Redistributable ($minimumVersion or later) is missing. Install it from $downloadUrl, or place vc_redist.x64.exe next to Install-Karma.ps1, then retry."
}

# Never execute an installer that Windows cannot trace to Microsoft.
$signature = Get-AuthenticodeSignature -LiteralPath $RedistPath
if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or
    $null -eq $signature.SignerCertificate -or
    $signature.SignerCertificate.Subject -notmatch '(^|,\s*)CN=Microsoft Corporation(,|$)') {
    throw "vc_redist.x64.exe 的微软数字签名无效，已拒绝运行。 / vc_redist.x64.exe does not carry a valid Microsoft signature; refusing to run it. ($($signature.Status))"
}

Write-Host '正在安装 Microsoft Visual C++ x64 运行库 / Installing the Microsoft Visual C++ x64 runtime...'
$process = Start-Process -FilePath $RedistPath -ArgumentList '/install', '/quiet', '/norestart' -Wait -PassThru
# 0: installed; 1638: a same or newer version is already present;
# 3010/1641: installed, restart required or initiated.
$exitCode = $process.ExitCode
if ($exitCode -notin @(0, 1638, 3010, 1641)) {
    throw "Microsoft Visual C++ 运行库安装失败，退出码 $exitCode。 / The Microsoft Visual C++ runtime installer failed with exit code $exitCode."
}
if (-not (Test-VCRuntime)) {
    throw "Microsoft Visual C++ 运行库安装后仍不满足要求（需要 $minimumVersion）。 / The Microsoft Visual C++ runtime still does not meet the requirement ($minimumVersion) after installation."
}
if ($exitCode -in @(3010, 1641)) {
    Write-Warning '运行库已安装，Windows 建议稍后重启。 / The runtime is installed; Windows recommends a restart.'
}
Write-Host "Microsoft Visual C++ x64 运行库已安装 / runtime installed: $(Get-InstalledVCRuntimeVersion)"
exit 0
