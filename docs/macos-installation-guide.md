# Karma macOS 安装与测试指南

本文档说明当前未签名 macOS 开发测试包的构建、安装、验证和卸载。macOS 版与 Windows v0.3.8 共用同一个管理控制台、策略格式和 `ServiceCore`（密码、审计、证据加密、重复违规封禁、记住的标题、断网时段计算），由 macOS 原生执行器负责屏幕采集、前台归属、进程处置和后台常驻。设计见 [macOS 版本设计规格](superpowers/specs/2026-10-23-macos-port-design.md)。

> macOS 版本仍在开发中，不适合家庭正式部署。断网时段只计算和审计、尚未拦截网络；尚未读取浏览器地址栏，网址黑白名单暂不生效；没有签名安装包、自动更新和 Intel Mac 支持。拥有本机管理员权限的用户可以停止 LaunchDaemon、撤销屏幕录制权限或删除程序，Karma 无法阻止。

## 1. 当前可测试范围

- `KarmaService`：root LaunchDaemon，开机启动，launchd `KeepAlive` 在异常退出或被终止后自动重启；只有经密码授权的卸载才会正常退出；
- 本地 Unix 域套接字 `/var/run/karma/service.sock`，与 Windows 命名管道使用同一套带长度前缀的协议；客户端拒绝不是 root 运行的服务端；
- Agent 身份由内核记录的对端审计令牌确认：只有安装目录中的 `karma-agent-macos`、且由当前控制台用户运行时才被接受，Agent 不持有任何密钥；
- `KarmaAgent.app`：每个图形会话一个 LaunchAgent，每个活动显示器一个 ScreenCaptureKit 流，显示器接入、断开或采集失败时自动重建；
- 与 Windows 相同的 ONNX 图像识别、PP-OCRv5 OCR、内置词包、证据加密保存、标题关键词、记住的标题与重复违规封禁；
- 帧归属到前台应用中覆盖该显示器面积最大的窗口；无法可靠归属时不处置；
- 处置前复核 PID、启动时间、可执行文件路径和所属用户，`SIGTERM` → 宽限期 → `SIGKILL` 均通过同一进程实例的审计令牌发送；不处置 root、系统进程、其他用户的进程和 Karma 自身；
- 控制台在 macOS 上连接 KarmaService，已安装应用列表来自“应用程序”文件夹；
- Agent 长时间无心跳时，KarmaService 会把 LaunchAgent 重新加载到当前用户的会话中。

## 2. 系统要求

- macOS 13 Ventura 或更高版本，Apple Silicon（arm64）；
- 构建：Xcode 命令行工具、仓库固定的 Rust 1.85 工具链、Node.js（`npm ci`）；
- 安装与卸载需要管理员密码（`sudo`）。

## 3. 构建测试包

```bash
bash tools/package-macos/package_bundle.sh
```

脚本构建前端与三个程序（服务端单独构建，避免链接屏幕采集框架），组装 `target/macos-test/Karma-<版本>-macos/` 和同名 zip，进行临时（ad hoc）签名，生成 `SHA256SUMS`，最后运行 `tools/package-macos/test_bundle_contract.sh` 校验包内容。模型默认取自 `release/windows-x64-test/models`（ONNX 模型与清单与平台无关），可用 `KARMA_MODELS_DIR` 指定其他已校验的模型目录；设置 `KARMA_CODESIGN_IDENTITY` 可改用 Developer ID 签名并启用 hardened runtime。

## 4. 安装

```bash
cd target/macos-test/Karma-*-macos
sudo ./install.sh
```

安装脚本：

1. 拒绝在已有安装上覆盖，并校验 `SHA256SUMS`；
2. 把 `KarmaService`、`KarmaControl`、`KarmaAgent.app`、模型和卸载脚本复制到 `/Library/Karma`，把控制台复制到 `/Applications/Karma Family Protection.app`，全部归 `root:wheel` 且其他用户不可写；
3. 移除测试包的下载隔离属性（未签名测试包否则无法被 launchd 启动）；
4. 安装 `/Library/LaunchDaemons/com.karma.service.plist` 与 `/Library/LaunchAgents/com.karma.agent.plist`，启动服务和当前用户的 Agent。

安装后：

1. 打开“应用程序”中的 Karma Family Protection，首次连接时设置至少 10 个字符的管理员密码，再在系统设置中设置超级密码；
2. 在“系统设置 → 隐私与安全性 → 屏幕与系统录音”中允许 KarmaAgent。未授权时控制台的显示器状态为“不可用”。测试包为临时签名，每次重新构建并安装后都需要重新授权；macOS 15 起系统还会定期再次确认此授权。

## 5. 验证

- `sudo launchctl print system/com.karma.service`：服务应为 `running`；
- `launchctl print gui/$(id -u)/com.karma.agent`：Agent 应为 `running`；
- 控制台“保护概览”应显示服务已连接、Agent 已连接，并按显示器列出采集、图像和 OCR 状态；
- `/Library/Logs/Karma/service.log` 只包含状态码，不包含标题、网址、OCR 文本、分数或屏幕内容；
- 运行 `cargo run -p karma-macos --features session-agent --example capture-probe` 可在不安装的情况下检查本机采集（只输出数量、尺寸和状态；需要先给运行它的终端授予屏幕录制权限）。

更完整的真机检查见 [macOS 真机验收清单](macos-acceptance.md)。

## 6. 卸载

```bash
sudo /Library/Karma/uninstall.sh            # 保留策略和加密证据
sudo /Library/Karma/uninstall.sh --purge-data
```

脚本安全地读取超级密码；已申请卸载并等待 72 小时后，也可以在随后 24 小时内使用管理员密码；未设置超级密码时使用管理员密码即可。`KarmaControl` 请求 KarmaService 授权，成功后服务正常退出，脚本再移除 launchd 任务、程序和控制台。默认保留 `/Library/Application Support/Karma`；`--purge-data` 会删除策略、审计记录、密钥和加密证据。

## 7. 开发调试

无需安装即可在本机以当前用户运行完整链路：

```bash
cargo build -p karma-service-macos
cargo build -p karma-agent-macos
target/debug/KarmaService --console --data-dir /tmp/karma-dev --socket /tmp/karma-dev/service.sock \
  --agent target/debug/karma-agent-macos
KARMA_SERVICE_SOCKET=/tmp/karma-dev/service.sock \
KARMA_IMAGE_MODEL_MANIFEST=$PWD/release/windows-x64-test/models/image/viddexa-nano/manifest.json \
KARMA_OCR_LIGHTWEIGHT_MANIFEST=$PWD/release/windows-x64-test/models/ocr/pp-ocrv5-mobile/manifest.json \
  target/debug/karma-agent-macos
KARMA_SERVICE_SOCKET=/tmp/karma-dev/service.sock npm --prefix apps/karma-ui run tauri -- dev
```

- `KARMA_SERVICE_SOCKET` 只在调试构建中生效；正式构建始终连接 `/var/run/karma/service.sock` 并要求服务端为 root。
- `--console` 模式下，安装器请求（`KarmaControl`）接受当前用户，Agent 必须是 `--agent` 指定的可执行文件。
- 只做界面开发时可用独立后端：`npm --prefix apps/karma-ui run tauri -- dev -- --features local-backend`。
