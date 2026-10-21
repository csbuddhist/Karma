# English

Karma v0.3.1 is a Windows x64 **prerelease for testing** that fixes in-place upgrades. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**In-place upgrade fix**

- Upgrading while the management console was open always failed: the console hides to the tray instead of exiting, so its files could not be replaced. The rollback then tried to overwrite the still-running console, failed, and left the service restarting into recovery. Protection stayed stopped, and password-gated uninstall could not reach the service.
- The updater now closes the console, agent, and KarmaControl before replacing files and before every rollback attempt. They hold no data; reopen the console after the upgrade.
- Rollback no longer overwrites files that were never replaced. Brief file locks (for example, antivirus scans) are retried. A failed rollback is retried with a delay instead of looping.
- The setup upgrade script closes the console itself, so upgrading from v0.2.4–v0.3.0 with the setup program works even though those versions still run the old updater.

**Upgrading from v0.2.4–v0.3.0**

- Use `Karma-windows-x64-test-v0.3.1-setup.exe` and enter the administrator password. Existing local policy and evidence data are retained.
- Automatic updates on v0.2.4–v0.3.0 still run the old updater. If the console is open when the update starts, the old updater can still fail. If KarmaService keeps stopping right after it starts, end `karma-ui.exe` in Task Manager and start KarmaService again; the pending recovery then finishes and the previous version keeps running. Then upgrade with the setup program.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Karma remains user-mode protection: a technical Windows administrator can take ownership, use safe mode, or alter the system offline. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.1/docs/windows-update-acceptance.md), including a live in-place upgrade with the console open.

# 简体中文

Karma v0.3.1 是修复原地升级问题的 Windows x64 **测试预发布版**。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**原地升级修复**

- 管理控制台打开时升级必定失败：控制台关闭时只会隐藏到托盘，不会退出，其文件无法替换。随后回滚又试图覆盖仍在运行的控制台，同样失败，导致服务反复重启进入恢复。保护一直处于停止状态，需要密码授权的卸载也无法连接服务。
- 升级器现在会在替换文件前、以及每次回滚前，关闭控制台、Agent 和 KarmaControl。它们不保存数据，升级完成后重新打开控制台即可。
- 回滚不再覆盖未被替换的文件；文件被短暂占用（例如杀毒扫描）时会重试；回滚失败会延时重试，不再陷入死循环。
- 安装器的升级脚本会先自行关闭控制台。因此 v0.2.4–v0.3.0 虽然仍运行旧的升级器，用安装器升级也能成功。

**从 v0.2.4–v0.3.0 升级**

- 请使用 `Karma-windows-x64-test-v0.3.1-setup.exe` 并输入管理员密码。默认保留现有本地策略和证据数据。
- v0.2.4–v0.3.0 的自动更新仍运行旧的升级器，若更新开始时控制台处于打开状态，仍可能失败。如果 KarmaService 每次启动后立即停止，请在任务管理器中结束 `karma-ui.exe`，再启动 KarmaService：未完成的恢复会随之完成，并继续运行原版本。之后再用安装器升级。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。Karma 仍是用户态保护：懂技术的 Windows 管理员可夺取所有权、进入安全模式或离线修改系统。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.1/docs/windows-update-acceptance.md)中的项目，包括控制台打开时的运行中原地升级。
