[CmdletBinding()]
param(
    [string] $InstallDirectory = (Join-Path $env:ProgramFiles 'Karma'),
    [switch] $StartConsole
)

$ErrorActionPreference = 'Stop'
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw '请以管理员身份运行 PowerShell，再执行 Install-Karma.ps1。'
}

& (Join-Path $PSScriptRoot 'Verify-KarmaTestBundle.ps1')
if ($LASTEXITCODE -ne 0) { throw '测试包完整性校验失败。' }

$source = [IO.Path]::GetFullPath($PSScriptRoot)
$destination = [IO.Path]::GetFullPath($InstallDirectory)
$existing = Get-Service -Name 'KarmaService' -ErrorAction SilentlyContinue
if ($null -ne $existing) {
    throw 'KarmaService 已安装。请先使用受密码保护的 Uninstall-Karma.ps1 卸载当前版本。'
}
if ($source -ne $destination) {
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    Copy-Item -Path (Join-Path $source '*') -Destination $destination -Recurse -Force
}

$serviceExe = Join-Path $destination 'KarmaService.exe'
if (-not (Test-Path -LiteralPath $serviceExe -PathType Leaf)) {
    throw "缺少服务程序：$serviceExe"
}

& sc.exe create KarmaService "binPath= `"$serviceExe`"" 'start= delayed-auto' 'DisplayName= Karma Family Protection' | Out-Null
if ($LASTEXITCODE -ne 0) { throw '创建 KarmaService 失败。' }
& sc.exe description KarmaService 'Karma 本地家庭内容保护与策略服务' | Out-Null
& sc.exe failure KarmaService 'reset= 0' 'actions= restart/1000/restart/3000/restart/10000' | Out-Null
& sc.exe failureflag KarmaService 1 | Out-Null
Start-Service -Name 'KarmaService'
(Get-Service -Name 'KarmaService').WaitForStatus('Running', [TimeSpan]::FromSeconds(15))

Write-Host "KarmaService 已安装并运行：$destination" -ForegroundColor Green
if ($StartConsole) {
    Start-Process -FilePath (Join-Path $destination 'karma-ui.exe')
}
