# English

Karma v0.3.11 is a Windows x64 **prerelease for testing** that monitors displays connected after the Agent starts. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Displays connected after startup are now monitored**

- The Agent enumerated displays only once when it started. After waking from sleep or signing in, an external monitor often reconnects a few seconds later, so it was never captured and the console showed only one **Active monitor**.
- The Agent now checks the display layout every second. When a display is added, removed, or moved, it restarts and captures every connected display, normally within 5 seconds.

**Upgrading**

- Use `Karma-windows-x64-test-v0.3.11-setup.exe` and enter the administrator password. The console is closed during the upgrade; reopen it afterwards. Existing local policy, evidence, and audit data are retained. Installations on v0.3.2 or later can also update automatically.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Karma remains user-mode protection: a technical Windows administrator can take ownership, use safe mode, or alter the system offline. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.11/docs/windows-update-acceptance.md), including that the console counts every display after resume and after plugging or unplugging a monitor.

# 简体中文

Karma v0.3.11 是 Windows x64 **测试预发布版**，修复 Agent 启动后才接入的显示器不被监控的问题。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**启动后接入的显示器也会被监控**

- 此前 Agent 只在启动时枚举一次显示器。从睡眠唤醒或登录后，外接显示器常常晚几秒才重新连接，因此一直未被采集，控制台的 **活动显示器** 只显示 1 块。
- 现在 Agent 每秒检查一次显示器布局。增加、移除或移动显示器后会自动重启并采集全部已连接的显示器，通常在 5 秒内完成。

**升级**

- 请使用 `Karma-windows-x64-test-v0.3.11-setup.exe` 并输入管理员密码。升级期间会关闭控制台，完成后重新打开即可。默认保留现有本地策略、证据和审计数据。v0.3.2 及以上版本也可通过自动更新升级。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。Karma 仍是用户态保护：懂技术的 Windows 管理员可夺取所有权、进入安全模式或离线修改系统。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.11/docs/windows-update-acceptance.md)中的项目，包括确认睡眠唤醒后以及插拔显示器后，控制台能统计到全部显示器。
