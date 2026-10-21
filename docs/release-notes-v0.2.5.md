# English

Karma v0.2.5 is a Windows x64 **prerelease for testing**. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

- Fixes the console showing protection as turned off after a refused save. When a save needs the super password and authorization is cancelled or not completed, the console now restores the policy the Service last accepted and explains that the edits were reverted. Previously the refused draft stayed on screen, so the Overview protection switch and status card read "off" while the Service kept protection running. Service-side authorization is unchanged: weakening protection has always required the super password.
- Because the Service authorizes a save as a whole, cancelling authorization reverts every unsaved policy edit in that save, including edits that would have tightened protection.
- Documents the administrator and super password permissions in the README.

Download `Karma-windows-x64-test-v0.2.5-setup.exe` for installation, or the ZIP for diagnostics. Existing v0.2.4 installations upgrade in place with the administrator password; automatic updates also install this signed, strictly newer version. Existing local policy and evidence data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Karma remains user-mode protection: a technical Windows administrator can take ownership, use safe mode, or alter the system offline. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.2.5/docs/windows-update-acceptance.md), including the first live in-place upgrade from v0.2.4.

# 简体中文

Karma v0.2.5 是 Windows x64 **测试预发布版**。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

- 修复保存被拒后控制台显示保护已关闭的问题。保存需要超级密码而授权被取消或未完成时，控制台现在会恢复为保护服务最后接受的策略，并提示修改已撤销。此前被拒绝的草稿仍留在界面上，总览中的总保护开关和保护状态卡片显示“关闭”，而服务实际仍在保护。服务端授权未变：削弱保护一直需要超级密码。
- 服务按整次保存进行授权，因此取消授权会撤销该次保存中全部未保存的策略修改，包括本可加强保护的修改。
- README 新增普通管理员密码与超级密码的权限说明。

安装请下载 `Karma-windows-x64-test-v0.2.5-setup.exe`，诊断可使用 ZIP。已安装的 v0.2.4 可输入管理员密码原地升级；自动更新也会安装这一经过签名的更高版本。默认保留现有本地策略和证据数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。Karma 仍是用户态保护：懂技术的 Windows 管理员可夺取所有权、进入安全模式或离线修改系统。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.2.5/docs/windows-update-acceptance.md)中的项目，包括首次从 v0.2.4 实际原地升级。
