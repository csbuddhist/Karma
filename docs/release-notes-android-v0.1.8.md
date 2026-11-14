# English

Karma Android v0.1.8 is a **developer preview** that lets anyone using the phone switch Karma between English and Simplified Chinese. It installs over v0.1.0–v0.1.7 and keeps the administrator password, rules, and activity log.

**Language**

- Status → **Language · 语言** offers **Follow the system**, **English**, and **简体中文**. The first-run setup page shows the same choice.
- No password is needed: the language changes only how Karma is shown, not what it blocks.
- The change applies at once to the app, the status notification, the block notice, and alerts.
- On Android 13 and later the choice is the same setting as App info → Language, so the two always agree. On Android 11 and 12 Karma remembers the choice itself.
- The settings guard recognises Karma in system settings in either language, so protection does not depend on which language is chosen.

**Installation**

Download `Karma-android-v0.1.8.apk`, check it against `SHA256SUMS-android-v0.1.8.txt`, and open it on the phone. It is signed by the same certificate as earlier versions (SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`), so it installs as an update. On v0.1.4 and later, Status → Updates → "Check for updates now" installs it without the administrator password; on v0.1.3, use Administrator → Updates. New installations follow the steps in the [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.8/README.en.md#android-developer-preview).

Verified on an Android 15 emulator: switching to 简体中文, then English, then Follow the system without a password changed the app, the status notification, and the notification channel names at once, matched the system per-app language each time, and left the accessibility service running. The Android 11–12 path has not yet been tried on a device. Real-device testing against [the Android acceptance checklist](https://github.com/csbuddhist/Karma/blob/android-v0.1.8/docs/android-acceptance.md) is still in progress. Keep the administrator password and recovery codes with the supervisor, not with the person using the phone.

# 简体中文

Karma Android v0.1.8 是**开发预览版**，使用手机的人可以自行把 Karma 切换为简体中文或英文。可直接覆盖安装 v0.1.0–v0.1.7，管理员密码、规则和活动日志都会保留。

**语言**

- “状态 → **语言 · Language**”中可选择 **跟随系统**、**English** 和 **简体中文**；首次设置页面也有同样的选项。
- 无需密码：语言只改变 Karma 的显示方式，不影响拦截内容。
- 切换后应用界面、状态通知、拦截提示和警报立即使用所选语言。
- Android 13 及以上，这里的选择与“应用信息 → 语言”是同一项设置，两处始终一致；Android 11 和 12 由 Karma 自行记住所选语言。
- “设置看门”在两种语言下都能识别系统设置中的 Karma，保护不受所选语言影响。

**安装**

下载 `Karma-android-v0.1.8.apk`，用 `SHA256SUMS-android-v0.1.8.txt` 核对后在手机上打开。本版本与此前版本使用同一证书签名（SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`），会作为更新安装。v0.1.4 及以上可在“状态 → 更新”中点击“立即检查更新”安装，无需管理员密码；v0.1.3 请在“管理员 → 更新”中检查。全新安装请按 [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.8/README.md#android-版开发预览) 中的步骤操作。

已在 Android 15 模拟器上验证：不输入密码依次切换为简体中文、English、跟随系统，应用界面、状态通知和通知渠道名称立即随之改变，每次都与系统的应用语言设置一致，无障碍服务保持运行。Android 11–12 的实现尚未在设备上验证。[Android 验收清单](https://github.com/csbuddhist/Karma/blob/android-v0.1.8/docs/android-acceptance.md)中的真机验收仍在进行。管理员密码和恢复码请由监督人保管，不要交给使用这部手机的人。
