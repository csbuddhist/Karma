# English

Karma Android v0.1.1 is a **developer preview** that fixes two setup-checklist buttons reported on a Honor phone (MagicOS 10, Android 16). It installs over v0.1.0 and keeps the administrator password, rules, and activity log.

**Device admin can be enabled from the checklist**

- Tapping "Enable" next to Device admin briefly flashed the system page, which then closed without enabling anything. Android refuses to show the device-admin activation page when it is opened as a separate task, and Karma opened every settings page that way. The checklist now opens settings pages from Karma's own screen, so the activation page stays open.

**The autostart button always opens a page**

- When a phone has an autostart page that third-party apps may not open, "Open" did nothing. Karma now tries each known autostart page in turn, including the newer Honor and Huawei pages, and falls back to App info. If the page that opens is not the autostart page, search system settings for "autostart" or "app launch".

**Installation**

Download `Karma-android-v0.1.1.apk`, check it against `SHA256SUMS-android-v0.1.1.txt`, and open it on the phone. It is signed by the same certificate as v0.1.0 (SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`), so it installs as an update. New installations follow the steps in the [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.1/README.en.md#android-developer-preview).

Real-device testing against [the Android acceptance checklist](https://github.com/csbuddhist/Karma/blob/android-v0.1.1/docs/android-acceptance.md) is still in progress. Keep the administrator password and recovery codes with the supervisor, not with the person using the phone.

# 简体中文

Karma Android v0.1.1 是**开发预览版**，修复了在荣耀手机（MagicOS 10，Android 16）上发现的两个设置向导按钮问题。可直接覆盖安装 v0.1.0，管理员密码、规则和活动日志都会保留。

**可以从设置向导启用设备管理器**

- 点击“设备管理器”旁的“启用”时，系统页面一闪而过就关闭了，设备管理器也没有启用。Android 不允许以独立任务的方式打开设备管理器激活页面，而 Karma 打开所有设置页面时都用了这种方式。现在设置向导从 Karma 自身的界面打开设置页面，激活页面会保持显示。

**“自启动”按钮总能打开页面**

- 如果手机上有自启动页面，但不允许第三方应用打开，此前点击“打开”没有任何反应。现在 Karma 会依次尝试已知的自启动页面（包括荣耀和华为较新的页面），都打不开时改为打开“应用信息”。如果打开的不是自启动页面，请在系统设置中搜索“自启动”或“应用启动管理”。

**安装**

下载 `Karma-android-v0.1.1.apk`，用 `SHA256SUMS-android-v0.1.1.txt` 核对后在手机上打开。本版本与 v0.1.0 使用同一证书签名（SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`），会作为更新安装。全新安装请按 [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.1/README.md#android-版开发预览) 中的步骤操作。

[Android 验收清单](https://github.com/csbuddhist/Karma/blob/android-v0.1.1/docs/android-acceptance.md)中的真机验收仍在进行。管理员密码和恢复码请由监督人保管，不要交给使用这部手机的人。
