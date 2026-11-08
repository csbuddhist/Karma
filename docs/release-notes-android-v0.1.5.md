# English

Karma Android v0.1.5 is a **developer preview** that lets the administrator set daily allowed or blocked times for single apps, to the minute. It installs over v0.1.0–v0.1.4 and keeps the administrator password, rules, and activity log.

**Daily times per app**

- Administrator → Add an app rule now has **Times** next to Block and Allow. Choose "Allow only during these times each day" or "Block during these times each day", then add up to eight time ranges with the system time picker. Times are exact to the minute; a range whose end is earlier than its start continues past midnight (for example 22:00–07:00).
- Outside its allowed times, or inside its blocked times, the app is closed like a blocked app. A blocked time also applies to an app that is otherwise always allowed.
- An app left open is closed within about 15 seconds of its blocked time starting, even without any tap or window change, and it can be opened again from the minute its blocked time ends.
- Rules appear in the Apps list with their times and can be edited or removed. One app has either an always-on rule or a time rule; saving one replaces the other.
- Verified on an Android 15 emulator: Calendar set to "blocked 10:16–10:18" was closed on its own between 10:16:04 and 10:16:14 and stayed usable after 10:18.

**Fix**

- After an update finished, the Updates section kept saying "installing version …" until the next check. It now says "updated to version …".

**Installation**

Download `Karma-android-v0.1.5.apk`, check it against `SHA256SUMS-android-v0.1.5.txt`, and open it on the phone. It is signed by the same certificate as earlier versions (SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`), so it installs as an update. On v0.1.4, Status → Updates → "Check for updates now" installs it without the administrator password; on v0.1.3, use Administrator → Updates. New installations follow the steps in the [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.5/README.en.md#android-developer-preview).

Real-device testing against [the Android acceptance checklist](https://github.com/csbuddhist/Karma/blob/android-v0.1.5/docs/android-acceptance.md) is still in progress. Keep the administrator password and recovery codes with the supervisor, not with the person using the phone.

# 简体中文

Karma Android v0.1.5 是**开发预览版**，管理员可以为单个应用设置每天允许或禁止使用的时间段，精确到分钟。可直接覆盖安装 v0.1.0–v0.1.4，管理员密码、规则和活动日志都会保留。

**单个应用的每日时段**

- “管理员 → 添加应用规则”中，“禁止”“允许”旁新增 **时段**。选择“每天仅在以下时段允许使用”或“每天在以下时段禁止使用”，再用系统时间选择器添加最多 8 个时间段。时间精确到分钟；结束时间早于开始时间表示跨越午夜（例如 22:00–07:00）。
- 不在允许时段内、或处于禁止时段内时，应用会像被禁止的应用一样被关闭。禁止时段对设为“始终允许”的应用同样生效。
- 应用一直开着、即使没有任何操作，也会在禁止时段开始后约 15 秒内被关闭；从禁止时段结束的那一分钟起可以再次打开。
- 规则连同时段显示在“应用”列表中，可编辑或移除。一个应用只能有一条规则：保存时段规则会替换原有的禁止/允许规则，反之亦然。
- 已在 Android 15 模拟器上验证：将“日历”设为“每天 10:16–10:18 禁止”后，它在 10:16:04 至 10:16:14 之间被自动关闭，10:18 之后可正常使用。

**修复**

- 更新完成后，“更新”区仍显示“正在安装版本 …”，直到下一次检查。现在会显示“已更新到版本 …”。

**安装**

下载 `Karma-android-v0.1.5.apk`，用 `SHA256SUMS-android-v0.1.5.txt` 核对后在手机上打开。本版本与此前版本使用同一证书签名（SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`），会作为更新安装。v0.1.4 可在“状态 → 更新”中点击“立即检查更新”安装，无需管理员密码；v0.1.3 请在“管理员 → 更新”中检查。全新安装请按 [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.5/README.md#android-版开发预览) 中的步骤操作。

[Android 验收清单](https://github.com/csbuddhist/Karma/blob/android-v0.1.5/docs/android-acceptance.md)中的真机验收仍在进行。管理员密码和恢复码请由监督人保管，不要交给使用这部手机的人。
