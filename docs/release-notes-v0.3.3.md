# English

Karma v0.3.3 is a Windows x64 **prerelease for testing** that adds two application-control conveniences. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Add an application to the allowlist from evidence**

- Each item on the Event evidence page now has an **Add to allowlist** button next to delete. After you confirm, the application the evidence was attributed to is added to Application control with the **Allow** action, so image and title enforcement no longer close it (blocklisted sites still apply). A matching "Always block" rule also becomes "Allow", and any repeat-closure restriction on the application is lifted.
- The super password is required when one is set. The change is saved immediately, separately from other unsaved edits, and recorded in the audit log as `evidence_application_allowed`.
- The Service takes the executable path from its own evidence record; the console sends only the evidence ID. Evidence whose source application could not be reliably attributed cannot be allowlisted, and the button shows "Allowlisted" when the application is already allowed.

**Choose an installed application**

- The Add application dialog can now **choose from installed applications** as well as accept a typed executable. The console lists programs from the Windows installed-program (Uninstall) and App Paths registrations, with search by name, publisher, or path. Choosing one fills in the name and executable; review the category and save as usual.
- Microsoft Store apps that are not registered there are not listed; enter their executable manually.

**Upgrading**

- Use `Karma-windows-x64-test-v0.3.3-setup.exe` and enter the administrator password. The console is closed during the upgrade; reopen it afterwards. Existing local policy and evidence data are retained. Installations on v0.3.2 can also update automatically.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.3/docs/windows-update-acceptance.md).

# 简体中文

Karma v0.3.3 是 Windows x64 **测试预发布版**，为应用管控新增两项便捷功能。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**从事件证据加入白名单**

- “事件证据”页每条证据的删除按钮旁新增 **加入白名单**。确认后，该证据的来源应用会加入“应用管控”列表，处理方式为 **允许使用**：图像与标题识别不再关闭它（网址黑名单仍然生效）。匹配它的“始终禁止”规则也会改为“允许使用”，对它的重复关闭限制同时解除。
- 已设置超级密码时需验证超级密码。该操作立即保存，不会顺带保存其他未保存的修改，并以 `evidence_application_allowed` 写入审计日志。
- 可执行文件路径由 Service 从自身证据记录读取，控制台只发送证据 ID。来源应用无法可靠归属的证据不能加入白名单；应用已允许使用时按钮显示“已在白名单”。

**选择已安装应用**

- “添加应用”弹窗除手动填写可执行文件外，现在还可 **从已安装应用选择**。控制台读取 Windows 已安装程序（Uninstall）与 App Paths 注册信息，支持按名称、发布者或路径搜索。选择后自动填入名称和可执行文件，确认类别后照常保存。
- 未在上述位置注册的 Microsoft Store 应用不会列出，请手动填写其可执行文件。

**升级**

- 请使用 `Karma-windows-x64-test-v0.3.3-setup.exe` 并输入管理员密码。升级期间会关闭控制台，完成后重新打开即可。默认保留现有本地策略和证据数据。v0.3.2 也可通过自动更新升级。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.3/docs/windows-update-acceptance.md)中的项目。
