# English

Karma v0.3.15 is a Windows x64 **prerelease for testing** that closes windows whose titles contain adult video codes (番号) such as `SSIS-123`. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Video codes in window titles**

- The built-in title keywords gain a **Video codes** category with 1,809 code prefixes, such as SSIS, JUY, SNIS, IPX, and MIDE. A title matches when a prefix is followed by 3–4 digits, with or without a hyphen or space in between: `SSIS-123`, `SSIS 123`, `SSIS123`, `SSIS - 123`, and full-width forms all match. `SSIS-12`, `SSIS-12345`, and `XSSIS-123` do not.
- 1,642 prefixes are always active. The 167 that also appear in ordinary titles are off by default and follow the existing “Also use terms prone to false positives” switch. These include two-letter prefixes (DV, BF), months and weekdays (NOV, WED), currencies and technical abbreviations (BTC, DPI, SCP), and common words (STAR, LOVE). JUL and SSIS stay always active, so titles such as “Jul 2024” or “SSIS 2019” (SQL Server Integration Services) are also closed; add an exemption keyword if one of them is a false positive.
- On the Keywords page, administrators can add their own prefixes: choose the Video codes category and enter a prefix of 2–16 letters or digits. The match mode is fixed to Code pattern. The category filter also lists video codes.
- Prefixes were collected from the MDCx, OpenAver, darkeye, webtor, dhtsearch, and dan211 projects. Sources and licenses are listed in `assets/keyword-lists/NOTICE.md`.

**Upgrading**

- v0.3.12 through v0.3.14 can update to this version from the console or automatically. v0.3.11 and earlier must run `Karma-windows-x64-test-v0.3.15-setup.exe` once and enter the administrator password. Existing local policy, evidence, and audit data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Karma remains user-mode protection: a technical Windows administrator can take ownership, use safe mode, or alter the system offline. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the network enforcement acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.15/docs/windows-network-enforcement-acceptance.md).

# 简体中文

Karma v0.3.15 是 Windows x64 **测试预发布版**，新增按成人影片番号（如 `SSIS-123`）识别窗口标题并关闭窗口。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**窗口标题番号识别**

- 内置标题关键词新增 **番号** 类别，收录 1,809 个番号前缀，如 SSIS、JUY、SNIS、IPX、MIDE。标题中出现前缀后接 3–4 位数字即命中，中间可有连字符或空格：`SSIS-123`、`SSIS 123`、`SSIS123`、`SSIS - 123` 以及全角写法均命中；`SSIS-12`、`SSIS-12345`、`XSSIS-123` 不命中。
- 1,642 个前缀始终生效。另有 167 个前缀在普通标题中也常出现，默认不生效，随现有的“同时启用易误报词”开关开启。它们包括两字母前缀（DV、BF）、月份与星期（NOV、WED）、货币与技术缩写（BTC、DPI、SCP）以及普通单词（STAR、LOVE）。JUL 和 SSIS 始终生效，因此“Jul 2024”或“SSIS 2019”（SQL Server Integration Services）这样的标题也会被关闭；如有误报，可添加豁免词。
- 管理员可在“关键词规则”页面自行补充番号：类别选择“番号”，输入 2–16 个字母或数字组成的前缀，匹配方式固定为“番号格式”。类别筛选也可只看番号。
- 番号前缀来自 MDCx、OpenAver、darkeye、webtor、dhtsearch 和 dan211 等项目，来源与许可见 `assets/keyword-lists/NOTICE.md`。

**升级**

- v0.3.12 至 v0.3.14 可通过控制台或自动更新升级到此版本。v0.3.11 及更早版本需先运行一次 `Karma-windows-x64-test-v0.3.15-setup.exe` 并输入管理员密码。默认保留现有本地策略、证据和审计数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。Karma 仍是用户态保护：懂技术的 Windows 管理员可夺取所有权、进入安全模式或离线修改系统。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[网络执行验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.15/docs/windows-network-enforcement-acceptance.md)中的项目。
