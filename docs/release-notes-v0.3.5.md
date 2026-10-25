# English

Karma v0.3.5 is a Windows x64 **prerelease for testing** that keeps trusted applications out of the evidence list. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Skip allowlisted applications in event evidence**

- The Event evidence page has a new **Skip allowlisted applications** switch. When it is on, a high-risk capture attributed to an application set to **Allow** in Application control (for example a PDF reader added with Bulk add applications) is discarded instead of being saved as evidence.
- The switch is off by default and takes effect after **Save settings**. Evidence that is already saved is kept, and captures whose source application cannot be reliably attributed are still saved.
- The check runs in the Service, which decides from its own policy rather than trusting the Agent. Like turning evidence saving off, the switch changes only what is recorded, not what is enforced, so it does not require the super password.

**Upgrading**

- Use `Karma-windows-x64-test-v0.3.5-setup.exe` and enter the administrator password. The console is closed during the upgrade; reopen it afterwards. Existing local policy and evidence data are retained. Installations on v0.3.2 or later can also update automatically.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.5/docs/windows-update-acceptance.md), including that evidence from an allowed application is not saved while the switch is on.

# 简体中文

Karma v0.3.5 是 Windows x64 **测试预发布版**，让受信任的应用不再进入证据列表。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**事件证据：不拦截白名单应用**

- “事件证据”页新增 **不拦截白名单应用** 开关。开启后，来源为“应用管控”中“允许使用”应用（例如通过“批量添加应用”加入的 PDF 阅读器）的高风险画面会被直接丢弃，不再保存为证据。
- 开关默认关闭，点击 **保存设置** 后生效。已保存的证据不受影响；来源应用无法可靠归属的画面仍会保存。
- 判断在 Service 中依据其自身策略进行，不依赖 Agent。与关闭证据保存一样，该开关只影响记录内容、不影响处置，因此不需要超级密码。

**升级**

- 请使用 `Karma-windows-x64-test-v0.3.5-setup.exe` 并输入管理员密码。升级期间会关闭控制台，完成后重新打开即可。默认保留现有本地策略和证据数据。v0.3.2 及以上版本也可通过自动更新升级。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.5/docs/windows-update-acceptance.md)中的项目，包括确认开关开启时“允许使用”应用的画面不会保存为证据。
