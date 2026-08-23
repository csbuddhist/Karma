# KarmaService tamper resistance on Windows 11 / Windows 11 上的 KarmaService 防篡改

Date / 日期: 2026-08-23

Scope / 范围: Windows 11, classic Win32 service, Microsoft first-party documentation only / Windows 11、经典 Win32 服务、仅采用微软第一方资料

## Executive conclusion / 执行结论

### English

The best supported design for Karma is **not** to try to make an ordinary Windows service impossible to stop. The reliable security boundary is:

1. The monitored child uses a **Standard User** account.
2. A parent-controlled credential is the only local administrator credential.
3. `KarmaService` has an explicit least-privilege service DACL, starts automatically (not delayed), and uses SCM recovery actions.
4. Service binaries, configuration, secrets, and WFP objects are separately ACL-protected and audited.
5. An optional WFP fail-closed layer keeps the monitored user's outbound traffic blocked whenever the service's dynamic WFP session disappears.

This stops a standard user from using Services, `sc.exe`, PowerShell, WMI, or Task Manager to turn protection off. It does **not** establish a security boundary against a local administrator. Microsoft explicitly states that administrators control the device and can disable security features. An administrator has default full service access, can take ownership and rewrite DACLs, and can use debug privilege to obtain full access to ordinary processes.

Protected Process Light (PPL) would materially resist admin-originated user-mode tampering, but the third-party route is `SERVICE_LAUNCH_PROTECTED_ANTIMALWARE_LIGHT`, which is for qualifying anti-malware vendors. It requires an ELAM driver, Microsoft Virus Initiative membership, WHQL/Microsoft signing, special service signing, and a constrained update/uninstall design. It is not a general-purpose entitlement for a parental-control service. Microsoft Defender Tamper Protection protects specified Microsoft Defender settings; Microsoft documents no general API that enrolls an unrelated third-party service.

### 简体中文

Karma 的最优受支持方案不是试图让普通 Windows 服务“绝对无法停止”，而是建立以下可靠安全边界：

1. 被监管的孩子只使用 **标准用户**账户。
2. 唯一的本机管理员凭据由家长独占。
3. 为 `KarmaService` 设置明确的最小权限服务 DACL，使用普通自动启动（非延迟启动），并配置 SCM 恢复动作。
4. 分别保护并审计服务二进制、配置、密钥以及 WFP 对象的 ACL。
5. 可选增加 WFP fail-closed 层：服务的动态 WFP 会话一旦消失，被监管用户的出站网络立即恢复为默认阻断。

这样可以阻止标准用户通过“服务”、`sc.exe`、PowerShell、WMI 或任务管理器关闭保护，但**不能**形成对抗本机管理员的安全边界。微软明确说明管理员控制设备，可以禁用安全功能；管理员默认拥有服务完全访问权，也能取得对象所有权、改写 DACL，并能通过调试权限取得普通进程的完全访问权。

Protected Process Light（PPL）可以显著抵抗来自管理员态用户进程的篡改，但第三方可用的路径是面向合格反恶意软件厂商的 `SERVICE_LAUNCH_PROTECTED_ANTIMALWARE_LIGHT`。它要求 ELAM 驱动、Microsoft Virus Initiative 会员资格、WHQL/微软签名、特殊服务签名，以及受约束的升级和卸载流程；它不是家长控制服务可直接使用的通用能力。Microsoft Defender Tamper Protection 保护微软明确列出的 Defender 设置；微软没有公开把无关第三方服务纳入该保护的通用 API。

## Threat boundary / 威胁边界

### English

| Actor | Supported protection level |
| --- | --- |
| Standard local user | Strong. SCM/service-object DACLs and process DACLs are real Windows access-control boundaries. |
| Unelevated member of Administrators | UAC normally prevents silent administrative operations, but elevation gives the user admin authority. Do not treat UAC consent as the product's parental authorization boundary. |
| Elevated local/domain administrator | No strong boundary for an ordinary service. The administrator can stop/disable/delete it, change its DACL or binary, terminate it, or remove WFP policy. |
| Kernel administrator/offline disk owner | Out of scope even for PPL. Microsoft treats administrator-to-kernel as a non-boundary. |

Microsoft's service defaults already withhold `SERVICE_STOP` from local authenticated users, while Administrators receive `SERVICE_ALL_ACCESS`, `DELETE`, `WRITE_DAC`, and `WRITE_OWNER`. This means that if a child can currently stop `KarmaService`, first determine whether the account is a local administrator or whether installation changed the DACL. Do not solve an account-model defect solely with a watchdog.

### 简体中文

| 行为主体 | Windows 能提供的保护级别 |
| --- | --- |
| 本机标准用户 | 强。SCM/服务对象 DACL 和进程 DACL 是真实的 Windows 访问控制边界。 |
| Administrators 组中但尚未提升的用户 | UAC 通常阻止静默执行管理员操作，但用户一旦批准提升就拥有管理员权限。不要把 UAC 同意框当成 Karma 的家长授权边界。 |
| 已提升的本机/域管理员 | 普通服务不存在强边界。管理员能停止、禁用、删除服务，修改 DACL 或二进制，终止进程，或者删除 WFP 策略。 |
| 内核管理员或可离线控制磁盘者 | 即使 PPL 也不覆盖这一威胁。微软把 Administrator-to-kernel 明确列为非安全边界。 |

微软的默认服务安全描述符本来就不向本机已认证普通用户授予 `SERVICE_STOP`，但向 Administrators 授予 `SERVICE_ALL_ACCESS`、`DELETE`、`WRITE_DAC` 和 `WRITE_OWNER`。因此，如果孩子目前能够停止 `KarmaService`，应先确认其账户是否为本机管理员，或安装流程是否改变了 DACL；不能只靠 watchdog 掩盖账户模型缺陷。

Sources / 来源: [Service Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/services/service-security-and-access-rights), [Local accounts](https://learn.microsoft.com/en-us/windows/security/identity-protection/access-control/local-accounts), [Microsoft Security Servicing Criteria for Windows](https://www.microsoft.com/en-us/msrc/windows-security-servicing-criteria), [Process Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/procthread/process-security-and-access-rights), [Debug programs](https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-10/security/threat-protection/security-policy-settings/debug-programs).

## 1. SCM security and the service DACL / SCM 安全与服务 DACL

### English

`SERVICE_STOP` (`0x0020`) is required to send `SERVICE_CONTROL_STOP`. `SERVICE_CHANGE_CONFIG` can change the executable that SCM launches; `DELETE` removes the service; `WRITE_DAC` changes the DACL; and `WRITE_OWNER` changes the owner. A robust policy must therefore protect more than just `SERVICE_STOP`.

Recommended access policy:

- Standard users: retain only the query/interrogate rights needed by health UI. Do not grant `SERVICE_STOP`, `SERVICE_PAUSE_CONTINUE`, `SERVICE_CHANGE_CONFIG`, `DELETE`, `WRITE_DAC`, or `WRITE_OWNER`.
- `LOCAL SYSTEM`: grant the operational rights needed by the service and installer design.
- Parent/maintenance identity: choose deliberately. Giving local Administrators full access is operationally simple but means any elevated administrator can stop the service. Removing stop/config/delete rights from Administrators reduces accidental stopping, but is only tamper resistance, not an admin security boundary.
- Prefer an allow-list DACL over broad deny ACEs. A deny ACE aimed at a group can also match a more privileged token that contains that group SID.

The installer should read the existing descriptor with `QueryServiceObjectSecurity`, construct and validate the intended ACL, and write it with `SetServiceObjectSecurity` (or the recommended general security-info API). Microsoft says DACL changes persist until the service is removed. `sc.exe sdshow`/`sdset` are useful for inspection and controlled testing, but shipping a copied SDDL string without resolving the actual maintenance identities can lock out the supported updater.

Useful diagnostics, not a universal production SDDL:

```powershell
sc.exe sdshow KarmaService
sc.exe stop KarmaService
```

Run the stop test once as the monitored standard user (expected: access denied), and separately under the supported maintenance identity (expected result depends on the chosen maintenance design).

Whether the service advertises `SERVICE_ACCEPT_STOP` is a separate gate. If it does not advertise it, `ControlService` rejects the stop control even for a caller with `SERVICE_STOP`. Karma already has a password-authenticated IPC shutdown request, so protected mode should **not** advertise `SERVICE_ACCEPT_STOP`; it should accept only system-originated shutdown/preshutdown notifications plus its authenticated maintenance IPC. This is defense in depth alongside the DACL, not a boundary against an administrator, who can still terminate or reconfigure an ordinary process after regaining the necessary rights.

### 简体中文

发送 `SERVICE_CONTROL_STOP` 必须具有 `SERVICE_STOP`（`0x0020`）。此外，`SERVICE_CHANGE_CONFIG` 可以改变 SCM 启动的可执行文件，`DELETE` 可以删除服务，`WRITE_DAC` 可以修改 DACL，`WRITE_OWNER` 可以修改所有者。因此，可靠策略不能只移除 `SERVICE_STOP`。

推荐的访问策略：

- 标准用户：只保留健康状态 UI 所需的查询/询问权限；不授予 `SERVICE_STOP`、`SERVICE_PAUSE_CONTINUE`、`SERVICE_CHANGE_CONFIG`、`DELETE`、`WRITE_DAC` 或 `WRITE_OWNER`。
- `LOCAL SYSTEM`：只授予服务和安装器设计实际需要的操作权限。
- 家长/维护身份：必须明确选择。让本机 Administrators 保持完全访问最容易维护，但任何已提升管理员都能停止服务；从 Administrators 移除停止、配置、删除权限可以降低误操作，却只是防篡改加固，不是管理员安全边界。
- 优先采用最小化 allow-list DACL，不要随意对大组添加拒绝 ACE；高权限令牌中也可能包含被拒绝组的 SID，从而被一并拒绝。

安装器应通过 `QueryServiceObjectSecurity` 读取现有描述符，构建并校验目标 ACL，再通过 `SetServiceObjectSecurity`（或微软推荐的通用安全信息 API）写回。微软说明，服务 DACL 的修改会持续到服务被删除。`sc.exe sdshow`/`sdset` 适合检查和受控测试，但不应直接发布一段未解析实际维护身份的固定 SDDL，否则可能把官方更新器也锁死。

以下命令用于诊断，不代表通用生产 SDDL：

```powershell
sc.exe sdshow KarmaService
sc.exe stop KarmaService
```

应分别以被监管标准用户执行停止测试（预期：拒绝访问），再以受支持的维护身份执行（预期结果取决于维护方案）。

服务是否声明 `SERVICE_ACCEPT_STOP` 是另一道门。如果不声明，即使调用者有 `SERVICE_STOP`，`ControlService` 也会拒绝停止控制。Karma 已有密码认证的 IPC 关停请求，因此保护模式下应**不声明** `SERVICE_ACCEPT_STOP`，只接受系统发出的 shutdown/preshutdown 通知和已认证的维护 IPC。这是 DACL 之外的纵深防御，并非对管理员的安全边界；管理员仍可在重新获得所需权限后终止或重配置普通进程。

Sources / 来源: [Service Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/services/service-security-and-access-rights), [QueryServiceObjectSecurity](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-queryserviceobjectsecurity), [SetServiceObjectSecurity](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-setserviceobjectsecurity), [Modifying the DACL for a Service](https://learn.microsoft.com/en-us/windows/win32/services/modifying-the-dacl-for-a-service), [Configuring a Service Using SC](https://learn.microsoft.com/en-us/windows/win32/services/configuring-a-service-using-sc), [sc sdset](https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-server-2012-r2-and-2012/cc742037%28v%3Dws.11%29), [SERVICE_STATUS](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_status), [ControlService](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-controlservice).

## 2. Automatic start, recovery, and watchdogs / 自动启动、恢复动作与 watchdog

### English

Use `SERVICE_AUTO_START`, not delayed auto-start, for a protection service. SCM starts auto services during boot. Delayed auto-start provides no exact start-time guarantee, starts services one by one after other auto services, and calls made before the service loads can fail. This repository currently installs `KarmaService` as `delayed-auto`; that creates an avoidable protection gap.

Configure `SERVICE_CONFIG_FAILURE_ACTIONS` with `SC_ACTION_RESTART` and bounded, increasing delays. SCM considers a service failed when the process terminates without reporting `SERVICE_STOPPED`. If `SERVICE_CONFIG_FAILURE_ACTIONS_FLAG.fFailureActionsOnNonCrashFailures` is true, recovery also runs when the service reports `SERVICE_STOPPED` with a nonzero `dwWin32ExitCode`. A clean manual stop that reports `SERVICE_STOPPED` and `ERROR_SUCCESS` is **not** a failure and does not trigger restart. A queued restart cannot be cancelled merely by starting and stopping the service; Microsoft says the service must be explicitly disabled to prevent that queued restart.

Implications for Karma:

- Keep the SCM recovery policy as availability defense for crashes and forced termination.
- On an unrecoverable runtime failure, report `SERVICE_STOPPED` with a nonzero Win32/service-specific exit code. The current `run_service` path always reports Win32 exit code `0` after `serve` returns, so `failureflag=1` cannot recover this graceful error path.
- Recovery is not anti-tamper: an authorized user can perform a clean stop, disable the service, remove the recovery actions, or delete the service.
- A second user-mode watchdog has the same limitation. It can detect an accidental outage, but an administrator can disable/kill both processes or their startup registrations. Circular watchdogs increase update races and crash loops without creating a new trust boundary. Prefer SCM recovery plus an independent health signal/alert.

A reasonable non-rebooting recovery sequence is restart after 5 seconds, 30 seconds, then 5 minutes, repeating the last action, with a documented reset period. Do not reboot a family PC as a routine third-failure action.

### 简体中文

保护服务应使用 `SERVICE_AUTO_START`，而不是延迟自动启动。SCM 会在引导期间启动普通自动服务；延迟自动启动没有精确时间保证，会在其他自动服务之后逐个启动，而且客户端在服务尚未加载时调用会失败。当前仓库把 `KarmaService` 安装为 `delayed-auto`，会产生不必要的保护空窗。

应通过 `SERVICE_CONFIG_FAILURE_ACTIONS` 配置带有有限递增延迟的 `SC_ACTION_RESTART`。服务进程未报告 `SERVICE_STOPPED` 就退出时，SCM 认为它失败；若 `SERVICE_CONFIG_FAILURE_ACTIONS_FLAG.fFailureActionsOnNonCrashFailures` 为 true，当服务报告 `SERVICE_STOPPED` 且 `dwWin32ExitCode` 非零时也会执行恢复动作。正常人工停止若报告 `SERVICE_STOPPED` 和 `ERROR_SUCCESS`，则**不是**失败，不会自动重启。已排队的重启动作不能通过先启动再停止来取消；微软说明必须把服务明确设为 Disabled 才能阻止它。

对 Karma 的含义：

- 保留 SCM 恢复策略，用于抵御崩溃和强制终止导致的可用性中断。
- 遇到不可恢复的运行时错误时，应以非零 Win32/服务专用退出码报告 `SERVICE_STOPPED`。当前 `run_service` 在 `serve` 返回后总是报告 Win32 退出码 `0`，因此 `failureflag=1` 无法恢复这条“正常退出但实际失败”的路径。
- 恢复动作不是防篡改：获授权者仍可正常停止、禁用服务、删除恢复动作或删除服务。
- 第二个用户态 watchdog 也有同样限制。它能发现意外中断，但管理员可以禁用/杀死两个进程或其启动项。互相看护会增加升级竞态与崩溃循环，却不会产生新的信任边界。应优先使用 SCM 恢复加独立健康信号/告警。

一个适合家庭电脑、不会主动重启整机的恢复序列可以是：5 秒、30 秒、5 分钟后重启服务，之后重复最后一个动作，并设置明确的失败计数重置周期。不要把第三次失败后重启整台家庭电脑作为常规策略。

Sources / 来源: [Automatically Starting Services](https://learn.microsoft.com/en-us/windows/win32/services/automatically-starting-services), [SERVICE_DELAYED_AUTO_START_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_delayed_auto_start_info), [sc.exe config](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/sc-config), [SERVICE_FAILURE_ACTIONS](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_failure_actionsw), [SERVICE_FAILURE_ACTIONS_FLAG](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_failure_actions_flag), [SC_ACTION](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-sc_action), [ChangeServiceConfig2](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-changeserviceconfig2w), [Guidelines for Services](https://learn.microsoft.com/en-us/windows/win32/rstmgr/guidelines-for-services).

## 3. Service SID and least privilege / 服务 SID 与最小权限

### English

A service SID is not an anti-stop feature. `SERVICE_SID_TYPE_UNRESTRICTED` adds `NT SERVICE\KarmaService` to the process token so files, registry keys, named pipes, and other securable resources can grant rights to this service rather than to all of LocalSystem. `SERVICE_SID_TYPE_RESTRICTED` additionally places the service SID and other SIDs in the token's restricted SID list, reducing where a compromised service can write.

For an own-process service, evaluate `SERVICE_SID_TYPE_RESTRICTED` and ACL all Karma-owned resources to `NT SERVICE\KarmaService`. Also declare only required token privileges through `SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO`. Test process-control, capture, DPAPI, model loading, IPC, updating, and uninstall on real Windows hardware before enabling restricted mode. LocalSystem has extensive local privileges, so retaining it without privilege trimming enlarges the impact of a service compromise.

### 简体中文

服务 SID 不是“禁止停止服务”的功能。`SERVICE_SID_TYPE_UNRESTRICTED` 会把 `NT SERVICE\KarmaService` 加入服务进程令牌，使文件、注册表项、命名管道等安全对象可以只授权给该服务，而不是笼统授权给整个 LocalSystem。`SERVICE_SID_TYPE_RESTRICTED` 还会把服务 SID 及其他 SID 加入令牌的受限 SID 列表，从而限制服务被攻陷后可写入的位置。

对于独立进程服务，应评估 `SERVICE_SID_TYPE_RESTRICTED`，并把 Karma 自有资源 ACL 授予 `NT SERVICE\KarmaService`；同时通过 `SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO` 只声明需要的令牌权限。启用 restricted 模式前，必须在真实 Windows 硬件上测试进程控制、捕获、DPAPI、模型加载、IPC、升级和卸载。LocalSystem 在本机拥有广泛权限，不做权限裁剪会放大服务被攻陷后的影响。

Sources / 来源: [SERVICE_SID_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_sid_info), [Service Changes for Windows Vista](https://learn.microsoft.com/en-us/windows/win32/services/service-changes-for-windows-vista), [LocalSystem Account](https://learn.microsoft.com/en-us/windows/win32/services/localsystem-account).

## 4. WFP fail-closed outbound control / WFP 出站 fail-closed 控制

### English

The proposed design is feasible and is the strongest useful defense-in-depth addition for the case “the service disappears, so enforcement must not silently disappear”:

1. At install time, create a Karma WFP provider and sublayer, then add **persistent** outbound catch-all block filters scoped to the monitored user's SID at the relevant ALE authorization layers for IPv4 and IPv6.
2. Do not associate the persistent provider with the `KarmaService` Windows service name. Microsoft says BFE activates service-associated persistent objects only when that service is configured for auto-start. An attacker who changes the service to Disabled could otherwise turn the intended fail-closed rule into fail-open at the next BFE start.
3. While healthy, `KarmaService` opens a WFP **dynamic session** and adds higher-weight permit filters in the **same Karma sublayer**, with the same user scope. When the engine handle closes or the service process dies, BFE automatically deletes every object created in that dynamic session, exposing the lower-weight persistent block.
4. Add/delete related objects in WFP transactions, use stable provider/sublayer/filter GUIDs, and validate both IPv4 and IPv6 coverage.

The same-sublayer requirement matters. Within a sublayer, WFP evaluates matching filters from highest to lowest weight and stops at the first Permit or Block; therefore the dynamic permit can precede the persistent block. Across sublayers, the final result follows arbitration rules where Block normally overrides Permit, and a default filter Block is a hard block. Merely putting “a higher-priority permit somewhere” is not sufficient. Other security providers can still block traffic, which is desirable: Karma's permit should not bypass Windows Firewall or another security product. There is no cross-layer arbitration, so every layer used by the policy must be reasoned about separately.

Important limits:

- This makes **network access** fail closed; it does not restart screen detection or prevent offline activity.
- Persistent filters are loaded when BFE starts. If protection is required before BFE initialization, a separate boot-time filter can bridge the boot interval; Microsoft forbids combining boot-time and persistent flags on one filter. For a per-user ALE rule, validate that the user's identity is available at the chosen layer and that logon occurs after the persistent policy is active.
- User-scoped matching is supported through `FWPM_CONDITION_ALE_USER_ID`; the condition contains a security descriptor and matches when the token has `FWP_ACTRL_MATCH_FILTER` access. Test multiple sessions, fast user switching, Microsoft accounts, local accounts, IPv6, loopback, captive portals, DNS/DHCP, VPNs, sleep/resume, BFE restart, and updates.
- WFP DACLs can prevent standard users from deleting provider/sublayer/filter objects. They cannot stop a local administrator. BFE deliberately always grants built-in Administrators the ability to open the engine; an administrator can enable `SeTakeOwnershipPrivilege`, take ownership, rewrite the DACL, and delete the filters. Kernel-mode callers bypass WFP access checks entirely.
- Recovery and maintenance need a fail-safe transaction: install the persistent block only when the service, dynamic permit path, emergency recovery procedure, and uninstall authorization are ready. Otherwise an interrupted installer can lock the monitored account out of the network.

Verdict: **adopt this as defense in depth**, with the exact same-sublayer/weight design and an unbound persistent provider. It materially converts service crash/kill into visible loss of network rather than silent loss of protection for a standard user. It still relies on the standard-user/parent-admin separation as the actual security boundary.

### 简体中文

该方案可行，而且对于“服务消失后不能静默失去管控”的目标，它是最有价值的纵深防御：

1. 安装时创建 Karma 的 WFP provider 和 sublayer，并在相关 IPv4/IPv6 ALE 授权层添加按被监管用户 SID 限定的**持久化**出站 catch-all 阻断过滤器。
2. 不要把持久化 provider 关联到 `KarmaService` 的 Windows 服务名。微软说明，BFE 只会在关联服务配置为自动启动时启用这类服务关联的持久化对象；否则攻击者把服务改成 Disabled 后，下一次 BFE 启动可能令原本的 fail-closed 规则变成 fail-open。
3. `KarmaService` 健康运行时打开 WFP **动态会话**，在**同一个 Karma sublayer** 中添加相同用户范围、权重更高的 permit 过滤器。当 engine handle 关闭或服务进程退出时，BFE 会自动删除该动态会话创建的全部对象，于是较低权重的持久化 block 重新生效。
4. 用 WFP transaction 原子化增删相关对象，使用稳定的 provider/sublayer/filter GUID，并验证 IPv4 与 IPv6 全覆盖。

“同一 sublayer”非常关键。在同一子层内，WFP 按权重从高到低匹配，在第一个 Permit 或 Block 处停止，因此动态 permit 可以排在持久化 block 之前。跨子层时，最终结果遵循通常由 Block 覆盖 Permit 的仲裁规则，而普通 filter Block 默认是 hard block；所以仅仅“在别处放一个更高优先级 permit”并不可靠。其他安全产品仍可阻断流量，这是正确行为：Karma 的 permit 不应绕过 Windows Firewall 或其他安全软件。不同过滤层之间不存在统一仲裁，因此所使用的每一层都必须单独推理和测试。

重要局限：

- 该方案让**网络访问** fail closed，并不能自动恢复屏幕检测，也不能阻止离线活动。
- 持久化过滤器在 BFE 启动时加载。如果要求覆盖 BFE 初始化前的间隙，可以另设 boot-time filter；微软禁止在同一个 filter 上同时使用 boot-time 与 persistent 标志。对按用户 ALE 规则，还需验证所选层能取得用户身份，并确保用户登录前持久化策略已生效。
- 可通过 `FWPM_CONDITION_ALE_USER_ID` 按用户匹配；该条件携带安全描述符，当令牌具有 `FWP_ACTRL_MATCH_FILTER` 时匹配。必须测试多会话、快速用户切换、微软账户、本地账户、IPv6、loopback、强制门户、DNS/DHCP、VPN、睡眠恢复、BFE 重启和升级。
- WFP DACL 可以防止标准用户删除 provider/sublayer/filter，却不能阻止本机管理员。BFE 特意保证内置 Administrators 总能打开 engine；管理员可启用 `SeTakeOwnershipPrivilege`、取得所有权、改写 DACL 并删除过滤器。内核态调用者完全跳过 WFP 访问检查。
- 安装、恢复和维护必须有 fail-safe transaction：只有在服务、动态 permit 路径、紧急恢复流程和卸载授权都准备好后，才能提交持久化 block，否则中断的安装可能使被监管账户断网。

结论：**建议作为纵深防御采用**，但必须落实“同一子层 + 正确权重 + 不绑定服务名的持久化 provider”。对于标准用户，它能把服务崩溃/被杀从“静默失去保护”转变为明显断网；真正的安全边界仍然是标准用户与家长管理员凭据分离。

Sources / 来源: [WFP Object Management](https://learn.microsoft.com/en-us/windows/win32/fwp/object-management), [WFP Best Practices](https://learn.microsoft.com/en-us/windows/win32/fwp/best-practices), [WFP Filter Arbitration](https://learn.microsoft.com/en-us/windows/win32/fwp/filter-arbitration), [WFP Access Control](https://learn.microsoft.com/en-us/windows/win32/fwp/access-control), [WFP Basic Operation](https://learn.microsoft.com/en-us/windows/win32/fwp/basic-operation), [FWPM_FILTER0](https://learn.microsoft.com/en-us/windows/win32/api/fwpmtypes/ns-fwpmtypes-fwpm_filter0), [Permitting and Blocking Applications and Users](https://learn.microsoft.com/en-us/windows/win32/fwp/permitting-and-blocking-applications-and-users), [Filtering condition identifiers](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/filtering-condition-identifiers).

## 5. PPL, ELAM, and Tamper Protection / PPL、ELAM 与 Tamper Protection

### English

Windows exposes four protected-service values, but `SERVICE_LAUNCH_PROTECTED_WINDOWS` and `_WINDOWS_LIGHT` are reserved for Windows. The third-party value is `_ANTIMALWARE_LIGHT`. Once launched protected, unprotected processes cannot call `ChangeServiceConfig(2)`, `ControlService(Ex)`, `DeleteService`, or `SetServiceObjectSecurity` against it, and cannot inject threads or write its virtual memory.

This is not a flag Karma can simply enable. Microsoft requires an installed ELAM driver whose embedded resource identifies the certificates allowed to sign the protected service. The service EXE must be page-hash signed; non-Windows DLLs loaded into it must have appropriate signatures. Production ELAM drivers require Microsoft Virus Initiative membership, WHQL verification, and Microsoft special signing. Updating signing certificates can require a new ELAM driver. For uninstall, the protected service itself must first call `ChangeServiceConfig2` to make itself unprotected, because an ordinary uninstaller cannot change it.

Therefore PPL should be a separate future product track only if Karma becomes a qualifying anti-malware vendor and accepts the driver, certification, signing, compatibility, support, and recovery obligations. It is not the optimal near-term solution.

Microsoft Defender Tamper Protection is documented as protecting selected Microsoft Defender Antivirus/Endpoint settings and is managed through Defender, Intune, Configuration Manager, or Windows Security. It explicitly does not change how non-Microsoft antivirus registers with Windows Security. **Inference from the published Microsoft interfaces:** it is not an enrollment API for protecting `KarmaService`; the documented third-party protected-service route is ELAM/PPL.

### 简体中文

Windows 定义了四种受保护服务值，但 `SERVICE_LAUNCH_PROTECTED_WINDOWS` 和 `_WINDOWS_LIGHT` 仅供 Windows 内部使用；第三方可用值是 `_ANTIMALWARE_LIGHT`。服务以受保护方式启动后，非受保护进程不能对它调用 `ChangeServiceConfig(2)`、`ControlService(Ex)`、`DeleteService` 或 `SetServiceObjectSecurity`，也不能向其注入线程或写入虚拟内存。

这并不是 Karma 可以直接打开的开关。微软要求计算机安装 ELAM 驱动，且驱动内嵌资源必须声明允许签署受保护服务的证书。服务 EXE 必须带页哈希签名，加载的非 Windows DLL 也必须有合适签名。生产 ELAM 驱动要求加入 Microsoft Virus Initiative、通过 WHQL 验证并取得微软特殊签名。更新签名证书还可能要求发布新的 ELAM 驱动。卸载时必须由受保护服务自身先调用 `ChangeServiceConfig2` 把自己改回非保护状态，因为普通卸载器无权修改它。

因此，只有当 Karma 未来成为合格反恶意软件厂商，并愿意承担驱动、认证、签名、兼容性、支持和恢复义务时，才应把 PPL 作为独立产品路线；它不是近期最优方案。

Microsoft Defender Tamper Protection 的官方范围是选定的 Microsoft Defender Antivirus/Endpoint 设置，通过 Defender、Intune、Configuration Manager 或 Windows Security 管理；文档还明确说它不改变非微软杀毒软件向 Windows Security 注册的方式。**根据微软公开接口作出的推论：**它不是把 `KarmaService` 纳入保护的注册 API；微软公开的第三方受保护服务路径是 ELAM/PPL。

Sources / 来源: [SERVICE_LAUNCH_PROTECTED_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_launch_protected_info), [Protecting anti-malware services](https://learn.microsoft.com/en-us/windows/win32/services/protecting-anti-malware-services-), [ELAM prerequisites](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/elam-prerequisites), [ELAM driver submission](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/elam-driver-submission), [Protect security settings with tamper protection](https://learn.microsoft.com/en-us/defender-endpoint/prevent-changes-to-security-settings-with-tamper-protection).

## 6. Recommended implementation order / 推荐实施顺序

### English

1. **Fix the account and enrollment model first.** During elevated setup, verify that each monitored account is not in local Administrators, require a separate parent-controlled administrator credential, and complete initial password enrollment through an installer-only one-time capability before interactive users can connect. Refuse to advertise “strong protection” otherwise.
2. **Change delayed auto-start to auto-start.** Preserve clean system shutdown handling.
3. **Install and continuously verify an explicit service DACL.** Standard monitored users get query-only rights. Protect service binary/config/data ACLs as a separate step.
4. **Correct recovery and health semantics.** Configure restart recovery and the non-crash flag; report nonzero exit status on fatal service errors; make Agent supervision depend on fresh authenticated capture/inference heartbeats, not merely a live process handle; emit health/audit events.
5. **Add a service SID and reduce privileges.** Roll out restricted SID only after hardware acceptance passes.
6. **Implement WFP fail-closed.** Use paired IPv4/IPv6 user-scoped persistent blocks and same-sublayer dynamic permits; add recovery tooling before enabling it.
7. **Design maintenance as an explicit state machine.** Parent authentication and OS caller validation authorize a short maintenance lease. Before exiting, the service launches a signed one-shot maintenance worker as LocalSystem and passes only inherited, operation-specific handles/nonces—never an arbitrary path or command. The worker waits for clean shutdown, atomically replaces files or deletes the service, rolls back on failure, and restores the service plus dynamic permits before the lease ends. Uninstall deletes persistent WFP objects only inside this workflow. This avoids reopening `STOP`, `CHANGE_CONFIG`, or `DELETE` to all Administrators merely to make updates work.
8. **Do not pursue PPL now.** Reassess only as an ELAM/MVI anti-malware program decision.

### 简体中文

1. **先修正账户和初始注册模型。** 已提权安装时验证每个被监管账户都不属于本机 Administrators，要求独立、由家长控制的管理员凭据，并在交互用户能连接前通过仅安装器可持有的一次性 capability 完成初始密码注册；否则不得宣称“强保护”。
2. **把延迟自动启动改为普通自动启动。** 同时保留干净的系统关机处理。
3. **安装并持续校验明确的服务 DACL。** 被监管标准用户只获查询权限；服务二进制、配置和数据 ACL 另行保护。
4. **修正恢复和健康语义。** 配置重启恢复和非崩溃失败标志；致命服务错误必须报告非零退出状态；Agent 监督必须依赖新鲜、已认证的采集/推理心跳，而不只是一个存活进程句柄；产生健康与审计事件。
5. **增加服务 SID 并裁剪权限。** Restricted SID 必须通过真实硬件验收后再推广。
6. **实现 WFP fail-closed。** 使用成对的 IPv4/IPv6、按用户持久化 block 和同一子层动态 permit；启用前先完成恢复工具。
7. **把维护设计成明确状态机。** 家长认证和操作系统调用者校验签发短时维护租约。服务退出前，以 LocalSystem 启动一次性的已签名维护助手，只传递继承的、针对特定操作的句柄/nonce，绝不接受任意路径或命令。助手等待服务干净停止，原子替换文件或删除服务，失败时回滚，并在租约结束前恢复服务和动态 permit。只能在该流程中删除持久化 WFP 对象。这样不必为了升级而向所有 Administrators 重新开放 `STOP`、`CHANGE_CONFIG` 或 `DELETE`。
8. **近期不采用 PPL。** 只有在决定进入 ELAM/MVI 反恶意软件路线时重新评估。

## 7. Acceptance matrix / 验收矩阵

| Test / 测试 | Expected result / 预期结果 |
| --- | --- |
| Standard user: Services UI, `sc stop`, `Stop-Service`, WMI stop / 标准用户通过服务 UI、命令或 WMI 停止 | Access denied; protection remains active / 拒绝访问，保护保持运行 |
| Standard user attempts to change startup, recovery, DACL, executable, or delete service / 标准用户修改启动、恢复、DACL、可执行路径或删除服务 | Access denied / 拒绝访问 |
| Kill service process from a permitted test harness / 通过获授权测试工具终止服务进程 | Dynamic WFP permit disappears immediately; outbound is blocked; SCM restarts service; permit is restored only after health checks / 动态 permit 立即消失、出站阻断、SCM 重启服务，健康检查通过后才恢复 permit |
| Service returns a fatal internal error / 服务返回致命内部错误 | Nonzero stopped status triggers configured recovery / 非零停止状态触发恢复 |
| Clean authenticated maintenance stop / 经认证的正常维护停止 | No crash loop; maintenance lease and audit exist; network behavior is intentional / 不产生崩溃循环，有维护租约与审计，网络行为符合设计 |
| Change `KarmaService` to Disabled in a lab / 实验环境把服务改为 Disabled | Persistent unbound WFP block remains after BFE/reboot; document recovery / 未绑定服务名的持久化 WFP block 在 BFE/重启后仍存在，并可按文档恢复 |
| BFE restart, reboot, sleep/resume, fast user switching / BFE 重启、整机重启、睡眠恢复、快速用户切换 | No fail-open interval for a logged-on monitored user; no unintended block for parent identity / 已登录被监管用户无 fail-open 空窗，家长身份不被误阻断 |
| Authorized update and uninstall / 授权升级与卸载 | Signed package, rollback on failure, service/WFP ACLs restored, persistent WFP filters removed only on successful uninstall / 验证签名，失败可回滚，恢复服务/WFP ACL，仅在成功卸载时删除持久化过滤器 |
| Elevated administrator adversarial test / 已提升管理员对抗测试 | Documented as bypassable; audit/remote alert where possible, never claimed impossible / 明确记录为可绕过，尽可能审计/远程告警，不宣称绝对不可绕过 |

## 8. Repository-specific observations / 当前仓库观察

### English

The generated installer currently uses `start= delayed-auto`, configures restart delays of 1/3/10 seconds with reset `0`, enables `failureflag`, and does not configure a service SID or explicit service DACL. The service advertises `STOP` while running and always reports Win32 exit code `0` after `serve` returns. Its Agent watchdog checks only process exit/session changes, not stale capture or inference heartbeats. Its local pipe grants interactive users read/write access; `ClientKind` is serialized input rather than an OS-authenticated identity, and the first interactive caller can enroll the initial administrator password. The authenticated `KarmaControl shutdown` path is a useful foundation, but production setup must bind initial enrollment to the elevated installer/parent identity and maintenance IPC must validate the OS caller in addition to the password. These findings explain the priority order above; this report does not modify implementation files.

### 简体中文

当前生成安装脚本使用 `start= delayed-auto`，设置 1/3/10 秒重启且 reset 为 `0`，启用了 `failureflag`，但没有配置服务 SID 或明确的服务 DACL。服务运行时声明接受 `STOP`，且 `serve` 返回后总是上报 Win32 退出码 `0`。Agent watchdog 只检查进程退出/会话变更，不检查采集或推理心跳是否过期。本机管道允许交互用户读写；`ClientKind` 是序列化输入，不是经操作系统认证的身份，而第一个交互调用者可以注册初始管理员密码。已认证的 `KarmaControl shutdown` 路径是良好基础，但生产安装必须把初始注册绑定到已提权安装器/家长身份，维护 IPC 也必须在密码之外校验操作系统调用者。这些现状解释了上述实施优先级；本报告未修改实现文件。

Local evidence / 本地证据: `release/windows-x64-test/Install-Karma.ps1`, `release/windows-x64-test/Uninstall-Karma.ps1`, `apps/karma-service-windows/src/windows_service.rs`.

## Primary Microsoft sources / 微软一手资料索引

- [Service Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/services/service-security-and-access-rights)
- [QueryServiceObjectSecurity](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-queryserviceobjectsecurity)
- [SetServiceObjectSecurity](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-setserviceobjectsecurity)
- [Modifying the DACL for a Service](https://learn.microsoft.com/en-us/windows/win32/services/modifying-the-dacl-for-a-service)
- [SERVICE_FAILURE_ACTIONS](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_failure_actionsw)
- [SERVICE_FAILURE_ACTIONS_FLAG](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_failure_actions_flag)
- [ChangeServiceConfig2](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-changeserviceconfig2w)
- [SERVICE_SID_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_sid_info)
- [Protecting anti-malware services](https://learn.microsoft.com/en-us/windows/win32/services/protecting-anti-malware-services-)
- [ELAM prerequisites](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/elam-prerequisites)
- [Microsoft Defender Tamper Protection](https://learn.microsoft.com/en-us/defender-endpoint/prevent-changes-to-security-settings-with-tamper-protection)
- [WFP Object Management](https://learn.microsoft.com/en-us/windows/win32/fwp/object-management)
- [WFP Filter Arbitration](https://learn.microsoft.com/en-us/windows/win32/fwp/filter-arbitration)
- [WFP Access Control](https://learn.microsoft.com/en-us/windows/win32/fwp/access-control)
- [Microsoft Security Servicing Criteria for Windows](https://www.microsoft.com/en-us/msrc/windows-security-servicing-criteria)
