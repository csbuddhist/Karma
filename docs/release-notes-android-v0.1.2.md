# English

Karma Android v0.1.2 is a **developer preview** that closes Chrome Incognito tabs as intended. It installs over v0.1.0 and v0.1.1 and keeps the administrator password, rules, and activity log.

**Private browser windows are closed again**

- The "Close browser windows that block screenshots (private tabs)" setting never took effect: Android does not refuse to capture such windows, it draws them black, so Karma saw a black screen and allowed it. Karma now recognizes a blacked-out browser frame and checks it again. Android 14 and later confirm it through a screenshot of the window itself, which reports a private window; Android 11 to 13 require three such frames in a row, so a black scene in a video does not close the browser.
- Verified on an Android 15 emulator: a Chrome Incognito tab is closed within a few seconds with the reason "A private browser window cannot be checked", and ordinary tabs stay open.

**Monitoring starts with the app already in front**

- When the accessibility service started or restarted while a browser or another app was in front, checks only began after switching to another window. They now begin immediately.

**Installation**

Download `Karma-android-v0.1.2.apk`, check it against `SHA256SUMS-android-v0.1.2.txt`, and open it on the phone. It is signed by the same certificate as earlier versions (SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`), so it installs as an update. New installations follow the steps in the [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.2/README.en.md#android-developer-preview).

Real-device testing against [the Android acceptance checklist](https://github.com/csbuddhist/Karma/blob/android-v0.1.2/docs/android-acceptance.md) is still in progress. Keep the administrator password and recovery codes with the supervisor, not with the person using the phone.

# 简体中文

Karma Android v0.1.2 是**开发预览版**，修复后 Chrome 无痕标签页会按设置被关闭。可直接覆盖安装 v0.1.0 和 v0.1.1，管理员密码、规则和活动日志都会保留。

**浏览器隐私窗口会被关闭**

- 此前“关闭禁止截图的浏览器窗口（无痕标签页）”这一设置从未生效：Android 并不会拒绝截取这类窗口，而是把它画成黑色，Karma 看到的是一片黑屏，于是放行。现在 Karma 会识别被涂黑的浏览器画面并再次确认：Android 14 及以上对该窗口单独截图，系统会明确报告这是隐私窗口；Android 11 至 13 要求连续三帧如此，避免视频中的黑场导致浏览器被关闭。
- 已在 Android 15 模拟器上验证：Chrome 无痕标签页会在几秒内被关闭，原因显示为“无法检查浏览器隐私窗口”，普通标签页不受影响。

**应用已在前台时也会立即开始监控**

- 无障碍服务启动或重启时，如果浏览器或其他应用已在前台，此前要等切换窗口后才开始检查，现在会立即开始。

**安装**

下载 `Karma-android-v0.1.2.apk`，用 `SHA256SUMS-android-v0.1.2.txt` 核对后在手机上打开。本版本与此前版本使用同一证书签名（SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`），会作为更新安装。全新安装请按 [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.2/README.md#android-版开发预览) 中的步骤操作。

[Android 验收清单](https://github.com/csbuddhist/Karma/blob/android-v0.1.2/docs/android-acceptance.md)中的真机验收仍在进行。管理员密码和恢复码请由监督人保管，不要交给使用这部手机的人。
