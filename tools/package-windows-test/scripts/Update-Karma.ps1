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

$deadline = (Get-Date).AddSeconds(120)
do {
    Start-Sleep -Seconds 2
    try {
        $runningVersion = & $control version
        $service = Get-Service -Name 'KarmaService'
        $guardian = Get-Service -Name 'KarmaGuardian'
        if ($LASTEXITCODE -eq 0 -and $runningVersion -eq $ExpectedVersion -and
            $service.Status -eq 'Running' -and $guardian.Status -eq 'Running') {
            Write-Host "Updated / 已升级：$ExpectedVersion" -ForegroundColor Green
            exit 0
        }
    } catch {
        # Replacement/restart may briefly make the executable unavailable.
        # 文件替换、重启期间可能短暂无法读取版本，继续等待。
    }
} while ((Get-Date) -lt $deadline)
throw 'Update not confirmed; inspect the console update status. Protection remains registered / 升级尚未确认，请查看控制台状态，保护服务仍保持注册。'
