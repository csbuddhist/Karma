# English

Karma v0.3.12 is a Windows x64 **prerelease for testing** that makes in-place updates from the console work. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**In-place updates no longer roll back**

- Before replacing files, the updater must close the console and the Agent. It asked Windows Restart Manager to do this, but Restart Manager cannot close programs running in the signed-in user's session from the Service, so every update rolled back and the console reported that the update could not be saved and to check disk space.
- The updater now closes only processes whose full path is one of Karma's own installed programs, in every session, and then applies the update.
- After a Service restart, an Agent left by the previous Service instance is closed first, so two Agents never run side by side.

**Upgrading**

- **v0.3.11 and earlier cannot update themselves to this version from the console**, because the old updater still performs the upgrade. Install `Karma-windows-x64-test-v0.3.12-setup.exe` once and enter the administrator password. The console is closed during the upgrade; reopen it afterwards. Existing local policy, evidence, and audit data are retained. Later versions can then be installed from the console or automatically.
- Includes v0.3.11: displays connected after the Agent starts are monitored.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Karma remains user-mode protection: a technical Windows administrator can take ownership, use safe mode, or alter the system offline. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.12/docs/windows-update-acceptance.md), including that a console-triggered update from v0.3.12 to a later version completes while the console is open.

# 简体中文

Karma v0.3.12 是 Windows x64 **测试预发布版**，修复从控制台原地更新总是失败的问题。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**原地更新不再回滚**

- 替换文件前，更新程序需要先关闭控制台和 Agent。此前它交给 Windows Restart Manager 处理，但 Restart Manager 无法从服务中关闭登录用户会话里的程序，因此每次更新都会回滚，控制台提示“无法保存更新文件，请检查可用磁盘空间”。
- 现在更新程序只关闭完整路径属于 Karma 自身安装程序的进程（覆盖所有会话），然后再应用更新。
- 服务重启后会先关闭上一个服务实例遗留的 Agent，避免两个 Agent 同时运行。

**升级**

- **v0.3.11 及更早版本无法从控制台直接更新到此版本**，因为升级仍由旧的更新程序执行。请先运行一次 `Karma-windows-x64-test-v0.3.12-setup.exe` 并输入管理员密码。升级期间会关闭控制台，完成后重新打开即可。默认保留现有本地策略、证据和审计数据。之后的版本即可通过控制台或自动更新安装。
- 包含 v0.3.11 的修复：Agent 启动后才接入的显示器也会被监控。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。Karma 仍是用户态保护：懂技术的 Windows 管理员可夺取所有权、进入安全模式或离线修改系统。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.12/docs/windows-update-acceptance.md)中的项目，包括确认在控制台打开时，从 v0.3.12 通过控制台更新到后续版本能够完成。
