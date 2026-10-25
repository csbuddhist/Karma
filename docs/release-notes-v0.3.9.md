# English

Karma v0.3.9 is a Windows x64 **prerelease for testing** that reduces image false positives on dark, text-heavy pages. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Fewer false positives on dark text pages**

- Dark, text-heavy screen UI such as GitHub README pages could be misread by the image model as drawn explicit content ("hentai"); one dark GitHub README scored 86.5% and crossed the default 82% threshold, closing the browser.
- The `hentai` probability now counts fully only when it is at least 0.95, or when `porn` is at least as likely; otherwise it counts at half weight. The same page now scores about 43%.
- Photographic explicit (`porn`) and suggestive (`sexy`) scoring is unchanged. Drawn explicit content the model is less than 95% sure of may no longer be enforced on its own.

**Upgrading**

- Use `Karma-windows-x64-test-v0.3.9-setup.exe` and enter the administrator password. The console is closed during the upgrade; reopen it afterwards. Existing local policy and evidence data are retained. Installations on v0.3.2 or later can also update automatically.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.9/docs/windows-update-acceptance.md), including that a dark-theme GitHub README page is no longer closed.

# 简体中文

Karma v0.3.9 是 Windows x64 **测试预发布版**，减少深色、以文字为主的页面被图像识别误判。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**深色文字页面误报减少**

- GitHub README 等深色、以文字为主的屏幕界面，可能被图像模型误判为绘画类色情内容（hentai）；某个深色 GitHub README 页面得分 86.5%，超过默认阈值 82%，导致浏览器被关闭。
- 现在仅当 `hentai` 概率不低于 0.95，或 `porn` 概率不低于 `hentai` 时，`hentai` 才全额计分，否则按半权重计分。同一页面现在得分约 43%。
- 真人露骨内容（`porn`）和性感暗示（`sexy`）的计分不变。模型把握低于 95% 的绘画类色情内容，可能不再单独触发处置。

**升级**

- 请使用 `Karma-windows-x64-test-v0.3.9-setup.exe` 并输入管理员密码。升级期间会关闭控制台，完成后重新打开即可。默认保留现有本地策略和证据数据。v0.3.2 及以上版本也可通过自动更新升级。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.9/docs/windows-update-acceptance.md)中的项目，包括确认深色主题的 GitHub README 页面不再被关闭。
