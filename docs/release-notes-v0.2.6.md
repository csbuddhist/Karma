# English

Karma v0.2.6 is a Windows x64 **prerelease for testing**. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

- Adds **Always allow** to each entry under Applications → Currently restricted applications. One click sets that application to "Allow" in the application list and lifts its current restriction immediately, so it can be opened again. When a super password is set, the Service requires it; without one, the administrator session suffices, as for other weakening changes.
- An "Always block" rule that matches the application is also changed to "Allow", because a block rule would otherwise outrank the new allow rule. The application's earlier closure count is cleared, so those closures cannot restrict it again.
- The action is saved immediately and on its own: other unsaved edits in the console are kept and are not saved with it. The Service accepts it only for applications that are currently restricted, refuses it if the settings changed in another session, and audits every authorized or denied attempt.

Download `Karma-windows-x64-test-v0.2.6-setup.exe` for installation, or the ZIP for diagnostics. Existing v0.2.4 and v0.2.5 installations upgrade in place with the administrator password; automatic updates also install this signed, strictly newer version. Existing local policy and evidence data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.2.6/docs/windows-update-acceptance.md), including live in-place upgrades and the new one-click allow on a restricted application.

# 简体中文

Karma v0.2.6 是 Windows x64 **测试预发布版**。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

- “应用管控 → 当前被限制的应用”中的每个应用新增**一键允许**。点击后，该应用会在应用列表中设为“允许使用”，并立即解除当前限制，可以重新打开。已设置超级密码时，服务要求验证超级密码；未设置时，与其他削弱保护的修改一样，使用普通管理员会话即可。
- 匹配该应用的“始终禁止”规则也会改为“允许使用”，否则禁止规则会优先于新的允许规则。该应用此前的关闭次数会被清除，不会因这些关闭再次被限制。
- 该操作立即单独保存：控制台中其他尚未保存的修改会保留，但不会一并保存。服务仅接受当前被限制的应用；若设置已被另一个管理会话修改则拒绝；每次授权成功或被拒都会写入审计。

安装请下载 `Karma-windows-x64-test-v0.2.6-setup.exe`，诊断可使用 ZIP。已安装的 v0.2.4、v0.2.5 可输入管理员密码原地升级；自动更新也会安装这一经过签名的更高版本。默认保留现有本地策略和证据数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.2.6/docs/windows-update-acceptance.md)中的项目，包括运行中原地升级，以及对被限制应用使用一键允许。
