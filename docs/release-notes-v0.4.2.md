# English

Karma v0.4.2 is a Windows x64 **prerelease for testing** that refines remembered window titles in Content Recognition. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**A program's own name is never remembered**

- When image recognition closed a window whose title was only the program's name (for example "Google Chrome" or "微信"), that name used to be remembered, and the program was then closed every time it opened, whatever it showed.
- Such titles are no longer remembered. A title counts as the program's name when it matches the executable's file name or the file description or product name in its version information, ignoring case, full-width characters, spaces, and punctuation. Page titles such as "Some video - Google Chrome" are remembered as before.

**Remembered titles are deleted after 5 hours**

- Each remembered title is now deleted automatically 5 hours after it was last remembered. It then stops closing windows and leaves the list on the Content Recognition page. A title that image recognition closes again is remembered again for another 5 hours.
- Titles remembered by earlier versions follow the same rule, so after updating, those older than 5 hours are deleted at once, including any program names remembered by mistake.
- Titles marked as misjudged are kept as before.

**Upgrading**

- v0.3.12 through v0.4.1 can update to this version from the console or automatically. v0.3.11 and earlier must run `Karma-windows-x64-test-v0.4.2-setup.exe` once and enter the administrator password. Existing local policy, evidence, and audit data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the update acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.4.2/docs/windows-update-acceptance.md) and [the ONNX inference acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.4.2/docs/windows-onnx-acceptance.md).

# 简体中文

Karma v0.4.2 是 Windows x64 **测试预发布版**，改进了“内容识别”中的记住窗口标题功能。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**不再记住程序自身的名称**

- 此前，图像识别关闭的窗口如果标题只是程序名称（例如“Google Chrome”或“微信”），这个名称也会被记住，之后该程序无论显示什么内容，一打开就会被关闭。
- 现在不再记住这类标题。标题与可执行文件名、或程序版本信息中的文件说明或产品名称相同时（忽略大小写、全半角、空格和标点），即视为程序名称。“某视频 - Google Chrome”这样的网页标题仍照常记住。

**已记住的标题 5 小时后自动删除**

- 每条已记住的标题在最近一次记住 5 小时后自动删除，之后不再关闭窗口，也从“内容识别”页的列表中消失。图像识别再次关闭同一标题时，会重新记住 5 小时。
- 旧版本记住的标题同样适用，升级后超过 5 小时的标题会立即删除，包括此前误记的程序名称。
- 已标记为误判的标题照常保留。

**升级**

- v0.3.12 至 v0.4.1 可通过控制台或自动更新升级到此版本。v0.3.11 及更早版本需先运行一次 `Karma-windows-x64-test-v0.4.2-setup.exe` 并输入管理员密码。默认保留现有本地策略、证据和审计数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成 [升级验收清单](https://github.com/csbuddhist/Karma/blob/v0.4.2/docs/windows-update-acceptance.md) 和 [ONNX 推理验收清单](https://github.com/csbuddhist/Karma/blob/v0.4.2/docs/windows-onnx-acceptance.md)中的项目。
