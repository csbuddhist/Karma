# Karma macOS 版本设计规格

日期：2026-10-23  
状态：开发中（分支 `feat/macos-port`，进度见 6.1）  
基线：Windows 测试版 v0.3.8 已实现的功能  
目标平台：macOS 13 Ventura 及以上，Apple Silicon 与 Intel  

## 1. 目标

以 Windows v0.3.8 已稳定的功能为准，在 macOS 上提供同一产品：同一套 Tauri 管理控制台、同一份策略格式、同一个 `ServiceCore`（SQLite、密码、审计、证据加密、重复违规封禁、记住的标题、网络时间表计算），由 macOS 原生执行器负责屏幕采集、前台归属、进程处置、后台常驻和网络控制。

原则：

- **共享核心不分叉**。`karma-service-core`、`karma-policy`、`karma-ipc` 协议、`karma-ai`、`karma-onnx` 两个平台共用；平台差异只出现在宿主层（传输、进程控制、密钥保护、采集、网络过滤、更新）。
- **只用受支持的系统接口**。不绕过 TCC、不隐藏进程、不注入其他进程、不安装根证书、不修改 `/etc/hosts`。
- **Windows 行为不回退**。对共享代码的改动只做加法或纯代码搬移，并用 `cargo xwin clippy --target x86_64-pc-windows-msvc` 与全部单元测试守住 Windows 行为。

## 2. 进程拓扑

```text
/Library/LaunchDaemons/com.karma.service.plist
  └─ KarmaService (root，开机启动，launchd KeepAlive 自动恢复)
       ├─ ServiceCore：策略、密码、审计、证据、封禁、记住的标题
       ├─ Unix 域套接字 /var/run/karma/service.sock（内核对端身份校验）
       ├─ 进程处置：身份校验 → SIGTERM → 宽限期 → SIGKILL
       └─ 网络时间表（M3：Network Extension 内容过滤器）

/Library/LaunchAgents/com.karma.agent.plist（每个图形会话，LimitLoadToSessionType=Aqua）
  └─ KarmaAgent.app/Contents/MacOS/karma-agent-macos（当前用户，KeepAlive）
       ├─ ScreenCaptureKit：每个显示器一个 SCStream
       ├─ CGWindowList：前台窗口、窗口标题、显示器归属
       ├─ ONNX 图像识别 + OCR
       └─ 通过套接字向 KarmaService 报告观察、证据与心跳

/Applications/Karma.app（Tauri 控制台，按需启动，无特权）
```

与 Windows 的差别：

| 能力 | Windows | macOS |
| --- | --- | --- |
| 特权服务 | `KarmaService` Windows 服务（LocalSystem） | `KarmaService` LaunchDaemon（root） |
| 服务自恢复 | SCM 恢复动作 + `KarmaGuardian` 互相守护 | launchd `KeepAlive`；只有 root 能 `launchctl bootout`，不需要 Guardian |
| Agent 启动 | 服务在活动控制台会话中 `CreateProcessAsUser` | launchd 按图形会话启动 LaunchAgent；服务只接受当前控制台用户的 Agent |
| 本地 IPC | 命名管道 + DACL（SY、BA、IU） | Unix 域套接字，目录 root 拥有 0755，套接字 0666；同一套长度前缀 JSON 帧 |
| Agent 认证 | 服务通过环境变量下发 64 位十六进制密钥 | 内核对端身份：`LOCAL_PEERTOKEN` 审计令牌 → `proc_pidpath_audittoken` 得到精确可执行文件路径，必须是 root 拥有的安装路径，且 uid 为当前控制台用户；宿主替 Agent 填入核心令牌，密钥从不离开 daemon |
| 安装器请求（卸载授权、离线更新） | 任何管道客户端 + 密码 | 对端 uid 必须为 0（`sudo KarmaControl ...`）+ 密码 |
| 数据目录 | `C:\ProgramData\Karma`，ACL 仅 SYSTEM/管理员 | `/Library/Application Support/Karma`，root:wheel 0700，文件 0600 |
| 证据主密钥 | DPAPI（本机）包装 | M1：root 专属 0600 文件（依赖 FileVault 静态加密）；M5：系统钥匙串包装 |
| 处置 | `WM_CLOSE` → 宽限期 → `TerminateProcess` | `SIGTERM` → 宽限期 → `SIGKILL`，均通过 `proc_signal_with_audittoken` 作用于已校验的那个进程实例 |
| 屏幕采集 | Windows.Graphics.Capture + D3D11 GPU 缩放 | ScreenCaptureKit，由系统按 640 像素最长边输出 BGRA，最高 4 帧/秒 |
| 前台归属 | `GetForegroundWindow` + 显示器重叠面积 | `CGWindowListCopyWindowInfo` 由前到后的第 0 层窗口；同样的 `SourceAttributor` 重叠规则 |
| 窗口标题 | `GetWindowTextW` | `kCGWindowName`（需要“屏幕录制”权限，Agent 本来就需要） |
| 浏览器地址栏域名 | UI Automation | M4：辅助功能（AXUIElement），需要“辅助功能”权限；未授权时标题与图像规则照常工作 |
| 网络时间表 | WFP 持久过滤器 | M3：Network Extension 内容过滤器（系统扩展，需要 Developer ID 与 entitlement） |
| 已安装应用选择器 | 注册表 + 开始菜单 | 扫描 `/Applications`、`/System/Applications`、`~/Applications` 的 `.app`，读取 `Info.plist` |
| 开机自启控制台 | tauri-plugin-autostart（注册表） | tauri-plugin-autostart（LaunchAgent） |
| 安装 / 卸载 | NSIS 安装包 + PowerShell | M3：开发安装脚本；M5：签名并公证的 `.pkg` |
| 签名更新 | 发布清单 Ed25519 签名 + zip + A/B | M5：同一套清单验证，载荷为 macOS 包 |

## 3. 安全设计

### 3.1 套接字与对端身份

- 套接字目录 `/var/run/karma` 由 root 创建为 0755，任何用户都不能替换套接字文件；套接字本身 0666，等同于 Windows 管道向交互用户授予读写。
- 服务端每接受一个连接，就通过 `getsockopt(SOL_LOCAL, LOCAL_PEERCRED / LOCAL_PEERPID / LOCAL_PEERTOKEN)` 取得内核记录的对端 uid、pid 与审计令牌。审计令牌包含 pid 版本号，进程退出或 `exec` 之后令牌失效，因此路径查询不存在 PID 复用竞争。
- 按请求的 `ClientKind` 授权：
  - `Ui`：任何本机用户都可连接，所有敏感操作仍由管理员密码、会话令牌与超级密码保护（与 Windows 相同）。
  - `Agent`：对端可执行文件必须等于安装目录中的 Agent，且 uid 必须是当前控制台用户（`/dev/console` 的所有者）且不为 root。通过后宿主把核心的 Agent 令牌写入请求；对端自带的任何令牌都被丢弃。未通过时直接返回 `AgentUnauthorized`，请求不进入核心。
  - `Installer`：对端 uid 必须为 0。
- 正式签名版（M5）在路径校验之外增加代码签名要求：`SecCodeCopyGuestWithAttributes(审计令牌)` + `SecCodeCheckValidity`，要求 Team ID 与 Bundle ID 匹配，并启用 hardened runtime 阻止 `DYLD_INSERT_LIBRARIES` 与调试器附加。开发版只有路径与 uid 校验，与 Windows 测试版“用户本人可读取 Agent 环境变量”的信任级别相当，文档中明确标注。

### 3.2 进程处置

- 只接受核心返回的 `DispositionRequired`，目标为 Agent 报告、核心复核过的 `ProcessIdentity`（pid、启动时间、可执行文件路径）。
- 首次检查时用 `task_name_for_pid` + `task_info(TASK_AUDIT_TOKEN)` 取得目标进程实例的审计令牌，再用 `proc_pidpath_audittoken` 与 `proc_pidinfo(PROC_PIDTBSDINFO)` 复核路径、启动时间与 uid。之后的 `SIGTERM` / `SIGKILL` 都用同一个令牌发送，系统保证只作用于这个实例。
- 拒绝处置：uid 为 0、uid 小于 501、uid 不是当前控制台用户、pid ≤ 1、Karma 自身组件。拒绝结果记为 `AccessDenied` 并进入审计。
- 优雅关闭使用 `SIGTERM`（浏览器与 Cocoa 应用会正常退出并保存会话），宽限期上限 5 秒，与 Windows 一致。

### 3.3 数据与密钥

- 数据目录 `/Library/Application Support/Karma`：`service-state.db`（SQLite WAL）、`agent.secret`、`evidence.key`、`evidence/`，全部 root:wheel，目录 0700、文件 0600。
- 证据密钥在 M1 为 root 专属文件。威胁模型与 Windows 相同：本机管理员 / root 可以读取。M5 改为系统钥匙串包装。
- 不记录屏幕原图、OCR 文本、完整 URL；记住的标题仍按 Windows 方案以 AES-256-GCM 加密保存在服务数据库中。

### 3.4 明确的边界

- 管理员可以 `sudo launchctl bootout` 停止服务、删除文件或关闭 SIP；这些只能事后发现，与 Windows 管理员边界一致。
- 用户可以在“系统设置 → 隐私与安全性”中撤销 Agent 的屏幕录制权限。Agent 进入失效安全状态并在心跳中报告显示器 `Unavailable`，控制台显示告警。
- macOS 15 起系统会定期再次确认屏幕录制授权，并显示录屏指示器；Karma 不尝试隐藏或绕过。
- 受 DRM 保护的内容、登录窗口与锁屏无法采集。

## 4. 策略在 macOS 上的语义

- 应用规则仍按“可执行文件路径尾部组件”匹配。macOS 的进程映像路径形如 `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`，规则可以写 `Google Chrome`、`Google Chrome.app/Contents/MacOS/Google Chrome` 或完整路径。
- `KNOWN_BROWSER_EXECUTABLES` 追加 macOS 浏览器的可执行文件名（`google chrome`、`safari`、`firefox`、`microsoft edge`、`brave browser`、`opera`、`vivaldi`、`arc`、`chromium` 等）。这些名字没有 `.exe` 后缀，不会与 Windows 进程映像路径相撞，所以对 Windows 行为没有影响。
- “从证据一键允许”接受 Windows 的 `X:\...\*.exe` 绝对路径，也接受 macOS 的 POSIX 绝对路径；Windows Agent 永远不会报告后者。
- 记住的标题、标题关键词、网站允许 / 阻止列表、重复违规封禁的逻辑完全共用。

## 5. 代码组织

```text
crates/karma-macos-ipc     Unix 域套接字传输：服务端、客户端、对端凭据
crates/karma-macos         macOS 平台适配：libproc 进程信息、控制台用户、审计令牌信号、
                           显示器枚举、CGWindowList 窗口、ScreenCaptureKit 采集
crates/karma-agent-core    从 karma-agent-windows 搬出的可移植 Agent 逻辑（推理调度、
                           OCR 档位、词包、策略句柄、服务客户端），两个平台 Agent 共用
apps/karma-service-macos   KarmaService（LaunchDaemon）与 KarmaControl
apps/karma-agent-macos     KarmaAgent（LaunchAgent，打包为 KarmaAgent.app）
apps/karma-ui              控制台：macOS 走服务后端；`local-backend` 特性保留独立开发后端
tools/package-macos        开发版组装、launchd plist、安装与卸载脚本
```

macOS 原生接口用 Rust + `objc2` 系列绑定实现（与 Tauri 依赖的 objc2 0.6 同版本），整个工作区仍只需要 `cargo` 构建，不引入 Swift 工具链。

## 6. 分阶段计划

| 阶段 | 内容 | 本机可验证性 |
| --- | --- | --- |
| **M1 服务端基础** | `karma-macos-ipc`；`karma-macos` 进程与信号；`karma-service-macos`（数据目录、密钥、核心、对端校验、处置、`--console` 开发模式、KarmaControl）；控制台 macOS 服务后端与已安装应用；macOS 浏览器名单 | 单元测试 + 以普通用户在临时目录运行 `--console`，控制台连接 |
| **M2 会话 Agent** | 抽取 `karma-agent-core`（Windows Agent 改为依赖它）；ScreenCaptureKit 采集、显示器热插拔、CGWindowList 归属与标题；`karma-agent-macos` 串起推理、证据、心跳、前台上下文 | 单元测试 + 授予终端屏幕录制权限后本机实测；Windows 用 xwin 交叉检查 |
| **M3 打包与网络** | `tools/package-macos`：KarmaAgent.app、Karma.app、plist、安装 / 卸载脚本；验收清单 `docs/macos-acceptance.md`；Network Extension 内容过滤器（需要 Apple Developer 账号，可能拆到 M3b） | 开发安装需管理员授权，由用户在本机执行 |
| **M4 浏览器域名** | 通过辅助功能读取 Chrome / Safari / Edge / Firefox / Arc 地址栏主机名 | 需要授予辅助功能权限 |
| **M5 发布** | Developer ID 签名、hardened runtime、公证、`.pkg`、代码签名要求校验、钥匙串包装密钥、签名更新、Endpoint Security（视 entitlement 审批） | 需要 Apple Developer 账号 |

### 6.1 当前进度（2026-10-23）

| 阶段 | 状态 |
| --- | --- |
| M1 服务端基础 | 已完成：`karma-macos-ipc`、`karma-macos` 进程与会话、`karma-service-macos`、控制台 macOS 服务后端与已安装应用、macOS 浏览器名单、平台文案 |
| M2 会话 Agent | 已完成：`karma-agent-core` 抽取（Windows Agent 已改用并通过 xwin 交叉检查）、ScreenCaptureKit 采集、CGWindowList 归属、`karma-agent-macos`；本机已验证 Agent 经对端校验接入服务、模型加载与无权限时的失效保护，真实帧路径待授予屏幕录制权限后验证 |
| M3 打包 | 测试包已完成：`tools/package-macos`、安装 / 卸载脚本、约定测试、安装指南与验收清单；Network Extension 未开始（需要 Apple Developer 账号） |
| M4 浏览器域名 | 未开始 |
| M5 发布 | 未开始 |

## 7. 验收标准（macOS）

- `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check` 在 macOS 通过；`cargo xwin clippy --target x86_64-pc-windows-msvc` 无警告。
- 非 Agent 进程（包括同一用户复制出的 Agent 二进制）以 `ClientKind::Agent` 连接时被拒绝；非 root 进程发出 `Installer` 请求时被拒绝。
- 处置只作用于当前控制台用户、身份完全匹配的进程；root 进程、系统进程与身份变化的进程一律不处置。
- 每个活动显示器独立采集；显示器接入或断开后 5 秒内恢复监控。
- 撤销屏幕录制权限后，心跳中对应显示器状态变为不可用，控制台可见。
- 默认运行不产生任何原始屏幕文件；日志中不出现标题、URL、OCR 文本或分数。
