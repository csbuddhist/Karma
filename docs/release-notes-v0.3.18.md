# English

Karma v0.3.18 is a Windows x64 **prerelease for testing** that applies website allowlists and blocklists to every browser. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Website allowlists in every browser**

- Website rules now read the address bar of every known browser, including 360, QQ, Sogou, and Quark, and of any application whose category is set to **Browser** in Application Control. Before, only Chrome, Edge, Firefox, Brave, Opera/Opera GX, Vivaldi, and Arc were read, so an allowlisted site in another browser could still be closed because of a keyword in its title.
- When a title keyword or a remembered title would close a browser whose address bar could not be read for a moment (for example while it is being edited), the Agent reads it again briefly before acting, so an allowlisted site whose title contains a keyword stays open.

**Upgrading**

- v0.3.12 through v0.3.17 can update to this version from the console or automatically. v0.3.11 and earlier must run `Karma-windows-x64-test-v0.3.18-setup.exe` once and enter the administrator password. Existing local policy, evidence, and audit data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the ONNX inference acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.18/docs/windows-onnx-acceptance.md).

# 简体中文

Karma v0.3.18 是 Windows x64 **测试预发布版**，让网址黑白名单对所有浏览器生效。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**网址白名单对所有浏览器生效**

- 网址规则现在会读取所有已知浏览器（包括 360、QQ、搜狗、夸克）以及在“应用管控”中类别设为 **浏览器** 的应用的地址栏。此前只读取 Chrome、Edge、Firefox、Brave、Opera/Opera GX、Vivaldi、Arc，其他浏览器中的白名单网站仍可能因标题含关键词而被关闭。
- 标题关键词或记住的标题将要关闭某个浏览器，而它的地址栏暂时读不到（例如正在编辑地址栏）时，Agent 会先短暂重读，因此标题含关键词的白名单网站不会被关闭。

**升级**

- v0.3.12 至 v0.3.17 可通过控制台或自动更新升级到此版本。v0.3.11 及更早版本需先运行一次 `Karma-windows-x64-test-v0.3.18-setup.exe` 并输入管理员密码。默认保留现有本地策略、证据和审计数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成 [ONNX 推理验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.18/docs/windows-onnx-acceptance.md)中的项目。
