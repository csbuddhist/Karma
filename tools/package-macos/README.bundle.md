# Karma macOS 测试包

这是未签名的开发测试包，不是正式发布版。它包含 `KarmaService`（LaunchDaemon）、`KarmaControl`、`KarmaAgent.app`（每个图形会话一个的 LaunchAgent）、管理控制台 `Karma Family Protection.app`、本地 ONNX/OCR 模型以及安装、卸载脚本。适用于 macOS 13 Ventura 及以上的 Apple Silicon Mac。

This is an unsigned development test bundle, not a release. It contains `KarmaService` (LaunchDaemon), `KarmaControl`, `KarmaAgent.app` (a LaunchAgent in every graphical session), the `Karma Family Protection.app` console, local ONNX/OCR models, and install and uninstall scripts. It runs on Apple Silicon Macs with macOS 13 Ventura or later.

## 安装 / Install

```bash
sudo ./install.sh
```

脚本先校验 `SHA256SUMS`，再把程序复制到 `/Library/Karma`、控制台复制到 `/Applications`，全部归 root 所有，注册并启动 `com.karma.service` 与当前用户的 `com.karma.agent`。之后：

1. 打开 Karma Family Protection，设置至少 10 个字符的管理员密码。
2. 在“系统设置 → 隐私与安全性 → 屏幕与系统录音”中允许 KarmaAgent。未授权时控制台的显示器页面显示“不可用”。测试包为临时签名，每次重新构建后都需要重新授权。

The script verifies `SHA256SUMS`, copies the programs to `/Library/Karma` and the console to `/Applications` (all owned by root), and registers and starts `com.karma.service` and the current user's `com.karma.agent`. Then set an administrator password in the console and allow KarmaAgent under Screen & System Audio Recording. The test bundle is ad-hoc signed, so every rebuild needs the permission again.

## 卸载 / Uninstall

```bash
sudo /Library/Karma/uninstall.sh [--purge-data]
```

脚本安全地读取超级密码（或等待期满后的管理员密码；未设置超级密码时使用管理员密码），通过 `KarmaControl` 请求 KarmaService 授权，授权成功后才移除 launchd 任务和程序。默认保留 `/Library/Application Support/Karma` 中的策略和加密证据；`--purge-data` 会一并删除。

The script reads the super password (or the administrator password after the waiting period, or when no super password is set), asks KarmaService to authorize through `KarmaControl`, and only then removes the launchd jobs and programs. Policy and encrypted evidence in `/Library/Application Support/Karma` are kept unless `--purge-data` is given.

## 尚未实现 / Not yet implemented

- 断网时段只计算和审计，尚未拦截网络（需要 Network Extension）。/ Network schedules are evaluated and audited but not enforced yet (needs a Network Extension).
- 浏览器地址栏域名识别（需要辅助功能权限），因此网站允许 / 阻止名单暂不生效；标题与图像规则照常工作。/ Browser host detection (needs the Accessibility permission), so website rules do not apply yet; title and image rules do.
- 签名更新、公证安装包与 Intel Mac 支持。/ Signed updates, a notarized installer, and Intel Macs.

## 诊断 / Diagnostics

- `sudo launchctl print system/com.karma.service`、`launchctl print gui/$(id -u)/com.karma.agent`：查看任务状态。
- `/Library/Logs/Karma/service.log`：服务日志，只含状态码，不含标题、网址或屏幕内容。
