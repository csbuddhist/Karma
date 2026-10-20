[CmdletBinding()]
param(
    [string] $InstallDirectory = (Join-Path $env:ProgramFiles 'Karma'),
    [switch] $StartConsole,
    # Microsoft vc_redist.x64.exe; the single-file installer passes its embedded
    # copy. Defaults to a copy placed next to this script.
    [string] $VCRedistPath = (Join-Path $PSScriptRoot 'vc_redist.x64.exe')
)

$ErrorActionPreference = 'Stop'
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw '请以管理员身份运行 PowerShell，再执行 Install-Karma.ps1。'
}

& (Join-Path $PSScriptRoot 'Verify-KarmaTestBundle.ps1')
if (-not $?) { throw '测试包完整性校验失败。' }

# 服务和 Agent 依赖 VC++ 运行库，缺失时会因找不到 DLL 而无法启动；必须在注册服务前满足。
& (Join-Path $PSScriptRoot 'Install-VCRuntime.ps1') -RedistPath $VCRedistPath
if (-not $?) { throw 'Microsoft Visual C++ 运行库检查失败。' }

function Protect-KarmaServiceRegistryKey {
    # 收紧服务注册表键（及其子键，含存放服务 SDDL 的 Security 子键）的 ACL：
    # 仅 SYSTEM 保留完全控制，管理员降为只读。这样即便有人打开 regedit，也无法把
    # Start 值改成 4（禁用启动），也无法在 Security 子键里改回服务停止权限。
    # 采用“仅 Allow、不加 Deny”的方式：因为 LocalSystem 令牌本身属于 Administrators，
    # 若对 Administrators 加 Deny 会连 SYSTEM 一并拒绝，破坏 SCM 读写。改权限只需
    # 管理员此刻（安装期）拥有的 WRITE_DAC；收紧后管理员失去写权限，regedit 无法还原。
    param([Parameter(Mandatory)] [string] $ServiceName)

    $rights = [System.Security.AccessControl.RegistryRights]'ReadPermissions, ChangePermissions, ReadKey'
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey(
        "SYSTEM\CurrentControlSet\Services\$ServiceName",
        [Microsoft.Win32.RegistryKeyPermissionCheck]::ReadWriteSubTree,
        $rights)
    if ($null -eq $key) { throw "找不到服务注册表键：$ServiceName" }
    try {
        $acl = $key.GetAccessControl([System.Security.AccessControl.AccessControlSections]::Access)
        # 断开继承且不保留继承来的 ACE，再清空现有显式 ACE，最后写入干净的三条规则。
        $acl.SetAccessRuleProtection($true, $false)
        foreach ($rule in @($acl.GetAccessRules($true, $false, [System.Security.Principal.SecurityIdentifier]))) {
            [void] $acl.RemoveAccessRule($rule)
        }
        $system = New-Object System.Security.Principal.SecurityIdentifier('S-1-5-18')      # NT AUTHORITY\SYSTEM
        $admins = New-Object System.Security.Principal.SecurityIdentifier('S-1-5-32-544')  # BUILTIN\Administrators
        $users  = New-Object System.Security.Principal.SecurityIdentifier('S-1-5-32-545')  # BUILTIN\Users
        $inherit = [System.Security.AccessControl.InheritanceFlags]::ContainerInherit
        $prop    = [System.Security.AccessControl.PropagationFlags]::None
        $allow   = [System.Security.AccessControl.AccessControlType]::Allow
        $full    = [System.Security.AccessControl.RegistryRights]::FullControl
        $read    = [System.Security.AccessControl.RegistryRights]::ReadKey
        $acl.AddAccessRule((New-Object System.Security.AccessControl.RegistryAccessRule($system, $full, $inherit, $prop, $allow)))
        $acl.AddAccessRule((New-Object System.Security.AccessControl.RegistryAccessRule($admins, $read, $inherit, $prop, $allow)))
        $acl.AddAccessRule((New-Object System.Security.AccessControl.RegistryAccessRule($users,  $read, $inherit, $prop, $allow)))
        $key.SetAccessControl($acl)
    } finally {
        $key.Close()
    }
}

$source = [IO.Path]::GetFullPath($PSScriptRoot)
$destination = [IO.Path]::GetFullPath($InstallDirectory)
$existing = Get-Service -Name 'KarmaService' -ErrorAction SilentlyContinue
$existingGuardian = Get-Service -Name 'KarmaGuardian' -ErrorAction SilentlyContinue
if ($null -ne $existing -or $null -ne $existingGuardian) {
    throw 'KarmaService 或 KarmaGuardian 已安装。请先使用受密码保护的 Uninstall-Karma.ps1 卸载当前版本。'
}
if ($source -ne $destination) {
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    Copy-Item -Path (Join-Path $source '*') -Destination $destination -Recurse -Force
}

$serviceExe = Join-Path $destination 'KarmaService.exe'
if (-not (Test-Path -LiteralPath $serviceExe -PathType Leaf)) {
    throw "缺少服务程序：$serviceExe"
}
$guardianExe = Join-Path $destination 'KarmaGuardian.exe'
if (-not (Test-Path -LiteralPath $guardianExe -PathType Leaf)) {
    throw "缺少守护服务程序：$guardianExe"
}

& sc.exe create KarmaService 'binPath=' "`"$serviceExe`"" 'start=' 'delayed-auto' 'DisplayName=' 'Karma Family Protection' | Out-Null
if ($LASTEXITCODE -ne 0) { throw '创建 KarmaService 失败。' }
& sc.exe description KarmaService 'Karma 本地家庭内容保护与策略服务' | Out-Null
if ($LASTEXITCODE -ne 0) { throw '设置 KarmaService 描述失败。' }
# reset=60：服务正常运行满 60 秒后失败计数清零，因此“结束任务”强杀被反复触发时，
# SCM 仍会每次在 1 秒内重启，而不是用尽三次预算后放弃。看护服务是更长时效的兜底。
& sc.exe failure KarmaService 'reset=' '60' 'actions=' 'restart/1000/restart/1000/restart/1000' | Out-Null
if ($LASTEXITCODE -ne 0) { throw '设置 KarmaService 恢复操作失败。' }
& sc.exe failureflag KarmaService 1 | Out-Null
if ($LASTEXITCODE -ne 0) { throw '启用 KarmaService 非崩溃恢复失败。' }

# KarmaGuardian 与 KarmaService 互相守护：任一被手动停止时，另一方在轮询周期内重新拉起。
# SCM 的 failure 恢复动作只在异常退出时触发，无法覆盖 Services.msc / sc stop 的正常停止，
# 守护服务正是为填补这一缺口而存在。使用自动启动，使其在开机后尽早接管看护。
& sc.exe create KarmaGuardian 'binPath=' "`"$guardianExe`"" 'start=' 'auto' 'DisplayName=' 'Karma Guardian' | Out-Null
if ($LASTEXITCODE -ne 0) { throw '创建 KarmaGuardian 失败。' }
& sc.exe description KarmaGuardian 'Karma 本地保护守护服务' | Out-Null
if ($LASTEXITCODE -ne 0) { throw '设置 KarmaGuardian 描述失败。' }
& sc.exe failure KarmaGuardian 'reset=' '60' 'actions=' 'restart/1000/restart/1000/restart/1000' | Out-Null
if ($LASTEXITCODE -ne 0) { throw '设置 KarmaGuardian 恢复操作失败。' }
& sc.exe failureflag KarmaGuardian 1 | Out-Null
if ($LASTEXITCODE -ne 0) { throw '启用 KarmaGuardian 非崩溃恢复失败。' }

# 收紧两个服务的 ACL：从管理员(BA)的权限中移除“停止(WP)/暂停(DT)/更改配置(DC)”，
# 使 services.msc、任务管理器“服务”页的“停止”以及属性里的“启动类型=禁用”对管理员
# 也返回“拒绝访问”。管理员仍保留查看、启动、删除(SD)、改 DACL(WD) —— 删除与改权限
# 都只能经终端 sc delete/sdset，GUI 无入口，符合“非专业管理员仅会用 GUI”的威胁模型。
# SYSTEM(SY) 保留全部权限：互相看护、SCM 恢复和密码授权卸载都在 SYSTEM 上下文运作。
$hardenedSddl = 'D:(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;SY)(A;;CCLCSWRPLOSDRCWDWO;;;BA)(A;;CCLCSWLORC;;;AU)'
foreach ($svc in @('KarmaService', 'KarmaGuardian')) {
    & sc.exe sdset $svc $hardenedSddl | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "收紧 $svc 服务权限失败。" }
}

# 第二层：收紧服务注册表键 ACL，封堵经 regedit 改 Start=4（禁用）或改 Security 子键的途径。
# 此处收紧会移除管理员对该键的写权限，因此必须在上面的 sc sdset 之后执行（此后无法再用
# regedit/注册表方式还原服务 SDDL）。卸载经 SCM(SYSTEM) 删除整个键，无需另行恢复。
foreach ($svc in @('KarmaService', 'KarmaGuardian')) {
    Protect-KarmaServiceRegistryKey -ServiceName $svc
}

Start-Service -Name 'KarmaService'
(Get-Service -Name 'KarmaService').WaitForStatus('Running', [TimeSpan]::FromSeconds(15))
Start-Service -Name 'KarmaGuardian'
(Get-Service -Name 'KarmaGuardian').WaitForStatus('Running', [TimeSpan]::FromSeconds(15))

Write-Host "KarmaService 与 KarmaGuardian 已安装并运行：$destination" -ForegroundColor Green
if ($StartConsole) {
    Start-Process -FilePath (Join-Path $destination 'karma-ui.exe')
}
