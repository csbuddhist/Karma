# English

Karma Android v0.1.4 is a **developer preview** that lets the person using the phone install updates without the administrator password. It installs over v0.1.0–v0.1.3 and keeps the administrator password, rules, and activity log.

**Updates from the status screen**

- The installed version, the result of the last check, and "Check for updates now" moved from Administrator → Updates to a new Updates section on the Status tab. Anyone can start a check there; no password is needed.
- Only a higher version is ever installed: the release must be newer than the installed version, sit at Karma's fixed GitHub or Gitee download address, match the release's `SHA256SUMS` file, and be signed by the same certificate as the installed app. An older or differently signed APK is never installed this way.
- Turning automatic updates off and allowing Karma to install apps stay under Administrator → Updates. Until the administrator allows installs, the status screen says so.
- Tapping "Check for updates now" again while a check is running no longer restarts the download.

**Installation**

Download `Karma-android-v0.1.4.apk`, check it against `SHA256SUMS-android-v0.1.4.txt`, and open it on the phone. It is signed by the same certificate as earlier versions (SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`), so it installs as an update. Phones running v0.1.3 with automatic updates on and installs allowed receive it on Wi-Fi. New installations follow the steps in the [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.4/README.en.md#android-developer-preview).

Real-device testing against [the Android acceptance checklist](https://github.com/csbuddhist/Karma/blob/android-v0.1.4/docs/android-acceptance.md) is still in progress. Keep the administrator password and recovery codes with the supervisor, not with the person using the phone.

# 简体中文

Karma Android v0.1.4 是**开发预览版**，使用这部手机的人无需管理员密码即可安装更新。可直接覆盖安装 v0.1.0–v0.1.3，管理员密码、规则和活动日志都会保留。

**在“状态”页更新**

- 已安装版本、上次检查结果和“立即检查更新”按钮从“管理员 → 更新”移到了“状态”页新增的“更新”区。任何人都可以在这里检查并安装，无需密码。
- 只会安装更高的版本：发布必须比已安装版本新、位于 Karma 固定的 GitHub 或 Gitee 下载地址、与发布页的 `SHA256SUMS` 一致，并且与已安装版本使用同一证书签名。旧版本或签名不同的 APK 不会通过此处安装。
- 关闭自动更新、允许 Karma 安装应用仍在“管理员 → 更新”中操作；管理员尚未允许安装时，“状态”页会给出提示。
- 检查进行中再次点击“立即检查更新”，不会再中断并重新开始下载。

**安装**

下载 `Karma-android-v0.1.4.apk`，用 `SHA256SUMS-android-v0.1.4.txt` 核对后在手机上打开。本版本与此前版本使用同一证书签名（SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`），会作为更新安装。已开启自动更新并允许安装应用的 v0.1.3 会在 Wi-Fi 下自动收到此更新。全新安装请按 [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.4/README.md#android-版开发预览) 中的步骤操作。

[Android 验收清单](https://github.com/csbuddhist/Karma/blob/android-v0.1.4/docs/android-acceptance.md)中的真机验收仍在进行。管理员密码和恢复码请由监督人保管，不要交给使用这部手机的人。
