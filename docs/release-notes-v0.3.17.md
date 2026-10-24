# English

Karma v0.3.17 is a Windows x64 **prerelease for testing** that protects critical event evidence with the super password and makes the video-code keyword list easier to read. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Critical evidence needs the super password to delete**

- Once a super password is set, deleting evidence marked **Critical** (risk score of at least 95%) requires it. The console asks for the super password when needed.
- A bulk deletion that includes any critical item is refused as a whole without the super password, so nothing in it is deleted. High-risk evidence can still be deleted with the ordinary administrator session.
- Without a super password, the administrator deletes evidence as before.

**Video codes in their own keyword group**

- The Keywords page now lists the 1,809 video code prefixes in a separate **Video codes** group instead of among Japanese words; codes match window titles in any language.
- Each prefix is shown as `DCX-###` instead of `DCX-123`: any 3–4 digits match, such as `DCX-001`, `DCX 1234`, or `DCX0456`. Hover over a prefix for matching examples. Adding a code no longer asks for a language. Matching itself is unchanged.

**System settings**

- The security note card was removed from System settings.

**Upgrading**

- v0.3.12 through v0.3.16 can update to this version from the console or automatically. v0.3.11 and earlier must run `Karma-windows-x64-test-v0.3.17-setup.exe` once and enter the administrator password. Existing local policy, evidence, and audit data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the ONNX inference acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.17/docs/windows-onnx-acceptance.md).

# 简体中文

Karma v0.3.17 是 Windows x64 **测试预发布版**，用超级密码保护“严重”事件证据，并让番号关键词列表更易读。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**删除“严重”证据需要超级密码**

- 设置超级密码后，删除标为 **严重** 的证据（风险分不低于 95%）需要超级密码，控制台会在需要时提示输入。
- 批量删除中只要包含严重证据，未提供超级密码时整批都会被拒绝，一条也不删除。“高风险”证据仍可用普通管理员会话删除。
- 未设置超级密码时，管理员删除证据的方式与之前相同。

**番号单独分组**

- “关键词”页面把 1,809 个番号前缀单独列为 **番号** 分组，不再混在日语词中；番号匹配任何语言的窗口标题。
- 每个前缀显示为 `DCX-###`，不再显示为 `DCX-123`：后接任意 3–4 位数字都会命中，如 `DCX-001`、`DCX 1234`、`DCX0456`。鼠标悬停可查看匹配示例。补充番号时不再需要选择语言。匹配规则本身没有变化。

**系统设置**

- 移除了“系统设置”中的“安全说明”卡片。

**升级**

- v0.3.12 至 v0.3.16 可通过控制台或自动更新升级到此版本。v0.3.11 及更早版本需先运行一次 `Karma-windows-x64-test-v0.3.17-setup.exe` 并输入管理员密码。默认保留现有本地策略、证据和审计数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成 [ONNX 推理验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.17/docs/windows-onnx-acceptance.md)中的项目。
