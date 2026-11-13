# English

Karma Android v0.1.7 is a **developer preview** that adds an encrypted settings backup and makes the block notice stay for a moment after "OK" is tapped. It installs over v0.1.0–v0.1.6 and keeps the administrator password, rules, and activity log.

**Settings backup**

- Status → **Settings backup** has **Export settings** and **Import settings**. Neither asks for a password, so the person using the phone can make and restore a backup.
- The exported file (`karma-settings-<date>.karma`) is encrypted with a key derived from the administrator password. It cannot be read, and a file that was changed in any way is refused.
- Importing asks for confirmation, then the rules in the file replace the current rules.
- A file exported on another phone or under an earlier administrator password asks for this phone's administrator password, and for the password it was exported with if that is different.
- After installing this update, the administrator needs to sign in once before the first export.
- Every export and import, including refused ones, appears in the activity log.

**Block notice countdown**

- Tapping **OK** on the "This app was closed" notice no longer closes it at once: the button shows "Closing in 10 s", counts down, and the notice closes when it reaches zero. A notice nobody taps closes after 30 seconds.

**Installation**

Download `Karma-android-v0.1.7.apk`, check it against `SHA256SUMS-android-v0.1.7.txt`, and open it on the phone. It is signed by the same certificate as earlier versions (SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`), so it installs as an update. On v0.1.4 and later, Status → Updates → "Check for updates now" installs it without the administrator password; on v0.1.3, use Administrator → Updates. New installations follow the steps in the [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.7/README.en.md#android-developer-preview).

Verified on an Android 15 emulator: export before the administrator signed in after the update was refused with a clear message; after sign-in the exported file held no readable rules; a file with one changed byte was refused; importing the export restored a removed rule; after the password changed, the old file asked for the administrator, was refused with only the new password, and imported with the password it was exported with; tapping OK on the notice counted down from 10 s before it closed. Real-device testing against [the Android acceptance checklist](https://github.com/csbuddhist/Karma/blob/android-v0.1.7/docs/android-acceptance.md) is still in progress. Keep the administrator password and recovery codes with the supervisor, not with the person using the phone.

# 简体中文

Karma Android v0.1.7 是**开发预览版**，新增加密的设置备份，并让拦截提示在点“知道了”后停留片刻再消失。可直接覆盖安装 v0.1.0–v0.1.6，管理员密码、规则和活动日志都会保留。

**设置备份**

- “状态 → **设置备份**”中有 **导出设置** 和 **导入设置**，都无需输入密码，使用手机的人也可以自行备份和恢复。
- 导出的文件（`karma-settings-<日期>.karma`）用管理员密码派生的密钥加密，无法查看；文件被改动任何内容都会被拒绝导入。
- 导入前会要求确认，之后文件中的规则替换当前规则。
- 在其他手机上或用旧管理员密码导出的文件，导入时需要输入本机管理员密码；导出时的密码不同时，还需填写导出时的密码。
- 安装本更新后，管理员需要先登录一次，才能进行第一次导出。
- 每次导出和导入（包括被拒绝的）都会记录在活动日志中。

**拦截提示倒计时**

- 在“此应用已被关闭”提示上点 **知道了** 后不再立即消失：按钮显示“10 秒后关闭”并倒计时，归零后提示关闭。无人点击时，提示 30 秒后自动关闭。

**安装**

下载 `Karma-android-v0.1.7.apk`，用 `SHA256SUMS-android-v0.1.7.txt` 核对后在手机上打开。本版本与此前版本使用同一证书签名（SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`），会作为更新安装。v0.1.4 及以上可在“状态 → 更新”中点击“立即检查更新”安装，无需管理员密码；v0.1.3 请在“管理员 → 更新”中检查。全新安装请按 [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.7/README.md#android-版开发预览) 中的步骤操作。

已在 Android 15 模拟器上验证：更新后管理员登录前导出会被拒绝并给出提示；登录后导出的文件中没有可读的规则；改动一个字节的文件被拒绝导入；导入导出文件后恢复了已删除的规则；修改密码后导入旧文件会要求管理员，只填新密码被拒绝，填入导出时的密码后导入成功；点提示上的“知道了”后从 10 秒倒计时再关闭。[Android 验收清单](https://github.com/csbuddhist/Karma/blob/android-v0.1.7/docs/android-acceptance.md)中的真机验收仍在进行。管理员密码和恢复码请由监督人保管，不要交给使用这部手机的人。
