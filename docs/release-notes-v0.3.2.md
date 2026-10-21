# English

Karma v0.3.2 is a Windows x64 **prerelease for testing** that completes the in-place upgrade fix from v0.3.1. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Setup upgrade fix**

- The v0.3.1 setup program still failed to upgrade v0.3.0 while the management console was open. Setup runs its upgrade script in 32-bit PowerShell, which cannot read the path of the 64-bit console, so the script silently skipped closing it. The older updater then hit the same loop that v0.3.1 was meant to fix.
- The upgrade script now finds the console through Windows process information that works from 32-bit PowerShell, closes it, and refuses to request the upgrade if the console is still running. In that case nothing is changed.
- v0.3.1 service-side fixes are included: the updater closes the console, agent, and KarmaControl before replacing files, rollback never overwrites files that were not replaced, and a failed rollback is retried with a delay instead of looping.

**Upgrading**

- Use `Karma-windows-x64-test-v0.3.2-setup.exe` and enter the administrator password. The console is closed during the upgrade; reopen it afterwards. Existing local policy and evidence data are retained.
- Automatic updates on v0.2.4–v0.3.0 still run their old updater. If KarmaService keeps stopping right after it starts, end `karma-ui.exe` in Task Manager and start KarmaService again; the pending recovery then finishes and the previous version keeps running.
- If the v0.3.1 setup program is still waiting after it reports that KarmaService accepted the update, end `karma-ui.exe` in Task Manager; the service then recovers.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Karma remains user-mode protection: a technical Windows administrator can take ownership, use safe mode, or alter the system offline. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.2/docs/windows-update-acceptance.md), including a live in-place upgrade with the console open.

# 简体中文

Karma v0.3.2 是 Windows x64 **测试预发布版**，完成 v0.3.1 开始的原地升级修复。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**安装器升级修复**

- v0.3.1 安装器在管理控制台打开时仍无法升级 v0.3.0。安装器以 32 位 PowerShell 运行升级脚本，读不到 64 位控制台的路径，脚本因此静默跳过了关闭控制台，旧升级器随即陷入 v0.3.1 本应修复的循环。
- 升级脚本现在改用 32 位 PowerShell 也能读取的 Windows 进程信息查找并关闭控制台；控制台仍在运行时拒绝提交升级，不做任何更改。
- 包含 v0.3.1 的服务端修复：替换文件前关闭控制台、Agent 和 KarmaControl；回滚不覆盖未被替换的文件；回滚失败会延时重试，不再死循环。

**升级**

- 请使用 `Karma-windows-x64-test-v0.3.2-setup.exe` 并输入管理员密码。升级期间会关闭控制台，完成后重新打开即可。默认保留现有本地策略和证据数据。
- v0.2.4–v0.3.0 的自动更新仍运行旧的升级器。如果 KarmaService 每次启动后立即停止，请在任务管理器中结束 `karma-ui.exe`，再启动 KarmaService：未完成的恢复会随之完成，并继续运行原版本。
- 如果 v0.3.1 安装器显示 “KarmaService accepted the update request” 后一直等待，请在任务管理器中结束 `karma-ui.exe`，服务随即恢复。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。Karma 仍是用户态保护：懂技术的 Windows 管理员可夺取所有权、进入安全模式或离线修改系统。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.2/docs/windows-update-acceptance.md)中的项目，包括控制台打开时的运行中原地升级。
