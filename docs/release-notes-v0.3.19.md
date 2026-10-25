# English

Karma v0.3.19 is a Windows x64 **prerelease for testing** that records which keyword closed a window. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Matched keyword in the audit log**

- When a window is closed because its title matches a keyword, the audit log now shows the keyword after the reason, for example “Window title keyword "ss**"”. The keyword is masked before the Service writes it: its last Chinese character, or otherwise its last two English characters, become `*`, and the first letter is kept.
- Window titles are still never recorded. Restriction summaries on the Application Control page keep grouping closures by reason alone. Entries written before this version have no keyword and show the reason only.

**Upgrading**

- v0.3.12 through v0.3.18 can update to this version from the console or automatically. v0.3.11 and earlier must run `Karma-windows-x64-test-v0.3.19-setup.exe` once and enter the administrator password. Existing local policy, evidence, and audit data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the ONNX inference acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.19/docs/windows-onnx-acceptance.md).

# 简体中文

Karma v0.3.19 是 Windows x64 **测试预发布版**，在审计日志中记录是哪个关键词导致窗口被关闭。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**审计日志显示命中的关键词**

- 因窗口标题命中关键词而关闭窗口时，审计日志会在原因后显示该关键词，例如“窗口标题命中关键词“赌球直*””。关键词由 Service 打码后才写入：最后一个汉字，或者最后 2 个英文字符，以 `*` 代替，首个字母保留。
- 仍不记录窗口标题。“应用管控”页的限制汇总仍只按原因分组。此版本之前写入的记录没有关键词，只显示原因。

**升级**

- v0.3.12 至 v0.3.18 可通过控制台或自动更新升级到此版本。v0.3.11 及更早版本需先运行一次 `Karma-windows-x64-test-v0.3.19-setup.exe` 并输入管理员密码。默认保留现有本地策略、证据和审计数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成 [ONNX 推理验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.19/docs/windows-onnx-acceptance.md)中的项目。
