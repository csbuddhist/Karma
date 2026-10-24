# English

Karma v0.3.14 is a Windows x64 **prerelease for testing** that makes it quicker to put closed applications into categories and makes unsaved settings easier to notice. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Categorize closed applications from the audit log**

- Each app-closure entry in the Audit Log now has an application type selector. Choosing a category puts that program in it, so a blocking schedule for the category also blocks the program from the network or from running. A program that already has a rule keeps its action; a new one uses “Explicit content only”. Common browsers always stay in the Browser category.
- The change takes effect after Save settings, like any other policy change.

**Console layout**

- Applications now shows the Application types panel above the application list.
- Save settings and Lock console moved from the page header to the sidebar, below System Settings. When settings are changed but not saved, the sidebar shows an “unsaved changes” notice and the save button is highlighted until you save.

**Upgrading**

- v0.3.12 and v0.3.13 can update to this version from the console or automatically. v0.3.11 and earlier must run `Karma-windows-x64-test-v0.3.14-setup.exe` once and enter the administrator password. Existing local policy, evidence, and audit data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Karma remains user-mode protection: a technical Windows administrator can take ownership, use safe mode, or alter the system offline. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the network enforcement acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.14/docs/windows-network-enforcement-acceptance.md).

# 简体中文

Karma v0.3.14 是 Windows x64 **测试预发布版**，可更快地把被关闭的应用归入类别，并让未保存的设置更醒目。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**在审计日志中为被关闭的应用归类**

- 审计日志中每条应用关闭记录新增“应用类型”选择框。选择类别后，该程序即加入此类别，针对该类别的断网屏蔽规则也会禁止它联网或启动。已有规则的程序保留原处置方式；新加入的程序为“仅色情内容处置”。常见浏览器始终属于“浏览器”类别。
- 与其他策略修改一样，点击“保存设置”后生效。

**控制台布局**

- “应用管控”页面中，“应用类型”面板移到应用列表上方。
- “保存设置”和“锁定控制台”按钮从页面顶部移到左侧菜单栏“系统设置”下方。修改设置但尚未保存时，菜单栏会提示“有设置未保存”，保存按钮保持高亮，直到保存为止。

**升级**

- v0.3.12 和 v0.3.13 可通过控制台或自动更新升级到此版本。v0.3.11 及更早版本需先运行一次 `Karma-windows-x64-test-v0.3.14-setup.exe` 并输入管理员密码。默认保留现有本地策略、证据和审计数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。Karma 仍是用户态保护：懂技术的 Windows 管理员可夺取所有权、进入安全模式或离线修改系统。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[网络执行验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.14/docs/windows-network-enforcement-acceptance.md)中的项目。
