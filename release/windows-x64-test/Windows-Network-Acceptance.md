# Windows network enforcement acceptance / Windows 网络执行验收

## Status / 状态

Windows hardware acceptance is **pending**. macOS unit tests, Windows cross-compilation, and packaging checks do not prove live WFP behavior. Complete both IPv4 and IPv6 cases before recording acceptance.

Windows 真机验收**待完成**。macOS 单元测试、Windows 交叉编译和打包检查不能证明实际 WFP 行为。IPv4 和 IPv6 场景均完成后才能记录验收通过。

## Preparation / 准备

Install the unsigned `v0.2.0` Windows x64 test build as administrator on Windows 10 22H2 or Windows 11 with the Visual C++ 2015–2022 x64 runtime. Upgrade using the existing password-authorized uninstall flow; policy and evidence remain in ProgramData. Enable protection and prepare a browser, a second browser registered with the browser category, two copies of a harmless network client in different directories, and IPv4/IPv6 TCP and UDP test endpoints you control. Use only synthetic traffic. The service applies schedules at startup, then every **60 seconds**; wait up to 65 seconds after a change before judging the result. GUI schedule status describes the computed schedule; it is not proof of successful WFP installation.

在 Windows 10 22H2 或 Windows 11 真机安装 Visual C++ 2015–2022 x64 运行库，以管理员身份安装未签名的 `v0.2.0` Windows x64 测试版。升级沿用现有密码授权卸载流程，ProgramData 中保留策略和证据。启用保护，准备一个常见浏览器、一个归类为浏览器的自定义程序、放在不同目录的两个无害联网客户端副本，以及自己控制的 IPv4/IPv6 TCP、UDP 测试端点；只使用合成测试流量。服务启动时同步，此后每 **60 秒**读取状态；变更后最多等待 65 秒再判断。GUI 的时段状态反映计算结果，不能证明 WFP 下发成功。

## Functional checks / 功能检查

| Case / 场景 | Expected / 预期 |
| --- | --- |
| No active block / 无生效时段 | Clients connect; no Karma block filters / 客户端可联网，无 Karma 阻断过滤器 |
| Browsers / 浏览器 | Known and custom-category browsers blocked; other clients work / 常见及自定义分类浏览器被阻断，其他客户端可联网 |
| Newly launched process / 新启动进程 | Close and reopen a targeted browser after the filters are installed; its first connection is blocked / 规则安装后重新启动目标浏览器，首次连接即被阻断 |
| Executable name / 文件名 | Both copies of `client.exe` are blocked; `otherclient.exe` is unaffected / 两处 `client.exe` 均被阻断，`otherclient.exe` 不受影响 |
| Full executable path / 完整路径 | Only the selected copy is blocked; another directory's same-name executable works / 仅指定位置被阻断，另一目录的同名程序可联网 |
| Absent executable / 程序尚不存在 | Configure a full path on an existing local drive before copying the client there; its first connection is blocked after copying / 在已存在的本地盘配置尚不存在的完整路径，随后复制程序，其首次连接被阻断 |
| Overlap / 重叠规则 | Browser + selected-application blocks combine; ending one leaves the other in force / 浏览器与指定应用规则合并，单条结束后其他规则继续生效 |
| All network / 全部断网 | Ordinary clients, including Windows curl/PowerShell, are blocked; Karma IPC/UI, DNS/DHCP and core system networking remain usable / 普通客户端及 Windows curl/PowerShell 被阻断，Karma IPC/UI、DNS/DHCP 和核心系统联网保持可用 |
| IPv4/IPv6 and protocols / 地址族与协议 | Repeat targeted/all-network tests for IPv4 and IPv6, TCP and UDP/QUIC, inbound and outbound; test loopback proxy traffic separately / 对 IPv4、IPv6、TCP、UDP/QUIC、入站及出站重复目标及全部断网测试，单独检查回环代理流量 |
| Existing connections / 既有连接 | Start a TCP download/stream and UDP/QUIC flow before the block; after installation, verify traffic ceases on reauthorization; reconnect works after restore / 阻断前建立 TCP 下载/流和 UDP/QUIC 连接，安装规则后确认重新授权停止传输，恢复后可重新连接 |
| Expiry, early restore, master switch / 到期、提前恢复、总开关 | Schedule end, correct-password restore and disabling protection remove blocks by the next poll; wrong password does not lift them; reenable/next block reapplies / 时段结束、正确密码恢复及关闭保护在下一轮询撤销规则，错误密码不放行，重新启用或下次时段重新阻断 |
| Cross-midnight and local zone / 跨午夜与本地时区 | Block begins/ends according to local time, including Sunday→Monday / 按本地时间开始及结束，覆盖周日跨周一 |

## Lifecycle and failures / 生命周期与失败

1. Force-end KarmaService in an isolated lab during a block. Persistent filters remain until SCM/Guardian restarts it; startup reconciles the current schedule, including an expired block or a persisted early restore. Reboot and check again. A normal service stop removes Karma filters, sublayer and provider.
2. In an isolated lab, interrupt BFE/RPC availability and verify a `network_enforcement` failure audit entry, unchanged previously committed policy where BFE remains active, then recovery at a subsequent poll. Check a bad/inaccessible full path does not partially replace the installed set. Repeated identical failures must not flood the audit log.
3. Export WFP state before and after repeated polls using elevated `netsh wfp show state file=...`. Find provider `9ddf8cd3-8b51-4ac7-9dca-c3cc0f175663` and sublayer `0bd6b682-e7ad-4ba7-a97d-8d7ee779ebaa`; verify all four ALE layers, stable filter counts when unchanged, and removal of block/permit filters when cleared. Another provider's rules must remain intact. Exemption permits must not bypass another firewall's block.
4. Password-authorized uninstall/upgrade during a block must restore access and leave no Karma WFP objects. If cleanup fails, uninstall must retain the binaries for recovery. Repeat with a stopped service or missing SCM registration. Keep BFE running for cleanup.
5. With the service stopped, the elevated command below removes only Karma WFP objects. Standard users must be refused by WFP. Restarting the service reapplies the current schedule. Administrators with terminal/ownership access can still bypass this user-mode test build.

1. 在隔离测试环境的断网时段强制结束 KarmaService，确认持久规则保留至 SCM/Guardian 重启；启动时按当前状态同步，包括时段已结束或已有持久化提前恢复。重启 Windows 后再验证。正常停止服务应删除 Karma 过滤器、子层和 provider。
2. 在隔离环境中断 BFE/RPC 可用性，确认 `network_enforcement` 失败审计；BFE 仍活动时保留先前提交的策略，后续轮询恢复。错误或不可访问的完整路径不能导致部分替换，重复相同失败不能刷屏审计日志。
3. 以管理员运行 `netsh wfp show state file=...`，对比重复轮询前后的 WFP 状态；查找 provider `9ddf8cd3-8b51-4ac7-9dca-c3cc0f175663` 和子层 `0bd6b682-e7ad-4ba7-a97d-8d7ee779ebaa`，确认四个 ALE 层齐全、状态不变时规则数量稳定、解除时清除 block/permit 过滤器。其他 provider 的规则必须保留，豁免 permit 不能绕过其他防火墙的阻断。
4. 在断网时段执行密码授权卸载/升级，确认恢复联网且无 Karma WFP 对象残留。清理失败时卸载应保留二进制供恢复；重复服务已停止或 SCM 注册已丢失的情况。清理时保持 BFE 运行。
5. 服务停止后，以管理员执行以下命令，仅删除 Karma 的 WFP 对象；标准用户必须被 WFP 拒绝。重启服务后重新应用当前时段。拥有终端/夺取所有权能力的管理员仍能绕过此用户态测试版。

```powershell
& 'C:\Program Files\Karma\KarmaService.exe' --clear-network-filters
```

## Evidence / 记录

Record build hash, installer SHA-256, Windows version, time zone, interface/VPN/proxy/security-product configuration, app paths/versions, protocol/address family, action time, enforcement time, result, filter counts and redacted audit transitions. Do not commit browsing history, real destinations, packet contents, credentials or unredacted WFP dumps. Mark unsupported IPv6/VPN cases as untested, never passed. System-hosted/proxied traffic can inherit an exempt host's identity; record this limitation explicitly.

记录构建提交、安装器 SHA-256、Windows 版本、时区、网卡/VPN/代理/安全软件配置、应用路径与版本、协议与地址族、操作及生效时间、结果、过滤器数量和脱敏审计状态变化。不要提交浏览记录、真实目的地址、数据包内容、凭据或未脱敏 WFP 导出。无条件验证的 IPv6/VPN 场景应标为未测，不能记通过；系统宿主/代理代发流量可能继承被豁免宿主的身份，需明确记录此限制。

Implementation references / 实现参考: [Microsoft WFP application identities](https://learn.microsoft.com/en-us/windows/win32/fwp/permitting-and-blocking-applications-and-users), [suffix match semantics](https://learn.microsoft.com/en-us/windows/win32/api/fwptypes/ne-fwptypes-fwp_match_type), [filter arbitration](https://learn.microsoft.com/en-us/windows/win32/fwp/filter-arbitration), [ALE reauthorization](https://learn.microsoft.com/en-us/windows/win32/fwp/ale-re-authorization).
