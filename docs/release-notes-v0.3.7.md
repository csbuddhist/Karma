# English

Karma v0.3.7 is a Windows x64 **prerelease for testing** that keeps browsers out of every quick way to allow an application. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Browsers cannot be allowed with one click or in bulk**

- Allowing a browser would exempt every page it shows from image and title enforcement. A browser is one Karma knows (such as Chrome, Edge, Firefox, 360, QQ Browser, or Quark) or an enabled application rule whose category is **Browser**.
- **Application control → Currently restricted applications:** **Always allow** is grayed out for a restricted browser, with an explanation on hover. The Service also refuses the request. The restriction ends on its own when it expires; other restricted applications can still be allowed as before.
- **Bulk add applications:** browsers are marked and their checkboxes are disabled, so they cannot be checked individually or picked by **Select all shown**.
- Together with v0.3.6, which did the same for **Add to allowlist** on the Event evidence page, none of the quick allow actions can allow a browser. Existing rules are not changed.

**Upgrading**

- Use `Karma-windows-x64-test-v0.3.7-setup.exe` and enter the administrator password. The console is closed during the upgrade; reopen it afterwards. Existing local policy and evidence data are retained. Installations on v0.3.2 or later can also update automatically.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Karma remains user-mode protection: a technical Windows administrator can take ownership, use safe mode, or alter the system offline. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.7/docs/windows-update-acceptance.md), including that a restricted browser cannot be allowed with one click and that bulk add does not let browsers be checked.

# 简体中文

Karma v0.3.7 是 Windows x64 **测试预发布版**，所有快捷允许操作都不能再允许浏览器。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**浏览器不能一键允许或批量添加**

- 允许浏览器后，其中显示的所有网页都不再被图像与标题识别处置。浏览器指 Karma 已知的浏览器（如 Chrome、Edge、Firefox、360、QQ 浏览器、夸克等），或类别为 **浏览器** 且已启用的应用规则。
- **应用管控 → 当前被限制的应用：** 被限制的浏览器，其 **一键允许** 按钮显示为灰色，鼠标悬停时说明原因；Service 同样拒绝该请求。限制到期后会自动解除；其他被限制的应用仍可照常一键允许。
- **批量添加应用：** 浏览器会被标出且复选框不可用，既不能单独勾选，也不会被 **全选当前结果** 选中。
- 加上 v0.3.6 对“事件证据”页 **加入白名单** 的同样限制，所有快捷允许操作都不能允许浏览器。已有规则保持不变。

**升级**

- 请使用 `Karma-windows-x64-test-v0.3.7-setup.exe` 并输入管理员密码。升级期间会关闭控制台，完成后重新打开即可。默认保留现有本地策略和证据数据。v0.3.2 及以上版本也可通过自动更新升级。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。Karma 仍是用户态保护：懂技术的 Windows 管理员可夺取所有权、进入安全模式或离线修改系统。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.7/docs/windows-update-acceptance.md)中的项目，包括确认被限制的浏览器不能一键允许，以及批量添加时浏览器不能勾选。
