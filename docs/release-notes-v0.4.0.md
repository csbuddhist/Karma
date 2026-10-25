# English

Karma v0.4.0 is a Windows x64 **prerelease for testing** that makes the repeat-closure restriction length configurable and raises its default to two hours. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Configurable repeat-closure restriction length**

- The repeat-closure limit card on the Application Control page has a new **Restriction length (minutes)** field next to the counting window and closure count. It accepts 1–10080 minutes (up to 7 days).
- The default restriction rises from one hour to **two hours**. Policies saved by earlier versions have no restriction length and use the new two-hour default.
- Lengthening the restriction only needs an ordinary administrator session. When a super password is set, shortening it is treated as weakening protection and requires the super password, like shortening the counting window or raising the closure count.
- A new length applies to restrictions established afterwards; applications already restricted keep their original end time.

**Upgrading**

- v0.3.12 through v0.3.21 can update to this version from the console or automatically. v0.3.11 and earlier must run `Karma-windows-x64-test-v0.4.0-setup.exe` once and enter the administrator password. Existing local policy, evidence, and audit data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the ONNX inference acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.4.0/docs/windows-onnx-acceptance.md).

# 简体中文

Karma v0.4.0 是 Windows x64 **测试预发布版**，重复关闭限制的封禁时长现在可以设置，默认值提高到 2 小时。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**可设置重复关闭限制时长**

- “应用管控”页的重复关闭限制卡片在“统计时间”和“关闭次数”旁新增 **限制时长（分钟）** 输入框，可填写 1–10080 分钟（最长 7 天）。
- 默认限制时长由 1 小时提高到 **2 小时**。早期版本保存的策略没有限制时长，按新的 2 小时默认值处理。
- 延长限制时长只需普通管理员会话。已设置超级密码时，缩短限制时长属于削弱保护，需要超级密码，与缩短统计时间或提高关闭次数相同。
- 新时长只对之后触发的限制生效；已被限制的应用仍按原结束时间解除。

**升级**

- v0.3.12 至 v0.3.21 可通过控制台或自动更新升级到此版本。v0.3.11 及更早版本需先运行一次 `Karma-windows-x64-test-v0.4.0-setup.exe` 并输入管理员密码。默认保留现有本地策略、证据和审计数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成 [ONNX 推理验收清单](https://github.com/csbuddhist/Karma/blob/v0.4.0/docs/windows-onnx-acceptance.md)中的项目。
