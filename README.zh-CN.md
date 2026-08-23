# Karma

Karma 是一套面向 macOS 和 Windows 的本地色情内容防护与数字健康软件。它持续监控所有显示器，在识别到高风险图片或视频后关闭来源应用，并提供应用黑白名单、上网时间段、管理员密码、审计日志和防随意退出能力。

本项目采用“一套产品、一个共享核心、两个原生执行端”的架构。界面、策略、AI、数据格式可以共用，但屏幕采集、进程管控、网络过滤、后台常驻及设备管理必须使用各平台受支持的原生能力。

> 本软件不能对拥有本机管理员、root、恢复环境或物理控制权的人承诺绝对不可卸载。强约束模式必须配合标准用户账户以及 MDM、WDAC/AppLocker 等设备管理能力。

## Windows 测试安装包

新用户可从 [v0.1.9 Windows Test Build](https://github.com/DrWeiZhou/Karma/releases/tag/v0.1.9) 下载单文件 `Karma-windows-x64-test-v0.1.9-setup.exe`，在 Windows 10 22H2 或 Windows 11 x64 上以管理员身份运行即可完成文件安装、哈希校验、Service 注册和快捷方式创建。安装前仍需安装 Microsoft Visual C++ 2015–2022 x64 Redistributable。

`main` 同时保留可审查的 **cloneable Windows test bundle**：[`release/windows-x64-test/`](release/windows-x64-test/)，用于诊断和复现打包。安装器检测到现有版本时会询问是否卸载，通过现有卸载器要求输入 Karma 管理员密码，自动关闭已安装的管理控制台，并且只在 Service 确认移除后继续安装。管理控制台默认在用户登录后后台启动，可在“系统设置”中关闭；保护 Service 则独立随 Windows 启动。该安装器仍是未签名的开发/测试软件，卸载继续要求 Karma 管理员密码；它不是正式签名的生产安装器。详细步骤见 [Windows 安装与测试指南](docs/windows-installation-guide.md)。

## 设计原则

- 本地优先：屏幕图像默认只在内存中处理，不上传、不保存原始画面。
- 及时处置：以连续帧识别代替低频截图，目标是在高风险内容出现后 1–3 秒内响应。
- 多屏独立：每个显示器单独采集、推理和维护判定状态。
- 最小权限：管理界面不持有系统权限，特权操作只允许由签名服务执行。
- 不解密 HTTPS：不安装自签根证书，不采用全局 HTTPS 中间人代理。
- 分层防护：普通密码保护、系统服务自恢复、设备托管依次增强。
- 可审计：所有策略变化和处置动作均形成结构化事件，但不记录敏感画面和完整浏览内容。

## 总体架构

```text
Tauri + TypeScript 管理界面
              │ 本地认证 IPC
              ▼
        Rust 共享核心
  ┌──────────────────────────┐
  │ ONNX 色情内容识别         │
  │ 时间与应用策略引擎        │
  │ 连续帧判定状态机          │
  │ SQLite、加密、审计日志    │
  │ 配置校验与统一 IPC 协议   │
  └──────────────────────────┘
         │              │
         ▼              ▼
 macOS 会话代理       Windows 会话代理
 ScreenCaptureKit     Windows.Graphics.Capture
 NSWorkspace          Win32 / WinRT
         │              │
         ▼              ▼
 macOS 特权执行端     Windows 特权服务
 LaunchDaemon         Windows Service
 Endpoint Security    WFP / App Control
 Network Extension    可选签名驱动
         │              │
         └──── MDM / 设备策略 ────┘
```

管理 UI、会话代理和特权执行端必须是不同进程。系统服务运行在特权上下文中，但屏幕采集只能在用户图形会话中完成，不能把 Windows Session 0 服务或 macOS LaunchDaemon 当作截屏进程。

## 进程拓扑

### Windows

```text
KarmaService.exe                 LocalSystem，开机自动启动
  ├─ 策略执行与看门狗
  ├─ WFP/防火墙规则管理
  ├─ 应用处置与服务恢复
  └─ 为每个登录会话启动并验证：
       KarmaAgent.exe            当前用户会话
         ├─ 多显示器采集
         ├─ 前台窗口/PID 归属
         ├─ ONNX 推理
         └─ 与 UI、Service 通信

KarmaUI.exe                      Tauri 管理界面，按需启动
```

### macOS

```text
com.karma.daemon                 LaunchDaemon，root
  ├─ 策略执行与看门狗
  ├─ Network Extension 管理
  ├─ System Extension 协调
  └─ 验证登录会话代理

KarmaAgent.app                   当前用户图形会话
  ├─ ScreenCaptureKit 多屏采集
  ├─ 前台应用归属
  ├─ ONNX 推理
  └─ 管理 UI

KarmaEndpointExtension           Endpoint Security System Extension
```

## 共享核心

共享核心使用 Rust，输出 Windows 和 macOS 本地库，也可以由会话代理作为独立进程加载。

### 策略引擎

统一支持：

- 每周七天、15 分钟粒度的允许/禁止时间段。
- 浏览器、播放器、游戏和自定义应用分类。
- 应用路径、包 ID、发布者签名和文件哈希规则。
- 黑名单、白名单以及默认策略。
- 浏览器域名白名单与黑名单：白名单始终优先；黑名单无需等待图像风险达到阈值即可立即关闭。
- 使用版本化高精度英语、汉语、日语和俄语成人内容词表匹配最前台窗口标题，并叠加控制台自定义的高风险与敏感关键词；自定义豁免词可抑制关键词命中。
- 临时放行、剩余使用时长和冷却期。
- 锁屏、睡眠、时区变化和夏令时修正。
- 策略优先级及冲突解释。

推荐优先级：

```text
设备强制策略 > 紧急停用策略 > 时间段禁止 > 应用黑名单
             > 临时放行 > 应用白名单 > 默认允许
```

浏览器内容处置采用更具体的优先级：`网址白名单 > 网址黑名单 > 应用“允许使用” > 标题关键词 > 图像阈值`。域名规则覆盖其子域名，但不会误匹配相邻域名。控制台配置的应用规则由 Service 强制执行：“允许使用”的应用（按可执行文件名或完整路径尾缀匹配）不会再被图像或标题识别关闭——但网址黑名单仍然生效；“始终禁止”的应用在前台窗口被观察到时直接关闭。

策略决策返回结构化结果，而不是直接执行系统操作：

```rust
Decision {
    action: Allow | Warn | CloseGracefully | Terminate | BlockNetwork,
    reason: ReasonCode,
    policy_id: String,
    expires_at: Option<Timestamp>,
}
```

### AI 识别

- ONNX Runtime，本地 CPU/GPU 推理。
- 同一份模型、标签、归一化参数和阈值覆盖两个平台。
- 每个显示器维护独立滑动窗口。
- 有界帧预处理最多每秒 4 帧，图像推理最多每秒 2 帧；既避免对每次捕获回调执行昂贵工作，也将预处理准入延迟控制在 250 毫秒内。
- 推理前缩放并模糊极小区域，降低文字和头像误判。
- 视频和静态图片采用同一连续帧状态机。

建议判定规则：

```text
score >= 0.95                         → 立即处置
最近 5 秒内至少 3 帧 score >= 0.82   → 处置
最近 8 秒内至少 5 帧 score >= 0.70   → 警告或处置
低于阈值持续 10 秒                    → 清除风险状态
```

阈值必须可通过签名配置更新。模型命中后，先确定该显示器上的前台窗口及所属 PID/Bundle ID，再处置来源应用，避免杀死无关浏览器或后台程序。

当前 Windows 测试版在保存控制台敏感度时会同步更新立即处置阈值，并迁移此前保存的不一致值。关闭图像识别后，Agent 会停止图像分类器工作，同时禁止生成图像证据或处置事件。

### 数据与加密

- SQLite 使用 WAL 模式。
- 配置、策略和审计日志采用版本化 schema。
- 管理员密码只保存 Argon2id 哈希，禁止可逆保存。
- 数据库密钥由 Windows DPAPI 或 macOS Keychain 封装。
- IPC 密钥首次安装时生成并存入系统凭据存储。
- 更新包、模型和策略均验证 Ed25519 签名。

默认只记录：

- 时间、设备、用户和显示器编号。
- 应用标识、发布者和处置结果。
- 模型版本、风险等级和归一化分数。
- 策略 ID、原因码和组件健康状态。

默认不记录屏幕截图、窗口正文、完整 URL、输入内容或动态生成的站点证书。Windows Agent 只在内存中短暂处理活动窗口标题和浏览器地址栏，仅向本机 Service 发送规范化域名及有长度上限的标题供独立复核；审计与证据记录均不会持久化这两个值。

## 屏幕采集

### Windows

- 使用 `EnumDisplayMonitors` 枚举全部活动显示器。
- 通过 `IGraphicsCaptureItemInterop::CreateForMonitor` 为每个 `HMONITOR` 创建 `Windows.Graphics.Capture` 会话。
- 每个登录会话运行一个 `KarmaAgent`，监听显示器插拔、分辨率、缩放、旋转和 HDR 变化。
- 使用 D3D11 纹理完成 GPU 缩放，尽量避免 GPU→CPU 全尺寸复制。
- WGC 不可用时可降级到 DXGI Desktop Duplication，但必须记录降级事件。
- 锁屏、UAC 安全桌面和受 DRM 保护内容不承诺可采集。

当前仓库已实现 Windows 帧输入、图像推理与 OCR 推理切片、前台窗口标题观察、浏览器地址栏域名发现，以及 Windows Service、认证 IPC、策略持久化、Agent watchdog、健康心跳、身份绑定处置执行端和 DPAPI + AES-GCM 证据库。Agent 在独立 apartment 中轮询活动窗口，先在本地评估上下文，仅在命中时向 Service 提交有长度上限的观察；Service 会重新应用白名单、黑名单与多语种标题策略，再授权绑定进程身份的关闭操作。图像观察也只携带规范化浏览器域名，确保白名单网站不会因为高图像分数被关闭。启用“保存事件证据”后，只有达到立即处置阈值的图像推理帧才会在 Agent 中编码并提交给 Service 加密保存，Service 会再次校验策略与阈值。Agent 从已验证的本地清单加载图像与 OCR 模型，运行时不会向日志输出识别原文、分数、标题、URL 或画面。连续帧风险融合仍未接入。

Windows 当前通过 UI Automation 支持 Chrome、Edge、Firefox、Brave、Opera/Opera GX、Vivaldi 和 Arc 的浏览器域名发现。规则只匹配域名（并覆盖子域名），不匹配 URL 路径。如果浏览器隐藏地址栏或未向辅助功能公开地址栏，Karma 无法识别该窗口是否命中网址白名单/黑名单；标题与图像规则仍继续工作。

管理界面位于 [`apps/karma-ui/`](apps/karma-ui/)：Windows 构建已通过支持并发客户端的本机命名管道连接 `KarmaService`，管理员密码、会话、实时 Agent/显示器状态、策略 revision 和证据查看均由 Service 掌控；系统设置中可在复验当前密码后修改管理员密码，并可将当前策略导出为 JSON 备份文件或导入既有备份以便检查和恢复，导入前会校验备份结构与策略合法性。Service 会话仅保存在内存中：KarmaService 停止再启动后，控制台会检测到会话失效并回到解锁页，重新解锁即恢复连接，不会一直停留在“服务尚未连接”状态；连接失败时 GUI 会显示明确的 Service 连接错误，不会再把未知认证状态误显示为密码解锁页。非 Windows 开发构建仍使用隔离的本地后端。

macOS 开发机上的测试和 `x86_64-pc-windows-msvc` 交叉编译只证明便携算法、Rust 类型约束和 Windows API 签名正确；GPU 驱动行为、实际帧颜色、资源释放及多屏性能必须按 [Windows 帧管线真机验收清单](docs/windows-frame-pipeline-acceptance.md) 与 [Windows ONNX 真机验收清单](docs/windows-onnx-acceptance.md) 在 Windows 10 22H2/Windows 11 上验证，未记录该证据前不视为运行时验收完成。

### macOS

- 使用 `SCShareableContent` 枚举 `SCDisplay`。
- 每台显示器建立独立 `SCStream`，输出到串行采集队列。
- 监听显示器配置变化并重建对应流。
- 首次启动明确申请 Screen Recording 权限；权限被撤销时进入失效保护状态并通知监护人。
- 不尝试绕过 TCC、系统采集指示或受保护内容限制。

### 帧归属

采集代理同步维护：

- 当前前台窗口及 PID。
- 窗口与显示器的交集面积。
- 全屏应用和画中画窗口。
- 浏览器多窗口场景。

命中帧优先归属于覆盖该显示器面积最大的前台窗口。无法可靠归属时只显示警告并临时遮罩，不直接终止多个候选进程。

## 应用管控

应用管控借鉴展翅鸟“持续进程监视、即时终止、服务重新拉起”的思路，但使用受支持、可签名和可审计的系统接口。

### 处置梯度

```text
1. 阻止新网络连接
2. 向窗口发送正常关闭请求
3. 等待 2 秒并重新检查
4. 终止确定的目标进程
5. 在冷却期内阻止相同应用重新启动
6. 记录事件并通知管理端
```

浏览器命中后默认关闭对应浏览器进程组；支持多配置文件的浏览器应尽量定位窗口所属进程。播放器、图片查看器和游戏可以直接按主进程处理。

### Windows 应用管控

基础模式：

- 使用 ETW/WMI/Win32 进程事件监测新进程。
- 验证可执行文件路径、签名发布者和父进程。
- 正常关闭使用 `WM_CLOSE`。
- 超时后由 `KarmaService` 使用受限句柄调用 `TerminateProcess`。
- 服务维护冷却表，应用重启时再次处置。

强化模式：

- 使用 AppLocker 或 WDAC 在禁止时间段执行应用控制策略。
- 使用 WFP ALE 层按应用标识阻断网络连接。
- 如基础模式无法达到自保护目标，再开发最小化、EV 签名的驱动；驱动只负责进程句柄保护和事件通知，不承载 AI、配置或 UI。
- 禁止采用全局 DLL 注入和无边界的键盘 Hook。

### macOS 应用管控

基础模式：

- 使用 `NSWorkspace` 获取运行应用及启动/退出通知。
- 正常关闭使用 `NSRunningApplication.terminate()`。
- 超时后由特权执行端发送受控终止信号。

强化模式：

- Endpoint Security 订阅 `AUTH_EXEC`，在进程真正执行前依据本地缓存策略允许或拒绝。
- 订阅相关 signal 事件，审计针对 Karma 组件的终止尝试。
- 授权回调禁止访问 SQLite、网络或 ONNX；所有可判定策略预编译为只读内存快照，确保在系统 deadline 内返回。
- Endpoint Security entitlement 未获 Apple 批准时，产品必须降级并明确显示能力差异。

## 防退出与自保护

### 一级：密码保护

- UI 退出、暂停监控、修改策略、卸载和临时放行均需要管理员密码。
- UI 关闭只隐藏管理界面，不停止 Agent 或 Service。
- 连续失败进行指数退避并记录审计事件。
- 提供一次性恢复码，恢复码只显示一次并以哈希形式保存。

### 二级：服务保护与自动恢复

- Windows Service/LaunchDaemon 开机自动启动。
- Service 和 Agent 双向心跳，任一异常退出后由系统服务恢复。
- 服务只接受签名客户端通过本机认证 IPC 发出的命令。
- 安装目录归 `SYSTEM/root` 和管理员所有，普通用户只读，禁止类似 `Everyone: FullControl` 的权限。
- 服务控制 ACL 不授予普通用户停止、修改或删除权限。
- 启动时验证组件签名和哈希，异常时进入故障保护并通知管理端。
- 使用操作系统原生恢复机制，避免两个用户态进程无限互相拉起。

### 三级：设备托管

Windows：

- 标准用户不持有管理员凭据。
- MDM 下发服务、WFP、WDAC/AppLocker 和卸载限制。
- BitLocker、防篡改启动策略和 Secure Boot 保持开启。

macOS：

- 标准用户不持有管理员凭据。
- MDM 下发 PPPC、System Extension、Network Extension 和应用不可移除策略。
- FileVault 和 System Integrity Protection 保持开启。

### 明确边界

以下情况只能检测或事后报告，不能保证阻止：

- 管理员/root 主动停用或卸载组件。
- 安全模式、恢复环境、离线修改磁盘。
- 用户撤销 macOS 屏幕录制权限。
- 禁用 Secure Boot/SIP、重装系统或更换启动盘。
- 物理遮挡、外部采集设备或另一台设备播放内容。

## 网络时间管控

网络管控只负责“哪些应用在什么时间可以联网”，色情画面判定仍由屏幕 AI 完成。

### Windows

- 使用 WFP ALE connect/accept 层按应用路径、用户和协议建立持久过滤器。
- 时间段变化时原子切换过滤器集合。
- 服务崩溃时由持久规则维持最后一次安全状态。
- 基础版本可先使用 Windows Firewall API，复杂场景再增加 WFP callout。

### macOS

- 使用 Network Extension Content Filter 或 DNS Proxy。
- 按应用和策略阻断连接，DNS 只用于域名级分类。
- 不通过修改 `/etc/hosts` 或反复切换系统代理实现强制控制。

浏览器扩展可以作为可选增强，用于提供完整 URL 分类和友好拦截页面，但不能成为唯一的安全边界。

## IPC 与权限边界

统一 IPC 协议使用长度前缀消息和版本字段：

```text
UI       → Core/Service：读取状态、提交带认证的策略变更
Agent    → Core        ：帧推理请求、窗口归属信息
Core     → Service     ：结构化 Decision，不直接传任意命令
Service  → Agent       ：健康检查、采集配置、策略快照版本
```

安全要求：

- Windows 使用命名管道并配置明确的 DACL。
- macOS 使用 XPC，并校验 Team ID、Bundle ID 和代码签名 requirement。
- 服务端不接受“执行任意路径”“终止任意 PID”等通用命令。
- 所有 PID 操作都必须同时验证启动时间、签名和应用标识，防止 PID 复用。
- IPC 包含 nonce、时间戳和会话密钥，拒绝重放。

## 故障保护

| 故障 | 默认行为 |
|---|---|
| AI 模型加载失败 | 停止色情识别，保留时间和应用策略，持续告警 |
| 单个显示器采集失败 | 重建该显示器流，不影响其他显示器 |
| Agent 崩溃 | Service 重启 Agent；短时间多次失败进入限速恢复 |
| Service 崩溃 | 操作系统服务恢复机制重启；网络保持最后策略 |
| 策略数据库损坏 | 使用最后一份签名快照并进入只读模式 |
| 系统时间突变 | 使用单调时钟校验，重新计算时间策略 |
| IPC 认证失败 | 拒绝请求并记录安全事件 |
| 权限被撤销 | 显示不可忽略告警；托管设备上通知管理端 |

## 安装、签名与更新

### Windows

- 使用 MSI 安装 Service、Agent、UI 和可选驱动。
- 所有 EXE、DLL、MSI 和驱动使用可信代码签名。
- 安装时配置服务 SID、目录 ACL、命名管道 DACL 和恢复策略。
- 卸载必须经过管理员密码和 UAC；MDM 模式由设备策略决定是否允许。

### macOS

- 使用签名 `.app`/`.pkg` 分发并完成 Apple 公证。
- 首次运行引导 Screen Recording、System Extension 和 Network Extension 权限。
- MDM 环境通过 PPPC 与 System Extension payload 预批准允许的能力。
- 更新程序验证 Team ID、指定 requirement 和更新包签名。

更新流程采用 A/B 组件目录：下载、验签、预检查、切换、健康确认；失败时自动回滚。数据库迁移必须支持向前恢复，禁止更新失败后清空配置。

## 与展翅鸟实现的取舍

保留的思想：

- 系统服务与登录会话代理分离。
- 实时监视进程并在策略命中后处置。
- 服务负责恢复核心组件。
- 本地模型识别，不把屏幕上传云端。
- 时间段和应用黑白名单统一判定。

明确不采用：

- 自签根证书和全局 HTTPS 中间人代理。
- 每隔数分钟把完整屏幕明文保存到磁盘。
- 未签名核心程序和普通用户可写安装目录。
- 全局 DLL 注入、隐蔽文件和过度键盘 Hook。
- 将密码、AI、网络、UI 和自保护耦合进单体进程。

## 技术栈

| 层 | 技术 |
|---|---|
| UI | Tauri + TypeScript |
| 共享核心 | Rust |
| AI | ONNX Runtime |
| 数据 | SQLite + DPAPI/Keychain |
| macOS 适配 | Swift、ScreenCaptureKit、XPC |
| macOS 强化 | Endpoint Security、Network Extension、MDM |
| Windows 适配 | Rust/C++、Win32、WinRT、WGC |
| Windows 强化 | Windows Service、WFP、WDAC/AppLocker、可选 WDK 驱动 |

C# 可以用于快速验证 WGC 或开发管理工具，但正式 Windows 执行端优先使用 Rust/C++，避免额外运行时和跨语言服务边界；WFP callout 与内核自保护也必须使用 WDK 支持的原生实现。

## 实施阶段

### Phase 1：可验证 MVP

- Rust 策略引擎、SQLite 和 IPC schema。
- Windows/macOS 单屏采集适配器。
- ONNX 推理和连续帧判定。
- 前台应用归属和正常关闭。
- Tauri 设置界面与管理员密码。

### Phase 2：多屏与基础常驻

- 多显示器热插拔及独立状态机。
- Windows Service 和 macOS LaunchDaemon。
- Agent 心跳、自恢复和签名验证。
- 应用黑白名单、时间段和冷却策略。
- 内存推理与隐私审计。

### Phase 3：网络与强化管控

- Windows WFP/ALE 过滤。
- macOS Network Extension。
- Endpoint Security `AUTH_EXEC`。
- WDAC/AppLocker 和 MDM 配置模板。
- 权限撤销、离线和故障保护测试。

### Phase 4：生产发布

- Windows/macOS 完整签名、公证和安装器。
- 模型与程序安全更新、A/B 回滚。
- 性能、误报率、电池和多用户会话测试。
- 辅助功能、隐私说明、数据导出和卸载流程。

## 验收指标

- 所有活动显示器均被独立采集，插拔后 5 秒内恢复监控。
- 常规色情视频在出现后 3 秒内触发，连续帧误杀率满足测试集目标。
- 禁止时间内应用启动后 500 毫秒内被拦截或处置。
- 普通用户无法停止系统服务、修改策略或卸载组件。
- Agent 异常退出后 5 秒内恢复，且不会形成无限重启风暴。
- 空闲状态 CPU 平均占用低于 3%，AI 活跃状态根据硬件设定分级预算。
- 默认运行 30 天不产生任何原始屏幕文件。
- 不安装根 CA，不记录完整网页内容和用户输入。
- 每个管理操作和系统处置都有可验证的审计记录。

## 测试重点

- 双屏、三屏、不同 DPI、旋转、HDR、睡眠唤醒和远程桌面。
- 浏览器视频、图片查看器、播放器、游戏和画中画。
- 多用户登录、快速用户切换、锁屏和会话注销。
- 应用反复重启、进程树变化、PID 复用及更新后路径变化。
- Agent/Service 被终止、数据库损坏、模型损坏和磁盘空间不足。
- macOS TCC 权限撤销、System Extension 未批准及 Endpoint Security 超时。
- Windows WFP 与 VPN、代理、安全软件和企业防火墙共存。
- 误报、漏报、肤色偏差、动漫内容以及医疗和艺术场景。
- 普通用户、管理员和 MDM 托管三种威胁等级。

## 合规与透明度

Karma 应只用于设备所有者、监护人或组织在合法授权范围内的内容防护。安装过程必须明确说明屏幕监控、应用处置、日志范围和卸载方式。产品不得隐藏监控事实，不采集键盘输入，不将截图用于训练，也不得通过技术手段绕过操作系统的隐私提示和设备所有者权限。
