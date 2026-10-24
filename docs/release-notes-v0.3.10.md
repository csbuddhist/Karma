# English

Karma v0.3.10 is a Windows x64 **prerelease for testing** that makes the audit log work and records why every application was closed. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**The audit log now shows its entries**

- The Service has always recorded audit entries, but the console never read them, so the **Audit Log** page and the overview's recent activity stayed empty. The console now shows the newest 500 entries, including those recorded by earlier versions.
- Entries are shown with readable names and second-level timestamps, and the page can be filtered to **App closures**.

**Every closure records its reason**

- Each closed application is logged with its program, the reason, and the result. Reasons are: image recognition (with risk score and threshold), a remembered window title, a blocked website, a window title keyword, an application set to block, or a kill while the application is restricted.
- Only image recognition can save evidence, so closures by the other rules previously left no trace. An application could be restricted after five closures without any evidence; the audit log now shows which rules closed it.
- **Currently restricted applications** on the Application Control page summarizes the reasons of the closures that caused each restriction.
- Audit details never contain window titles or URLs. Closures recorded by v0.3.9 and earlier show the reason as not recorded.

**Upgrading**

- Use `Karma-windows-x64-test-v0.3.10-setup.exe` and enter the administrator password. The console is closed during the upgrade; reopen it afterwards. Existing local policy, evidence, and audit data are retained. Installations on v0.3.2 or later can also update automatically.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.10/docs/windows-update-acceptance.md), including that the Audit Log page lists entries and that each closure shows its reason.

# 简体中文

Karma v0.3.10 是 Windows x64 **测试预发布版**，修复审计日志始终为空的问题，并记录每次关闭应用的原因。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**审计日志可以正常显示**

- Service 一直在记录审计日志，但控制台从未读取，因此 **审计日志** 页和总览的近期活动始终为空。现在控制台会显示最新 500 条记录，包括旧版本已保存的记录。
- 记录以易读的名称显示，时间精确到秒，并可筛选为 **应用关闭**。

**每次关闭都记录原因**

- 每次关闭应用都会记录程序、原因和结果。原因包括：图像识别（含风险分和阈值）、已记住的窗口标题、网站黑名单、窗口标题关键词、应用被设为禁止，以及限制期内的查杀。
- 只有图像识别会保存证据，此前其他规则触发的关闭不留任何痕迹：应用可能在累计 5 次关闭后被限制，却没有任何证据。现在可在审计日志中查到是哪条规则关闭了它。
- “应用管控”页的 **当前被限制的应用** 会汇总构成每次限制的各次关闭原因。
- 审计详情不包含窗口标题或网址。v0.3.9 及更早版本记录的关闭会显示为原因未记录。

**升级**

- 请使用 `Karma-windows-x64-test-v0.3.10-setup.exe` 并输入管理员密码。升级期间会关闭控制台，完成后重新打开即可。默认保留现有本地策略、证据和审计数据。v0.3.2 及以上版本也可通过自动更新升级。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.10/docs/windows-update-acceptance.md)中的项目，包括确认审计日志页能列出记录，且每次关闭都显示原因。
