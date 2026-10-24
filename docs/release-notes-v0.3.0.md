# English

Karma v0.3.0 is a Windows x64 **prerelease for testing**. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Built-in window-title keywords**

- The built-in list grows from 34 phrases in 4 languages to 2,543 terms in 26 languages. Each term has a category (sexual activity or body parts), a match mode (whole word, contains, or word prefix), and a confidence.
- The 1,592 strict terms are always active. The 951 terms that also appear in ordinary titles (such as “奶” or “adult”) are off by default; Keywords → Built-in keywords has a switch to turn them on.
- Ordinary administrators can add their own terms to the built-in list, choosing a language, category, and match mode. Added terms are always active. Once a super password is set, removing an added term or turning off the false-positive-prone terms requires it.
- The built-in list can be searched and filtered by language, category, and confidence.
- Some strict terms, such as the English whole words “sex” and “anal”, can still match harmless titles. Add an exemption phrase if that happens.

**Event evidence**

- Each saved image now shows why it was saved, with its image risk score, directly below the thumbnail.
- Administrators can delete one evidence item, or select several (or all items shown by the current filter) and delete them together after confirming. The super password is not required. Deleting evidence does not forget remembered titles or relax any rule. A record whose encrypted original is already missing can now be deleted too.

**Remembered titles**

- Content Recognition shows why each title was remembered (image risk score and the threshold at the time; titles from v0.2.6 or earlier show "not recorded").
- A misjudged title can be marked **Misjudged**: it is forgotten and never remembered again. Misjudged titles are listed separately and can be remembered again. Marking a title as misjudged requires the super password when one is set.

Download `Karma-windows-x64-test-v0.3.0-setup.exe` for installation, or the ZIP for diagnostics. Existing v0.2.4–v0.2.6 installations upgrade in place with the administrator password; automatic updates also install this signed, strictly newer version. Existing local policy and evidence data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.0/docs/windows-update-acceptance.md), including live in-place upgrades, title matching with the expanded keyword list, and deleting evidence.

# 简体中文

Karma v0.3.0 是 Windows x64 **测试预发布版**。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**系统内置窗口标题关键词**

- 内置词库从 4 种语言 34 个短语扩展为 26 种语言 2,543 个词。每个词标注类别（性行为或身体部位）、匹配方式（整词、包含或单词前缀）和可信度。
- 1,592 个严格词始终生效。951 个在普通标题中也常出现的易误报词（如“奶”“adult”）默认不生效，可在“关键词 → 系统内置关键词”中开启。
- 普通管理员可向内置词库补充关键词，并选择语言、类别和匹配方式；补充词始终生效。设置超级密码后，删除补充词或关闭易误报词需要超级密码。
- 内置词库可搜索，并可按语言、类别和可信度筛选。
- 部分严格词（如英文整词 “sex”“anal”）仍可能命中普通标题，遇到时可添加豁免词。

**事件证据**

- 每张保存的图片缩略图下方显示保存原因及图像风险分。
- 管理员可删除单条证据，也可勾选多条（或当前筛选结果全部）确认后批量删除，不需要超级密码。删除证据不会删除已记住的标题，也不会放宽任何规则。加密原图已丢失的证据记录现在也能删除。

**已记住的标题**

- “内容识别”页显示每条标题被记住的原因（图像风险分和当时的阈值；v0.2.6 及更早版本记住的标题显示“未记录”）。
- 误判的标题可标记为**误判**：删除该标题且以后不再记住。误判标题单独列出，并可恢复记住。已设置超级密码时，标记误判需要超级密码。

安装请下载 `Karma-windows-x64-test-v0.3.0-setup.exe`，诊断可使用 ZIP。已安装的 v0.2.4–v0.2.6 可输入管理员密码原地升级；自动更新也会安装这一经过签名的更高版本。默认保留现有本地策略和证据数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.0/docs/windows-update-acceptance.md)中的项目，包括运行中原地升级、扩展词库后的标题匹配，以及删除证据。
