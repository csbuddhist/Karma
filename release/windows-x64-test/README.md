# Karma Windows x64 测试包

这是未签名的开发测试包，不是正式发布版。它包含 `KarmaService.exe`、`KarmaControl.exe`、管理 GUI、会话 Agent、本地 ONNX/OCR 模型及 PowerShell 安装脚本。

## 推荐测试方式

1. 安装 Microsoft Visual C++ 2015–2022 Redistributable x64。
2. 以管理员身份打开 PowerShell，进入本目录。
3. 对当前 PowerShell 执行 `Set-ExecutionPolicy -Scope Process Bypass`。
4. 执行 `.\Install-Karma.ps1 -StartConsole`。
5. GUI 首次打开时创建至少 10 个字符的管理员密码。

安装脚本把测试包复制到 `C:\Program Files\Karma`，注册自动启动的 `KarmaService`，配置 SCM 崩溃恢复并启动服务。Service 会在当前活动控制台会话启动 Agent；Agent 退出后 watchdog 会重新启动它。GUI、Agent 和 Service 通过仅限本机、支持并发客户端的版本化命名管道通信。GUI 无法连接 Service 时会明确显示连接错误，不会假定管理员密码已经设置。

卸载时以管理员身份执行：

```powershell
C:\Program Files\Karma\Uninstall-Karma.ps1
```

脚本会安全提示输入 Karma 管理员密码，通过 `KarmaControl.exe` 请求 Service 授权关闭，然后删除服务和程序。默认保留 `C:\ProgramData\Karma`；使用 `-PurgeData` 才会删除策略、审计记录、DPAPI 密钥和加密证据。

## 独立诊断方式

- `.\Start-KarmaConsole.ps1`：只启动 GUI；未安装 Service 时无法登录或保存服务策略。
- `.\Start-KarmaTest.ps1`：只以前台方式运行 Agent，不会取得 Service 注入的 Agent 密钥，因此 GUI 会保持 Agent 未连接。
- `KarmaService.exe --console`：只适合管理员诊断；正常安装必须由 SCM 启动。

## 已知限制

- 所有 EXE 和脚本均未签名，SmartScreen 会显示未知发布者。
- 已实现 Service、认证 IPC、心跳、策略、身份绑定处置执行端、DPAPI + AES-GCM 证据库和 Agent watchdog。启用“保存事件证据”后，达到立即处置阈值的缩放推理帧会自动加密保存并可在 GUI 中查看。
- 已接入前台来源窗口观察、浏览器域名白名单/黑名单以及英语、汉语、日语和俄语窗口标题匹配。策略优先级为“网址白名单 > 网址黑名单 > 标题关键词 > 图像阈值”，命中后由 Service 独立复核并关闭身份绑定的来源进程。
- 浏览器域名发现依赖 UI Automation 读取地址栏；浏览器未公开地址栏时，网址规则不可用，但标题与图像规则仍会工作。连续帧风险融合仍未接入。
- 应用时段限制和多用户同时登录会话尚未完成；watchdog 当前跟随活动控制台会话。
- 这是用户态家长控制。管理员仍可接管文件、服务配置或脱机修改系统；没有 ELAM/PPL/内核驱动时，不能诚实承诺对 Windows 管理员绝对不可终止。

## v0.2.2 / Version 0.2.2

Service 状态改为保存在事务型 SQLite 数据库 `service-state.db` 中（WAL、`synchronous=FULL`），每次修改原子提交，崩溃或断电不再可能留下写到一半的状态文件。首次启动会自动导入旧的 `service-state.json`，并保留为 `service-state.json.migrated`。已记住的窗口标题、域名和程序路径使用 AES-256-GCM 加密保存，上限由 200 条提高到 5000 条；Agent 只接收 128 位摘要。控制台“内容识别”页支持分页和搜索已记住的标题。审计记录上限由 5000 条提高到 50000 条。不支持降级到 v0.2.1 及更早版本（旧版本会进入首次设置）。

Service state now lives in the transactional SQLite database `service-state.db` (WAL, `synchronous=FULL`); every change commits atomically, so a crash or power loss can no longer leave a half-written state file. On first start the old `service-state.json` is imported automatically and kept as `service-state.json.migrated`. Remembered window titles, hosts, and executable paths are encrypted with AES-256-GCM, and the cap rises from 200 to 5,000; the Agent receives only 128-bit digests. The Content Recognition page pages through and searches remembered titles. The audit cap rises from 5,000 to 50,000 entries. Downgrading to v0.2.1 or earlier is not supported (the older build would start with first-time setup).

**Windows 真机验收待完成 / Windows hardware acceptance pending.** 需验证：从 v0.2.1 升级后密码、策略、封禁和已记住标题均保留；`service-state.json.migrated` 已生成；重启服务后状态不变；`service-state.db` 的 ACL 仅允许 SYSTEM 和 Administrators。Verify that upgrading from v0.2.1 keeps the password, policy, bans, and remembered titles; `service-state.json.migrated` exists; state survives a service restart; and the ACL on `service-state.db` grants only SYSTEM and Administrators.

## v0.2.1 / Version 0.2.1

图像色情识别关闭应用后，记住当时的窗口标题（浏览器即网页标题）和域名；以后同一应用再显示相同标题时直接关闭，网站白名单和“允许使用”的应用除外。控制台“内容识别”页可查看、删除已记住的标题或关闭此功能；标题仅保存在本机 Service 状态中（最多 200 条），不写入审计日志。重复关闭限制默认改为“20 分钟内被关闭 5 次即禁止打开 1 小时”，时间与次数可在“应用管控”页设置，当前被限制的应用也会显示在该页。Service 现在把封禁和已记住的标题下发给 Agent，重新打开被封禁的程序会立即被关闭（此前仅在程序再次触发识别时才关闭）。

When explicit image detection closes an application, Karma now remembers that window's title (the page title in browsers) and host; the same application showing the same title again is closed immediately, except on allowlisted sites and for "Allow" applications. The Content Recognition page lists and deletes remembered titles and can turn the feature off; titles stay in the local Service state (at most 200) and never enter the audit log. The repeat-closure limit now defaults to 5 closures within 20 minutes, which blocks the application for 1 hour; both values are configurable on the Application Control page, which also lists currently restricted applications. The Service now publishes bans and remembered titles to the Agent, so a banned program is closed as soon as it is reopened (previously only when it triggered detection again).

**Windows 真机验收待完成 / Windows hardware acceptance pending.** 需验证：图像关闭后同一网页重新打开即被关闭、不同网页不受影响、删除标题后恢复、20 分钟内第 5 次关闭后 1 小时内无法打开、修改时间与次数后生效。Verify that a reopened page is closed after an image closure, other pages are unaffected, deleting a title restores access, the fifth closure within 20 minutes blocks the application for an hour, and changed thresholds take effect.

## v0.2.0 / Version 0.2.0

新增每周多时段断网和 Windows WFP 执行：覆盖浏览器、指定软件和全部应用的 IPv4/IPv6 TCP、UDP 等联网。服务启动即同步，之后每 60 秒读取一次状态；提前恢复、时段结束和保护开关变更最多约 60 秒生效。更新使用 WFP 事务，异常退出保留持久规则，正常停止及密码授权卸载清理规则。保留核心 Windows 联网基础设施和 Karma 自身，不放行其他位置的同名程序。失败与恢复写入审计日志。

Adds weekly multi-block schedules with Windows WFP enforcement for browsers, selected software, or all applications across IPv4/IPv6 and TCP/UDP. The service synchronizes on startup and every 60 seconds; early restore, schedule end, and protection changes take effect within about 60 seconds. Updates use WFP transactions; crashes retain persistent rules, while normal stop and password-authorized uninstall clean them up. Core Windows network infrastructure and Karma itself remain exempt by exact path. Failure/recovery transitions are audited.

**Windows 真机验收待完成 / Windows hardware acceptance pending.** 验收清单 / Checklist: [windows-network-enforcement-acceptance.md](https://github.com/csbuddhist/Karma/blob/v0.2.0/docs/windows-network-enforcement-acceptance.md)。需验证新连接、既有连接、UDP/QUIC、IPv6、提前恢复、重启和卸载；VPN/代理或服务代发流量可能使用中间进程身份。Verify new/existing connections, UDP/QUIC, IPv6, early restore, restart, and uninstall; VPN/proxy or service-hosted traffic may use the intermediary identity.

恢复命令（以管理员运行，仅在服务停止后使用） / Recovery command (elevated, only after the service stops):

```powershell
& 'C:\Program Files\Karma\KarmaService.exe' --clear-network-filters
```

## v0.1.14 / Version 0.1.14

新增配套守护服务 KarmaGuardian 与 KarmaService 每 5 秒互相守护：任一被“结束任务”强杀都会被服务控制管理器与对端在数秒内重新拉起。安装时收紧两个服务的 ACL（移除管理员的停止/暂停/更改配置权限）并收紧其注册表键 ACL（仅 SYSTEM 可写、管理员只读），因此在没有 Karma 密码时，services.msc / 任务管理器的“停止”、“启动类型=禁用”以及 regedit 改 `Start`/服务权限都会被拒绝。这面向“仅会使用 GUI 的非专业管理员”；懂终端命令、夺取所有权、恢复环境或离线修改的人仍可绕过。

Adds a companion KarmaGuardian service that mutually guards KarmaService every 5 seconds: an End-task kill of either is restarted within seconds by the Service Control Manager and the peer. Installation hardens both service ACLs (removing the administrator's stop/pause/change-config rights) and their registry-key ACLs (SYSTEM-writable, administrator read-only), so without the Karma password the Stop action in Services.msc / Task Manager, the "Startup type = Disabled" change, and regedit edits to `Start` / the service permissions are all denied. This targets a non-technical, GUI-only administrator; terminal commands, taking ownership, the recovery environment, or offline edits can still bypass it.

## v0.1.13 / Version 0.1.13

保护总览顶部新增总保护开关。解锁后关闭开关并点击“保存设置”停止保护；重新打开并保存恢复。项目代码采用 GPL-3.0-only，见 LICENSE；第三方资源保留各自许可证。

Protection Overview now includes the master protection switch. Unlock the console, turn it off, and click Save settings to stop protection; turn it on and save to resume. Project code is licensed under GPL-3.0-only; see LICENSE. Third-party assets retain their own licenses.
