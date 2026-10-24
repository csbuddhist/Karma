# English

Karma v0.3.4 is a Windows x64 **prerelease for testing** that makes it easier to stop ordinary software, such as PDF readers, from being closed by mistake. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Bulk add applications**

- Application control has a new **Bulk add applications** button next to Add application. It lists the applications installed on this computer, with search by name, publisher, or path, and lets you check several at once (or **select all shown**).
- Checked applications become **Allow** rules for their full executable path, so image and title enforcement no longer close them (blocklisted sites still apply). A matching "Always block" rule also becomes "Allow". Applications that are already allowed are marked **Allowed**, and ones currently always blocked are marked as such.
- Known browsers are marked and excluded from "select all": allowing a browser exempts every page it shows, so it must be checked individually and a warning is shown.
- As with other application edits, the rules apply after **Save settings**, which requires the super password when one is set. The list is limited to 256 application rules.

**More installed applications found**

- The installed-application list, used by both bulk add and Add application → Choose from installed applications, now also reads Start menu (all users and the current user) and desktop shortcuts, in addition to the installed-program (Uninstall) and App Paths registrations. Shortcuts are only read; nothing is launched.

**Wording**

- The Content recognition setting is now called **Risk threshold** instead of "Risk sensitivity": a lower threshold enforces more readily, and raising it requires the super password when one is set. Its behavior is unchanged.

**Upgrading**

- Use `Karma-windows-x64-test-v0.3.4-setup.exe` and enter the administrator password. The console is closed during the upgrade; reopen it afterwards. Existing local policy and evidence data are retained. Installations on v0.3.2 or later can also update automatically.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.4/docs/windows-update-acceptance.md), including that bulk add lists the expected applications.

# 简体中文

Karma v0.3.4 是 Windows x64 **测试预发布版**，便于避免 PDF 阅读器等正常软件被误关闭。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**批量添加应用**

- “应用管控”页的“添加应用”旁新增 **批量添加应用** 按钮。它列出本机已安装的应用，支持按名称、发布者或路径搜索，可一次勾选多个（或 **全选当前结果**）。
- 勾选的应用按完整可执行文件路径设为 **允许使用**：图像与标题识别不再关闭它们（网址黑名单仍然生效）。匹配它的“始终禁止”规则也会改为“允许使用”。已允许的应用标为 **已允许**，当前为“始终禁止”的应用也会标出。
- 已知浏览器会被标出且不参与“全选”：允许浏览器后，其中显示的所有网页都不再被识别处置，因此需要单独勾选，并会显示提醒。
- 与其他应用规则修改一样，点击 **保存设置** 后生效，已设置超级密码时需验证超级密码。应用规则最多 256 条。

**能找到更多已安装应用**

- 已安装应用列表（批量添加和“添加应用 → 从已安装应用选择”共用）除 Windows 已安装程序（Uninstall）与 App Paths 注册信息外，现在还读取开始菜单（所有用户和当前用户）与桌面快捷方式。快捷方式只读取，不会被运行。

**措辞调整**

- “内容识别”中的设置由“风险灵敏度”改称 **风险阈值**：阈值越低越积极，已设置超级密码时提高阈值需验证超级密码。行为不变。

**升级**

- 请使用 `Karma-windows-x64-test-v0.3.4-setup.exe` 并输入管理员密码。升级期间会关闭控制台，完成后重新打开即可。默认保留现有本地策略和证据数据。v0.3.2 及以上版本也可通过自动更新升级。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.4/docs/windows-update-acceptance.md)中的项目，包括确认批量添加能列出预期的应用。
