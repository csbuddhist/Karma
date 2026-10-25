# English

Karma v0.3.20 is a Windows x64 **prerelease for testing** that stops two built-in title keywords from closing ordinary pages. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Fewer false title-keyword closures**

- The built-in Japanese keyword “s ＆ m” matched anywhere in a title, so titles such as “IEEE Journals & Magazine | IEEE Xplore” closed the browser (“journal**s & m**agazine”). It has been removed; the English keyword “s&m” still blocks the term as a whole word.
- The built-in keyword “3p” also matched inside words such as “3pm”. It now matches only the whole word and has moved to the keywords prone to false positives, which are off by default.
- The built-in list now has 2,542 terms: 1,590 strict and 952 prone to false positives.

**Upgrading**

- v0.3.12 through v0.3.19 can update to this version from the console or automatically. v0.3.11 and earlier must run `Karma-windows-x64-test-v0.3.20-setup.exe` once and enter the administrator password. Existing local policy, evidence, and audit data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the ONNX inference acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.20/docs/windows-onnx-acceptance.md).

# 简体中文

Karma v0.3.20 是 Windows x64 **测试预发布版**，修正了两个会误关普通网页的内置标题关键词。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**减少标题关键词误判**

- 内置日文关键词“s ＆ m”会在标题任意位置匹配，导致“IEEE Journals & Magazine | IEEE Xplore”这类标题被关闭（“journal**s & m**agazine”）。该词已删除；英文关键词“s&m”仍按整词拦截。
- 内置关键词“3p”也会匹配“3pm”等单词内部。现改为只匹配整词，并移入默认关闭的易误报词。
- 内置词库现为 2,542 个词：严格 1,590 个，易误报 952 个。

**升级**

- v0.3.12 至 v0.3.19 可通过控制台或自动更新升级到此版本。v0.3.11 及更早版本需先运行一次 `Karma-windows-x64-test-v0.3.20-setup.exe` 并输入管理员密码。默认保留现有本地策略、证据和审计数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成 [ONNX 推理验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.20/docs/windows-onnx-acceptance.md)中的项目。
