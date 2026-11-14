# English

Karma v0.4.3 is a Windows x64 **prerelease for testing** that adds Time Zone Settings. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Time Zone Settings**

- A new **Time Zone Settings** item in the left sidebar shows two time zones side by side:
  - **System time zone**: read from the current Windows setting (for example UTC−05:00, i.e. −5), for reference only.
  - **Real-life time zone**: the time zone where the family actually lives, set by the super administrator. Any offset in civil use from UTC−12:00 to UTC+14:00 can be chosen, including +5:30 and +5:45.
- Once the real-life time zone is set, **every time setting follows it**: blocking and launch-blocking schedules are evaluated in the real-life time zone, and the console shows audit, evidence, ban, uninstall-window, and update times in it. Until it is set, the system time zone is used, and the page reminds you to set it.
- The page shows the current time in both zones, and warns when the system time zone differs from the real-life time zone. The Blocking Schedules page notes the time zone its blocks follow.
- Setting, changing, or clearing the real-life time zone moves when every schedule applies, so once a super password is configured, saving such a change requires it. Each change is recorded in the audit log as "Real-life time zone changed" with the offsets before and after.
- The real-life time zone is a fixed UTC offset; in regions with daylight saving time, adjust it when the clocks change.

**Upgrading**

- v0.3.12 through v0.4.2 can update to this version from the console or automatically. v0.3.11 and earlier must run `Karma-windows-x64-test-v0.4.3-setup.exe` once and enter the administrator password. Existing local policy, evidence, and audit data are retained. After updating, open Time Zone Settings and set the real-life time zone.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the update acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.4.3/docs/windows-update-acceptance.md) and [the network enforcement acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.4.3/docs/windows-network-enforcement-acceptance.md).

# 简体中文

Karma v0.4.3 是 Windows x64 **测试预发布版**，新增“时区设置”。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**时区设置**

- 左侧导航新增 **时区设置**，并列显示两种时区：
  - **系统时区**：读取自当前 Windows 设置（例如 UTC−05:00，即 −5），仅供参考。
  - **现实时区**：家庭实际所在地的时区，由超级管理员设置。可选择 UTC−12:00 至 UTC+14:00 之间所有通行的时区，包括 +5:30、+5:45 等。
- 设置现实时区后，**所有时间设置都以现实时区为准**：断网屏蔽与禁止启动时段按现实时区计算，控制台中的审计、证据、封禁、卸载窗口和更新时间也按现实时区显示。尚未设置时暂按系统时区计算，页面会提示尽快设置。
- 页面同时显示两个时区的当前时间；系统时区与现实时区不同时会给出提示。“断网屏蔽”页也会注明时段所依据的时区。
- 设置、修改或清除现实时区会改变所有时段的实际生效时间，因此设置超级密码后，保存此类修改需要验证超级密码。每次修改都会写入审计日志“修改现实时区”，并记录修改前后的时区。
- 现实时区为固定的 UTC 偏移；实行夏令时的地区请在时令切换时手动调整。

**升级**

- v0.3.12 至 v0.4.2 可通过控制台或自动更新升级到此版本。v0.3.11 及更早版本需先运行一次 `Karma-windows-x64-test-v0.4.3-setup.exe` 并输入管理员密码。默认保留现有本地策略、证据和审计数据。升级后请打开“时区设置”设置现实时区。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成 [升级验收清单](https://github.com/csbuddhist/Karma/blob/v0.4.3/docs/windows-update-acceptance.md) 和 [网络执行验收清单](https://github.com/csbuddhist/Karma/blob/v0.4.3/docs/windows-network-enforcement-acceptance.md)中的项目。
