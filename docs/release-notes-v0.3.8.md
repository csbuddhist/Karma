# English

Karma v0.3.8 is a Windows x64 **prerelease for testing** that protects **Launch at login** with the super password. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Turning off Launch at login requires the super password**

- Once a super password is set, saving **System Settings → Launch at login** as off is treated as weakening protection, so the Service refuses it without the super password. Before, the administrator session could turn it off, and a restart would then leave the console closed.
- If authorization is cancelled, the console restores the saved setting and explains that the edits were reverted. Turning the option on still needs only the administrator session. Without a super password, behavior is unchanged.
- The switch controls only the administration console and its tray icon. The protection Service and the monitoring Agent start independently and keep running whether or not it is on.
- The documentation is reorganized: `README.md` is now the Chinese homepage and `README.en.md` its English equivalent, with architecture details in `技术说明.md` and `TECHNICAL.md`.

**Upgrading**

- Use `Karma-windows-x64-test-v0.3.8-setup.exe` and enter the administrator password. The console is closed during the upgrade; reopen it afterwards. Existing local policy and evidence data are retained. Installations on v0.3.2 or later can also update automatically.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.8/docs/windows-update-acceptance.md), including that Launch at login cannot be turned off without the super password.

# 简体中文

Karma v0.3.8 是 Windows x64 **测试预发布版**，“登录后自动启动”改由超级密码保护。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**关闭“登录后自动启动”需要超级密码**

- 设置超级密码后，在 **系统设置 → 登录后自动启动** 中关闭并保存，会被视为削弱保护，未提供超级密码时 Service 拒绝保存。此前普通管理员会话即可关闭，重启后控制台便不再运行。
- 取消授权时，控制台会恢复为已保存的设置，并提示修改已撤销。开启此项仍只需普通管理员会话；未设置超级密码时行为不变。
- 该开关只控制管理控制台及其托盘图标。保护服务和监控 Agent 独立启动，无论开关是否开启都会持续运行。
- 文档已重新整理：`README.md` 现为中文主页，`README.en.md` 为对应英文版，架构与实现细节见 `技术说明.md` 和 `TECHNICAL.md`。

**升级**

- 请使用 `Karma-windows-x64-test-v0.3.8-setup.exe` 并输入管理员密码。升级期间会关闭控制台，完成后重新打开即可。默认保留现有本地策略和证据数据。v0.3.2 及以上版本也可通过自动更新升级。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.8/docs/windows-update-acceptance.md)中的项目，包括确认未提供超级密码时无法关闭“登录后自动启动”。
