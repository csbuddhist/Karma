# English

Karma Android v0.1.0 is the first **developer preview** of the Android client. It has been verified on an Android 15 emulator; real-device testing against [the Android acceptance checklist](https://github.com/csbuddhist/Karma/blob/android-v0.1.0/docs/android-acceptance.md) is still pending, so expect rough edges on vendor ROMs.

**What it does**

- Checks the screen on the phone itself through the accessibility service (at most 2 frames per second, slower on still screens) with the same viddexa-nano model and multi-frame rules as the Windows version. Screenshots are scaled in memory only; they are never saved or uploaded.
- Closes the app that shows explicit images or blocked content: Karma returns to the home screen, shows the reason and the remaining cooldown, and closes the app again at once if it is reopened during the cooldown.
- Applies app allow/block lists, address-bar website allow/block lists, page-title keywords, and time schedules. Private browser windows that refuse screenshots are closed by default.
- Protects its own settings with an administrator password (Argon2id, 30-second lockout after 5 failures) and one-time recovery codes. Device admin prevents uninstalling without the password, and the settings guard asks for the password when system pages that could turn Karma off are opened. Optional Device Owner mode adds stronger uninstall and tamper protection.
- The interface is in English and Simplified Chinese and follows the system language; on Android 13 and later it can also be chosen under App info → Language.

**Verified on the emulator (Android 15, arm64)**

- Signed release build: password setup and recovery codes, model loading, the setup checklist, a blocked website closing Chrome within two seconds, immediate re-closing during the cooldown, and "Prepare uninstall" releasing device admin and opening the uninstall dialog.
- Rust core tests and JVM unit tests pass, and Android lint reports no errors.

**Installation**

1. Download `Karma-android-v0.1.0.apk` and check it against `SHA256SUMS-android-v0.1.0.txt`.
2. Open the APK on the phone (Android 11 or later) and allow your browser or file manager to install unknown apps.
3. Open Karma, set the administrator password, write down the recovery codes, and follow the setup checklist. On Android 13 and later, if the accessibility switch is greyed out, first choose "Allow restricted settings" from the ⋮ menu in App info.
4. Add app and website rules on the Administrator tab, then arm the settings guard.

The APK is signed by the certificate with SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7` (`apksigner verify --print-certs`). Later versions will be signed by the same key and install as updates.

Some Chinese vendor ROMs need autostart allowed and battery optimization turned off by hand. Keep the administrator password and recovery codes with the supervisor, not with the person using the phone.

# 简体中文

Karma Android v0.1.0 是 Android 客户端的首个**开发预览版**。已在 Android 15 模拟器上验证；[Android 验收清单](https://github.com/csbuddhist/Karma/blob/android-v0.1.0/docs/android-acceptance.md)中的真机验收仍待完成，在各厂商 ROM 上可能还有不完善之处。

**功能**

- 通过无障碍服务在手机本地检查屏幕（最高每秒 2 帧，静止画面降频），使用与 Windows 版相同的 viddexa-nano 模型和连续帧规则。截图只在内存中缩放，不保存、不上传。
- 关闭显示色情图片或被禁止内容的应用：返回桌面并显示原因和剩余冷却时间，冷却期内再次打开会被立即关闭。
- 支持应用黑白名单、浏览器地址栏网址黑白名单、页面标题关键词和按时段禁用；无法截图的浏览器隐私窗口默认直接关闭。
- 用管理员密码（Argon2id，5 次错误锁定 30 秒）和一次性恢复码保护自身设置。设备管理器使没有密码时无法卸载，“设置看门”在打开可能关闭 Karma 的系统页面时要求输入密码。可选的设备所有者模式提供更强的防卸载与防篡改保护。
- 界面支持简体中文和英文，跟随系统语言；Android 13 及以上也可在“应用信息 → 语言”中单独设置。

**模拟器验证（Android 15，arm64）**

- 签名发布版：设置密码和恢复码、加载模型、设置向导、被禁网址在 2 秒内关闭 Chrome、冷却期内再次打开立即关闭，以及“准备卸载”解除设备管理器并打开卸载对话框。
- Rust 核心测试和 JVM 单元测试全部通过，Android lint 无错误。

**安装**

1. 下载 `Karma-android-v0.1.0.apk`，用 `SHA256SUMS-android-v0.1.0.txt` 核对。
2. 在手机（Android 11 及以上）上打开 APK，允许浏览器或文件管理器“安装未知应用”。
3. 打开 Karma，设置管理员密码并抄下恢复码，然后按“设置向导”完成设置。Android 13 及以上如果无障碍开关是灰色的，先在“应用信息”右上角菜单中选择“允许受限设置”。
4. 在“管理员”页添加应用和网址规则，最后启用“设置看门”。

APK 由 SHA-256 为 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7` 的证书签名（可用 `apksigner verify --print-certs` 查看）。后续版本使用同一密钥签名，可直接覆盖升级。

部分国产 ROM 需要手动允许自启动并关闭电池优化。管理员密码和恢复码请由监督人保管，不要交给使用这部手机的人。
