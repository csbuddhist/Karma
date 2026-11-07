# Karma Android 技术方案（RFC）

状态：草案 v0.1 · 日期：2026-11-07 · 适用分支：`main`

本文档定义 Karma Android 端的目标、边界、架构、目录结构、核心接口与实施路线，
作为 Android 版本开发的起点。与桌面端共享的原则（本地推理、不上传画面、管理员密码、审计）保持不变。

相关文档：`技术说明.md` / `TECHNICAL.md`（桌面端架构）、`docs/model-assets.md`（模型资产）、
`AGENTS.md`（仓库规范）。

---

## 1. 目标与非目标

### 1.1 目标

| # | 目标 | 说明 |
|---|------|------|
| G1 | 监控手机屏幕画面 | 对前台应用持续取帧（≤ 2 fps），本地推理，不保存原始帧 |
| G2 | 检测到高风险图片/视频后关闭来源应用 | 把应用打回后台并遮罩，冷却期内阻止重新进入 |
| G3 | 全部处理在手机端完成 | 无网络依赖；模型随 APK 分发并校验 |
| G4 | 无管理员密码不能退出/停用/卸载 | 分两档保护（见 §6），密码仅存 Argon2id 哈希 |
| G5 | 与桌面端共享策略语义 | 复用 `karma-domain` / `karma-policy` / `karma-ai` 的风险融合与决策，复用同一模型清单 |

### 1.2 非目标（首个版本）

- 不声称抵抗 root、自定义 Recovery、刷机或物理控制设备的人。
- 不做云端同步、远程管理、家长端 App。
- 不覆盖 HarmonyOS NEXT（非 Android）。
- 不覆盖 Android 10 及以下（`minSdk = 30`）。
- 不做 OCR（Android 可直接从无障碍树读取文本，见 §4.3）。

---

## 2. 平台约束（决定设计的事实）

| 约束 | 影响 | 应对 |
|------|------|------|
| `MediaProjection` 在 Android 14+ 每次会话需用户确认，token 一次性，重启不可静默恢复 | 不能作为常驻监控方案 | 主路径改用 `AccessibilityService.takeScreenshot()`（API 30+） |
| `FLAG_SECURE` 窗口不可截图（Chrome 无痕、Firefox 隐私模式、DRM 播放器） | 出现视觉盲区 | 策略兜底：前台为浏览器且截图返回 `ERROR_TAKE_SCREENSHOT_SECURE_WINDOW` → 直接执行 `CloseGracefully`（"看不到就不许用"） |
| `takeScreenshot` 有系统最小间隔限制 | 帧率上限 | 目标 1–2 fps，与桌面端一致 |
| 无法 kill 前台第三方进程 | "关闭应用"语义需重新定义 | `GLOBAL_ACTION_HOME` + 无障碍全屏遮罩 + `killBackgroundProcesses` + 冷却期；Device Owner 下 `setPackagesSuspended` |
| Android 13+ 侧载应用的"受限设置" | 无障碍开关默认灰色 | 引导页：应用信息 → 允许受限设置 |
| 国产 ROM（MIUI/HyperOS、ColorOS、OriginOS、EMUI）激进杀后台 | 服务静默死亡 | 前台服务 + 电池优化豁免 + 各厂商自启动白名单引导；看门狗 WorkManager 周期自检 |
| Android 15 Private Space、多用户 | 无障碍服务仅对当前用户生效 | Device Owner 下 `DISALLOW_ADD_USER`；普通档位在文档中声明为已知限制 |
| 连续截图 + 推理耗电 | 用户因耗电主动卸载 | 按需触发（§4.2），推理输入 224×224，XNNPACK CPU |

---

## 3. 总体架构

```
┌──────────────────────────────────────────────────────────────────┐
│ apps/karma-android  (Kotlin)                                      │
│                                                                   │
│  ┌──────────────┐   ┌──────────────┐   ┌───────────────────────┐  │
│  │ ui/          │   │ guard/       │   │ monitor/              │  │
│  │ Compose 管理 │   │ DeviceAdmin  │   │ KarmaAccessibility    │  │
│  │ 密码/策略/   │   │ DeviceOwner  │   │ Service               │  │
│  │ 审计/引导页  │   │ 设置页看门   │   │  ├ 前台包名/窗口事件    │  │
│  └──────┬───────┘   └──────┬───────┘   │  ├ takeScreenshot      │  │
│         │                  │           │  ├ URL / 文本提取       │  │
│         │                  │           │  └ 执行：HOME+遮罩      │  │
│         │                  │           └──────────┬────────────┘  │
│         │                  │                      │ Bitmap        │
│         │                  │           ┌──────────▼────────────┐  │
│         │                  │           │ inference/            │  │
│         │                  │           │ ONNX Runtime AAR      │  │
│         │                  │           │ (XNNPACK EP)          │  │
│         │                  │           │ viddexa-nano 224×224  │  │
│         │                  │           └──────────┬────────────┘  │
│         │                  │                      │ 5 个概率值     │
│  ───────┴──────────────────┴──────────────────────┴──────────────  │
│                     UniFFI 生成的 Kotlin 绑定                        │
└──────────────────────────────┬───────────────────────────────────┘
                               │ 小结构体，≤ 2 次/秒
┌──────────────────────────────▼───────────────────────────────────┐
│ crates/karma-mobile-ffi  (Rust, cdylib, cargo-ndk)                 │
│   ├ ViddexaRiskMapper   (karma-ai)       概率 → RiskObservation    │
│   ├ RiskState / DecisionEngine (karma-policy) 连续帧融合、规则、时段 │
│   ├ ContextPolicy       (karma-policy)   域名 / 标题关键词          │
│   ├ Argon2id 密码、恢复码、退避  (karma-service-core 抽取)          │
│   └ 审计与策略存储  rusqlite (karma-service-core::store 抽取)       │
└──────────────────────────────────────────────────────────────────┘
```

**分工原则（硬性）：**

- **Kotlin** 负责截图、Bitmap 缩放/归一化、ONNX 推理、所有 Android API 调用。
- **Rust** 负责风险融合、连续帧状态机、策略判定、密码校验、审计记录。
- **边界上只传分数与元数据，不传像素。** 单次调用负载 < 1 KB。
- Rust 侧 `observe()` 是纯函数式的：时间戳由调用方传入，不自行起线程、不读时钟。这保证
  同一段录制的分数序列可以在桌面端与 Android 端得到完全相同的决策，可作回归测试。

---

## 4. 模块设计

### 4.1 推理（Kotlin，`inference/`）

- 依赖 `com.microsoft.onnxruntime:onnxruntime-android`，Session 选项启用 XNNPACK EP，线程数 2。
- 模型资产沿用 `assets/image/viddexa-nano/`：`model.onnx` + `manifest.json`，打包进 APK `assets/`。
  启动时按桌面端同样的规则校验 `file_bytes`、`sha256`、输入名 `pixel_values`、输出名 `logits`、形状 `[1,3,224,224]`。
- 预处理严格按 `manifest.json`：RGB、NCHW、`scale = 1/255`、`mean/std` 取清单值。
  **预处理参数只能从清单读取，不得在 Kotlin 中硬编码**，否则三端分数漂移。
- 输出 softmax 后得到 `normal / hentai / porn / sexy / drawing` 五个概率，原样传给 Rust 的 `ViddexaRiskMapper`。
- 不使用 NNAPI（Android 15 起弃用、国产 SoC 驱动不一致）。GPU 加速留待评估 LiteRT。

### 4.2 取帧与触发（Kotlin，`monitor/`）

`KarmaAccessibilityService` 配置：

```xml
android:canTakeScreenshot="true"
android:canRetrieveWindowContent="true"
android:accessibilityEventTypes="typeWindowStateChanged|typeWindowContentChanged|typeViewScrolled"
android:notificationTimeout="250"
```

触发规则（降低功耗的关键）：

1. 前台包名在白名单（系统设置、输入法、Karma 自身、用户允许列表）→ 不截图。
2. 前台包名在黑名单 → 不截图，直接 `CloseGracefully`（对应桌面端 `ApplicationBlocked`）。
3. 其他应用 → 进入"观察态"：收到窗口/内容变化事件或滚动事件时截图，静止时退化为每 2 s 一帧，
   连续 30 s 无变化后暂停，下一次事件再唤醒。
4. 视频类应用（含浏览器全屏）→ 固定 1 fps，不依赖内容变化事件。
5. 截图返回 `ERROR_TAKE_SCREENSHOT_SECURE_WINDOW` 且前台为浏览器 → 视为 `SourceUncertain`，
   由策略决定（默认：浏览器隐私模式 = 关闭）。

截图得到的 `HardwareBuffer` 立即转为 224×224 软件 Bitmap 后释放，原图不落盘、不缓存。

### 4.3 文本/URL 层（Kotlin，`monitor/context/`）

无障碍树可直接读到：

- 浏览器地址栏 URL（Chrome `com.android.chrome:id/url_bar`、Edge、Firefox、三星浏览器、UC/夸克等需逐一适配 view id）；
- 窗口标题、视频标题、`contentDescription`。

这些文本交给 Rust `ContextPolicy::evaluate(browser_host, window_title)`，复用
`assets/keyword-lists/window-title-explicit.json` 与域名规则。因此 Android 端不需要 OCR 模型。

### 4.4 执行（Kotlin，`monitor/enforce/`）

Rust 返回 `Decision { action, reason, policy_id, expires_at_ms }` 后：

| Action | Android 实现 |
|--------|-------------|
| `Allow` | 无操作 |
| `Warn` | 顶部无障碍 Toast 式提示，不打断 |
| `CloseGracefully` | `performGlobalAction(GLOBAL_ACTION_HOME)` → 弹 `TYPE_ACCESSIBILITY_OVERLAY` 全屏遮罩（显示原因代码、剩余冷却时间、管理员解锁入口）→ `ActivityManager.killBackgroundProcesses(pkg)` |
| `Terminate` | 同上，并在 Device Owner 档位 `setPackagesSuspended([pkg], true)` 至 `expires_at_ms` |
| `BlockNetwork` | 首版不实现；预留给 VpnService 层 |

冷却期内该包名再次进入前台 → 直接重复执行，不再推理。

### 4.5 安全存储（Kotlin `security/` + Rust）

- 管理员密码：Rust 侧 Argon2id 哈希（参数与桌面端一致），失败指数退避 + 审计事件；一次性恢复码仅存哈希。
- 数据库：rusqlite，文件级 AES-GCM 加密，密钥由 Android Keystore 生成（`setUnlockedDeviceRequired(false)`，
  保证锁屏时后台服务可用）。不使用已弃用的 Jetpack Security `EncryptedSharedPreferences`。
- Kotlin 侧不再另开 Room 访问同一数据库；UI 偏好等纯 Android 数据放 DataStore。

---

## 5. 共享核心：`crates/karma-mobile-ffi`

新增 crate，`crate-type = ["cdylib", "staticlib"]`，通过 UniFFI（proc-macro 模式）导出。
依赖 `karma-domain`、`karma-policy`、`karma-ai`，以及从 `karma-service-core` 抽出的
与 Windows 无关的存储/口令逻辑（抽取本身是一次独立重构，见 §8 M2）。

接口草案（UniFFI 伪代码，以 Rust 为准）：

```rust
#[derive(uniffi::Record)]
pub struct ClassifierProbabilities {   // 来自 Kotlin ORT 输出
    pub normal: f32, pub hentai: f32, pub porn: f32, pub sexy: f32, pub drawing: f32,
}

#[derive(uniffi::Record)]
pub struct ForegroundContext {
    pub package_name: String,
    pub window_title: String,
    pub browser_host: Option<String>,   // 从地址栏提取的 host，非浏览器为 None
    pub secure_window: bool,            // 截图被 FLAG_SECURE 拒绝
    pub captured_at_ms: i64,
    pub minute_of_week: u16,
}

#[derive(uniffi::Object)]
pub struct KarmaCore { /* DecisionEngine + ContextPolicy + Store */ }

#[uniffi::export]
impl KarmaCore {
    #[uniffi::constructor]
    pub fn open(data_dir: String, db_key: Vec<u8>) -> Result<Arc<Self>, CoreError>;

    /// 一帧的视觉结果 + 上下文 → 决策。纯函数式，时间由调用方给。
    pub fn observe_frame(&self, probs: ClassifierProbabilities, ctx: ForegroundContext) -> Decision;

    /// 没有截图（白名单/黑名单/安全窗口）时仅凭上下文决策。
    pub fn observe_context(&self, ctx: ForegroundContext) -> Decision;

    pub fn verify_admin_password(&self, password: String) -> Result<AdminSession, AuthError>;
    pub fn change_admin_password(&self, session: AdminSession, new_password: String) -> Result<(), AuthError>;
    pub fn redeem_recovery_code(&self, code: String) -> Result<AdminSession, AuthError>;

    pub fn get_policy(&self) -> PolicyDocument;
    pub fn set_policy(&self, session: AdminSession, policy: PolicyDocument) -> Result<(), CoreError>;

    pub fn record_audit(&self, event: AuditEventRecord);
    pub fn query_audit(&self, since_ms: i64, limit: u32) -> Vec<AuditEventRecord>;
}
```

构建：`cargo ndk -t arm64-v8a -t x86_64 -o apps/karma-android/core/src/main/jniLibs build --release -p karma-mobile-ffi`，
然后 `uniffi-bindgen generate --library ... --language kotlin`。两步封装进 Gradle 任务 `cargoBuild`。

`arm64-v8a` 为唯一正式 ABI，`x86_64` 仅供模拟器。

---

## 6. 防篡改分档

| 档位 | 安装方式 | 能力 | 可被绕过的方式 |
|------|---------|------|---------------|
| **L1 普通安装** | 用户自行安装 APK，开启无障碍、Device Admin、电池豁免 | 卸载前必须停用 Device Admin；无障碍"看门"识别 Settings 中与 Karma 相关的页面（应用信息、停用设备管理器、无障碍开关、强行停止）立即 BACK 并弹密码框；前台服务常驻通知 | 安全模式启动、恢复出厂、ADB、换 ROM |
| **L2 Device Owner** | 设备无账号时 `adb shell dpm set-device-owner com.karma.android/.guard.KarmaDeviceAdminReceiver` | `setUninstallBlocked`、`DISALLOW_APPS_CONTROL`、`setPermittedAccessibilityServices`、`DISALLOW_SAFE_BOOT`、`DISALLOW_FACTORY_RESET`、`DISALLOW_DEBUGGING_FEATURES`、`DISALLOW_ADD_USER`、`setPackagesSuspended`；解除 DO 本身走管理员密码 | 刷机、Bootloader 解锁、物理拆机；部分国产 ROM 对 DO 支持不完整 |

文档与应用内引导都要明确告诉用户两档的差异，与桌面端 README "限制"一节保持同一口径。
不提供 root/Magisk 方案。

---

## 7. 目录结构

```
Karma/
├── apps/
│   ├── karma-agent-windows/            （现有）
│   ├── karma-service-windows/          （现有）
│   ├── karma-ui/                       （现有，Tauri 桌面管理台）
│   └── karma-android/                  ★ 新增：Gradle 工程，Android Studio 直接打开此目录
│       ├── settings.gradle.kts
│       ├── build.gradle.kts
│       ├── gradle/libs.versions.toml
│       ├── app/                        应用模块
│       │   ├── src/main/AndroidManifest.xml
│       │   ├── src/main/assets/models/viddexa-nano/   (model.onnx + manifest.json，构建时从 assets/image 复制)
│       │   ├── src/main/res/xml/accessibility_service_config.xml
│       │   ├── src/main/res/xml/device_admin.xml
│       │   └── src/main/kotlin/com/karma/android/
│       │       ├── KarmaApp.kt
│       │       ├── ui/                 Compose：Onboarding、Unlock、Policy、Audit、Status
│       │       ├── monitor/            KarmaAccessibilityService、FrameScheduler、ScreenshotSource
│       │       │   ├── context/        BrowserUrlExtractor、TitleExtractor
│       │       │   └── enforce/        Enforcer、BlockOverlay、CooldownRegistry
│       │       ├── inference/          OrtSessionHolder、ManifestVerifier、Preprocessor
│       │       ├── guard/              KarmaDeviceAdminReceiver、DeviceOwnerPolicies、SettingsWatchdog
│       │       ├── security/           KeystoreKeys、CoreBootstrap
│       │       └── service/            MonitorForegroundService、WatchdogWorker、BootReceiver
│       ├── core/                       Android library 模块：UniFFI 生成的 Kotlin + jniLibs/
│       │   └── build.gradle.kts        含 cargoBuild / uniffiBindgen 任务
│       └── README.md
├── crates/
│   ├── …（现有）
│   └── karma-mobile-ffi/               ★ 新增：UniFFI 导出层（§5）
├── assets/image/viddexa-nano/          （现有，Android 复用）
├── assets/keyword-lists/               （现有，Android 复用）
├── docs/
│   ├── android-technical-plan.md       ★ 本文档
│   ├── android-acceptance.md           （M1 后补：真机验收清单，格式同 windows-*-acceptance.md）
│   └── android-rom-compatibility.md    （M3 后补：各厂商 ROM 存活与 DO 支持矩阵）
└── tools/
    └── package-android/                （M4：签名、校验和、Release 资产脚本）
```

`crates/karma-mobile-ffi` 加入 `Cargo.toml` 的 `[workspace] members` 时机：M2 开始、能通过
`cargo build --workspace` 后再加，避免空壳破坏 CI。

---

## 8. 实施路线

| 里程碑 | 内容 | 验收 |
|--------|------|------|
| **M0 骨架** | Gradle 工程、Compose 空壳、无障碍服务可开启、前台服务常驻 | Pixel 真机上重启后服务自动恢复 |
| **M1 视觉闭环** | `takeScreenshot` → 预处理 → ORT XNNPACK 推理 → 临时 Kotlin 阈值 → HOME + 遮罩 | 浏览器打开测试图集时 ≤ 1.5 s 内打回；连续运行 1 h 电量消耗记录 |
| **M2 接入 Rust 核心** | `karma-mobile-ffi` + UniFFI + cargo-ndk；从 `karma-service-core` 抽出平台无关的存储/口令；删除 M1 临时阈值 | 同一分数序列在桌面端单元测试与 Android instrumented test 得到相同 `Decision` |
| **M3 L1 防篡改 + 国产 ROM** | Device Admin、SettingsWatchdog、电池豁免、厂商自启动引导页、WorkManager 看门狗 | 小米/OPPO/vivo/华为各一台，24 h 存活；设置页绕过路径全部被挡 |
| **M4 L2 Device Owner + 文本层** | DO 策略集、`setPackagesSuspended`、浏览器 URL 提取与 `ContextPolicy`、安全窗口兜底 | DO 模式下无法卸载/停用/安全模式；无痕模式被关闭 |
| **M5 发布** | 签名、`android-v0.1.0` tag、Release 资产（APK + SHA256）、README/README.en/技术说明/TECHNICAL 同步更新 | 遵守 AGENTS.md 双语要求 |
| M6（可选） | `VpnService` 本地 DNS/SNI 过滤，对应 `BlockNetwork` | — |

---

## 9. 构建与 CI

- 工具链：JDK 17、AGP 8.x、Kotlin 2.x、Compose BOM、NDK r27、`cargo-ndk`、`uniffi-bindgen`，Rust 目标
  `aarch64-linux-android` / `x86_64-linux-android`。
- `.github/workflows/android.yml`：`paths` 过滤 `apps/karma-android/**`、`crates/karma-mobile-ffi/**`、
  `crates/karma-{domain,policy,ai}/**`；步骤：cargo test（核心）→ cargo ndk → gradle assembleDebug → gradle test。
- 真机验收不进 CI，按 `docs/android-acceptance.md` 人工执行并附证据。

---

## 10. 版本与发布

- Tag 前缀：`android-vX.Y.Z`，与桌面端节奏独立。
- Release 资产：`karma-android-vX.Y.Z-arm64.apk` + `.sha256`；Release 说明中英双语，注明基于的核心提交。
- 模型文件不进 Git（沿用 `docs/model-assets.md` 流程），打包脚本从本地导出目录复制并校验清单。

---

## 11. 合规与披露

- Google Play：无障碍服务需在 Play Console 声明用途（家长控制/数字健康类），应用内需有明确的
  常驻通知与权限说明；否则按 stalkerware 政策处理。
- 国内分发：各应用商店对无障碍与自启动有独立审核；首版可先以 GitHub/Gitee Release APK 分发。
- 隐私承诺与桌面端一致：不上传画面、不保存原始帧、审计仅记录包名/原因/时间。

---

## 12. 已知风险

| 风险 | 等级 | 缓解 |
|------|------|------|
| 国产 ROM 杀后台 | 高 | M3 专项；引导页 + 看门狗；文档明确支持矩阵 |
| FLAG_SECURE 盲区 | 中 | 安全窗口 = 关闭 的默认策略；URL 层补充 |
| Device Owner 配置门槛 | 中 | 提供一键脚本 `tools/package-android/set-device-owner.sh` 与图文教程 |
| 功耗 | 中 | 按需触发、224 输入、XNNPACK；M1 起持续测量 |
| 误杀 | 中 | 沿用桌面端滑动窗口融合；Android 阈值初期保守 |
| `karma-service-core` 抽取影响 Windows 端 | 中 | M2 作为独立 PR，Windows 全量测试通过后合并 |
