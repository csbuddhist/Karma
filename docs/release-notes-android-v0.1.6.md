# English

Karma Android v0.1.6 is a **developer preview** that makes the website blocklist and allowlist clearer and asks for the new administrator password twice. It installs over v0.1.0–v0.1.5 and keeps the administrator password, rules, and activity log.

**Website blocklist and allowlist**

- Administrator → Websites is now **Website blocklist and allowlist**, with two separate lists, each with its own field and button:
  - **Blocklist · closed as soon as it opens**: a listed site is closed as soon as it opens in a browser.
  - **Allowlist · never closed by image or title checks**: a listed site is allowed.
- A rule also covers its subdomains. A full address such as `https://www.example.com/page` can be pasted; only the domain is kept. Adding a site to the other list moves it there.
- Existing website rules appear in the matching list; nothing needs to be set up again.

**Confirm the new password**

- Administrator → Password now has a **Confirm password** field. "Change password" stays disabled until both entries match, and "The passwords do not match." appears while they differ.

**Installation**

Download `Karma-android-v0.1.6.apk`, check it against `SHA256SUMS-android-v0.1.6.txt`, and open it on the phone. It is signed by the same certificate as earlier versions (SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`), so it installs as an update. On v0.1.4 and later, Status → Updates → "Check for updates now" installs it without the administrator password; on v0.1.3, use Administrator → Updates. New installations follow the steps in the [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.6/README.en.md#android-developer-preview).

Verified on an Android 15 emulator: a mismatched confirmation kept "Change password" disabled, a matching one changed the password and the new password unlocked the administrator area; `example.com` added to the blocklist was closed in Chrome with "This website is blocked", and `https://www.wikipedia.org/wiki` was added to the allowlist as `www.wikipedia.org`. Real-device testing against [the Android acceptance checklist](https://github.com/csbuddhist/Karma/blob/android-v0.1.6/docs/android-acceptance.md) is still in progress. Keep the administrator password and recovery codes with the supervisor, not with the person using the phone.

# 简体中文

Karma Android v0.1.6 是**开发预览版**，网站黑白名单更清晰，修改管理员密码时需要输入两次确认。可直接覆盖安装 v0.1.0–v0.1.5，管理员密码、规则和活动日志都会保留。

**网站黑白名单**

- “管理员 → 网站”改为 **网站黑白名单**，分成两个名单，各有自己的输入框和按钮：
  - **黑名单 · 打开即关闭**：名单内的网站在浏览器中一打开就会被关闭。
  - **白名单 · 放行，不会因图像或标题被关闭**：名单内的网站放行。
- 规则同时适用于子域名。可以直接粘贴完整网址（如 `https://www.example.com/page`），只保留域名。把网站加入另一个名单会将它从原名单移出。
- 已有的网站规则会自动出现在对应名单中，无需重新设置。

**确认新密码**

- “管理员 → 密码”新增 **确认密码** 输入框。两次输入一致前“修改密码”按钮不可用，输入不一致时提示“两次输入的密码不一致。”

**安装**

下载 `Karma-android-v0.1.6.apk`，用 `SHA256SUMS-android-v0.1.6.txt` 核对后在手机上打开。本版本与此前版本使用同一证书签名（SHA-256 `2b6e9836d496091e8607bbabf6b4f7dda856e656cddefa5f6d6adb1936129bd7`），会作为更新安装。v0.1.4 及以上可在“状态 → 更新”中点击“立即检查更新”安装，无需管理员密码；v0.1.3 请在“管理员 → 更新”中检查。全新安装请按 [README](https://github.com/csbuddhist/Karma/blob/android-v0.1.6/README.md#android-版开发预览) 中的步骤操作。

已在 Android 15 模拟器上验证：确认密码不一致时“修改密码”不可用，一致时修改成功，并可用新密码解锁管理员页面；加入黑名单的 `example.com` 在 Chrome 中被关闭并提示“此网站已被禁止”，`https://www.wikipedia.org/wiki` 以 `www.wikipedia.org` 加入白名单。[Android 验收清单](https://github.com/csbuddhist/Karma/blob/android-v0.1.6/docs/android-acceptance.md)中的真机验收仍在进行。管理员密码和恢复码请由监督人保管，不要交给使用这部手机的人。
