# English

Karma Android v0.1.3 is a **developer preview** that catches explicit page titles and videos in small players that earlier versions missed, adds automatic updates, and uses the desktop logo. It installs over v0.1.0–v0.1.2 and keeps the administrator password, rules, and activity log.

**Page titles are checked in browsers**

- Chrome reports only its own name as the window title, so title keywords were never compared with the page. Karma now reads the page title from the browser, so a keyword anywhere in the title closes the page.
- Traditional Chinese titles (such as `內射` or `性愛`) now match the built-in keywords; titles and keywords are folded to Simplified Chinese before matching (OpenCC data, Apache-2.0). The desktop applies the same folding.

**Videos in small players are recognized**

- A video in a web page's own player covers only part of the screen, so it was recognized only after switching to full screen. As on the desktop, Karma now finds the part of the screen that keeps changing, crops it from the full-resolution screenshot, and checks it on its own. While the screen keeps changing it is checked once a second instead of pausing after 30 seconds.
- Verified on an Android 15 emulator at 1080×2400: explicit videos in a page's small player were closed within a few seconds, and an ordinary video playing in a small player was not.

**Automatic updates**

- Twice a day on Wi-Fi, Karma checks GitHub and its Gitee mirror for a newer APK, downloads it, and installs it only when its checksum matches the release's `SHA256SUMS` file and it is signed by the same certificate as the installed app. Updates can be turned off or checked at once under Administrator → Updates, where Karma must also be allowed to install apps.
- The first update shows the system confirmation; later updates on Android 12 and later, and every update under Device Owner, install without asking. The settings guard lets the installer through for 10 minutes after Karma starts a verified update.
- This adds the `INTERNET` and `REQUEST_INSTALL_PACKAGES` permissions. Update checks are the only network access; nothing is uploaded.

**New icon**

- The app and notification icons now use the desktop Karma logo.

**Installation**

Download `Karma-android-v0.1.3.apk`, check it against `SHA256SUMS-android-v0.1.3.txt`, and open it on the phone. It is signed by the same certificate as earlier versions (SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`), so it installs as an update. New installations follow the steps in the [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.3/README.en.md#android-developer-preview). From this version on, later releases can be installed from Administrator → Updates.

Real-device testing against [the Android acceptance checklist](https://github.com/csbuddhist/Karma/blob/android-v0.1.3/docs/android-acceptance.md) is still in progress. Keep the administrator password and recovery codes with the supervisor, not with the person using the phone.

# 简体中文

Karma Android v0.1.3 是**开发预览版**，能识别此前漏掉的网页标题关键词和小窗播放的色情视频，新增自动更新，并改用桌面版图标。可直接覆盖安装 v0.1.0–v0.1.2，管理员密码、规则和活动日志都会保留。

**浏览器中检查网页标题**

- Chrome 的窗口标题只有浏览器自身名称，因此此前标题关键词从未与网页标题比对。现在 Karma 从浏览器读取网页标题，标题中任意位置出现关键词都会关闭页面。
- 繁体标题（如 `內射`、`性愛`）现在也能命中内置关键词：匹配前把标题和关键词统一折算为简体（OpenCC 数据，Apache-2.0）。桌面端同样适用。

**识别小窗播放的视频**

- 网页内播放器中的视频只占屏幕一部分，此前要切换到全屏才会被识别。现在与桌面端一样，Karma 会找出屏幕上持续变化的区域，从全分辨率截图中裁出单独识别；画面持续变化时每秒检查一次，不再在 30 秒后暂停。
- 已在 1080×2400 的 Android 15 模拟器上验证：网页小窗播放的色情视频在数秒内被关闭，小窗播放的普通视频不受影响。

**自动更新**

- Karma 每天两次在 Wi-Fi 下检查 GitHub 及其 Gitee 镜像，下载更新的 APK；只有校验和与发布页的 `SHA256SUMS` 一致、且与已安装版本使用同一证书签名时才安装。可在“管理员 → 更新”中关闭自动更新或立即检查，并需在该处允许 Karma 安装应用。
- 首次更新会弹出系统确认；之后在 Android 12 及以上，以及设备所有者模式下，更新无需确认。Karma 开始安装已校验的更新后 10 分钟内，“设置看门”放行安装器窗口。
- 本版本新增 `INTERNET` 和 `REQUEST_INSTALL_PACKAGES` 权限。检查更新是唯一的联网操作，不上传任何内容。

**新图标**

- 应用图标和通知图标改用桌面版 Karma 标志。

**安装**

下载 `Karma-android-v0.1.3.apk`，用 `SHA256SUMS-android-v0.1.3.txt` 核对后在手机上打开。本版本与此前版本使用同一证书签名（SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`），会作为更新安装。全新安装请按 [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.3/README.md#android-版开发预览) 中的步骤操作。从本版本起，之后的版本可在“管理员 → 更新”中安装。

[Android 验收清单](https://github.com/csbuddhist/Karma/blob/android-v0.1.3/docs/android-acceptance.md)中的真机验收仍在进行。管理员密码和恢复码请由监督人保管，不要交给使用这部手机的人。
