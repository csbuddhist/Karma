# English

Karma v0.3.6 is a Windows x64 **prerelease for testing** that keeps browsers out of the evidence allowlist. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Browsers cannot be allowlisted from evidence**

- On the Event evidence page, **Add to allowlist** is grayed out for evidence whose source application is a browser, with an explanation on hover: allowing a browser would exempt every page it shows from image and title enforcement.
- A browser is one of the browsers Karma knows (such as Chrome, Edge, Firefox, 360, QQ Browser, or Quark) or an application rule in Application control whose category is **Browser** and that is enabled.
- The Service enforces the same rule, so a browser cannot be allowlisted from evidence even by a modified console. Other applications, such as PDF readers, can still be allowlisted as before.

**Upgrading**

- Use `Karma-windows-x64-test-v0.3.6-setup.exe` and enter the administrator password. The console is closed during the upgrade; reopen it afterwards. Existing local policy and evidence data are retained. Installations on v0.3.2 or later can also update automatically.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Karma remains user-mode protection: a technical Windows administrator can take ownership, use safe mode, or alter the system offline. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.6/docs/windows-update-acceptance.md), including that browser evidence shows a grayed-out Add to allowlist button.

# 简体中文

Karma v0.3.6 是 Windows x64 **测试预发布版**，禁止从事件证据把浏览器加入白名单。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**浏览器不能从证据加入白名单**

- “事件证据”页中，来源应用为浏览器的证据，其 **加入白名单** 按钮显示为灰色，鼠标悬停时说明原因：允许浏览器后，其中显示的所有网页都不再被图像与标题识别处置。
- 浏览器指 Karma 已知的浏览器（如 Chrome、Edge、Firefox、360、QQ 浏览器、夸克等），或“应用管控”中类别为 **浏览器** 且已启用的应用规则。
- Service 同样执行该规则，即使控制台被修改也无法从证据把浏览器加入白名单。PDF 阅读器等其他应用仍可照常加入白名单。

**升级**

- 请使用 `Karma-windows-x64-test-v0.3.6-setup.exe` 并输入管理员密码。升级期间会关闭控制台，完成后重新打开即可。默认保留现有本地策略和证据数据。v0.3.2 及以上版本也可通过自动更新升级。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。Karma 仍是用户态保护：懂技术的 Windows 管理员可夺取所有权、进入安全模式或离线修改系统。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.6/docs/windows-update-acceptance.md)中的项目，包括确认浏览器证据的“加入白名单”按钮显示为灰色。
