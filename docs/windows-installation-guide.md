# Karma Windows 安装与测试指南

本文档说明当前未签名 Windows x64 开发测试版的安装、验证和卸载。当前包已经包含 Windows Service、管理 GUI、会话 Agent、本地模型、认证命名管道、策略持久化、Agent watchdog、身份绑定进程处置执行端和加密证据库。

> 当前版本仍不是可用于家庭正式部署的完整产品。高风险图像的来源窗口观察和进程处置已经接入，但连续帧风险融合、应用时段限制及网络过滤仍未完成。没有 ELAM、PPL 或签名内核驱动时，本项目不能保证拥有 Windows 管理员权限的用户绝对无法终止或篡改它。

## 1. 当前可测试范围

当前端到端基础设施已实现：

- 枚举并同时监控所有活动显示器；
- 为每个显示器建立独立的 Windows Graphics Capture 和 D3D11 处理管线；
- 使用本地 ONNX Runtime 执行图像色情内容识别；
- 使用 PP-OCRv5 ONNX 模型识别简体中文、繁体中文和英文；
- 使用内置且严格校验的关键词规则生成 OCR 风险摘要；
- 独立维护每个显示器的图像和 OCR 健康状态；
- OCR 模型不可用时保留屏幕采集和图像推理；
- 仅输出计数、延迟和稳定错误代码，不输出截图、OCR 原文、命中词或分类分数。
- 自动启动的 `KarmaService`、SCM 崩溃恢复和活动控制台 Agent watchdog；
- GUI 与 Service 的密码认证、状态、策略和证据 IPC；
- Service 侧 Argon2id 密码、会话、限速、nonce 防重放和策略 revision；
- Agent 多屏健康心跳与策略快照；
- PID 创建时间和完整路径二次核验后的进程处置执行端；
- 图像分数达到立即处置阈值时，将命中画面可靠归属到命中显示器上的前台窗口和进程；
- Agent 提交绑定 PID、创建时间和完整路径的观察，Service 二次核验身份后立即关闭并终止仍存活的来源进程；
- DPAPI 保护主密钥、AES-256-GCM 加密证据和密码二次验证查看。
- 仅在启用证据且图像分数达到立即处置阈值时，异步编码并提交缩放命中帧；Service 再次校验策略后才保存；

当前版本不会：

- 执行基于连续帧融合结果的处置阈值；
- 限制上网时段或阻止应用启动；
- 根据 OCR 摘要或连续帧风险状态机触发来源进程处置；
- 执行应用时段限制或网络过滤；
- 同时管理多个已登录但非活动的 Windows 会话；
- 提供正式签名的生产 MSI 或安装 EXE。当前只提供未签名的单文件测试安装器。

## 2. 系统要求

### 2.1 支持的系统

- Windows 10 22H2 x64；
- Windows 11 x64；
- 交互式桌面会话，不支持在 Session 0 中直接进行屏幕采集；
- 支持 D3D11 和 Windows Graphics Capture 的显卡及驱动。

建议测试机器至少具备：

- 4 核 CPU；
- 8 GB 内存；
- 2 GB 可用磁盘空间；
- 最新稳定版显卡驱动；
- 一个到三个显示器。

### 2.2 必需的运行库

安装 **Microsoft Visual C++ Redistributable 2015–2022 x64**。如果没有安装，启动时通常会报告以下文件缺失：

- `MSVCP140.dll`
- `MSVCP140_1.dll`
- `VCRUNTIME140.dll`
- `VCRUNTIME140_1.dll`

不要从非微软网站单独下载这些 DLL。应安装微软提供的完整 x64 Redistributable。

### 2.3 权限

安装 Service 需要管理员权限；屏幕采集 Agent 由 Service 在活动交互桌面中以该登录用户身份启动。测试不要求安装内核驱动，也不要求关闭 Windows Defender。

首次运行时：

- Windows SmartScreen 可能提示“未知发布者”，因为当前测试 EXE 尚未进行代码签名；
- 防病毒软件可能对新生成、未签名的程序执行额外扫描；
- 不要为了运行测试而永久关闭安全软件。

## 3. 安装 Windows 测试版

### 3.1 单文件安装器（推荐）

新用户从 [v0.1.1 Windows Test Build](https://github.com/DrWeiZhou/Karma/releases/tag/v0.1.1) 下载：

```text
Karma-windows-x64-test-v0.1.1-setup.exe
```

安装第 2.2 节所述 Microsoft Visual C++ Redistributable 后，右键安装器并选择“以管理员身份运行”。安装器会：

- 把完整测试包安装到 `C:\Program Files\Karma`；
- 在注册 Service 前验证全部二进制和模型哈希；
- 注册并启动具有延迟自动启动和崩溃恢复配置的 `KarmaService`；
- 创建桌面和开始菜单管理控制台快捷方式；
- 在 Windows“已安装的应用”中注册卸载入口。卸载仍需输入 Karma 管理员密码。

安装器和内部 EXE 当前均未签名，SmartScreen 可能显示“未知发布者”。不要因此永久关闭 Windows Defender。

### 3.2 从仓库安装或诊断

测试人员无需在 Windows 上构建 Rust、下载模型或设置环境变量。克隆仓库后，图形管理界面入口位于 [`release/windows-x64-test/Start-KarmaConsole.ps1`](../release/windows-x64-test/Start-KarmaConsole.ps1)，Agent 测试入口位于 [`release/windows-x64-test/Start-KarmaTest.ps1`](../release/windows-x64-test/Start-KarmaTest.ps1)：

```powershell
git clone <远端仓库地址> Karma
Set-Location .\Karma\release\windows-x64-test
```

先安装第 2.2 节所述 Microsoft Visual C++ Redistributable。然后以管理员身份执行：

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\Install-Karma.ps1 -StartConsole
```

控制台首次成功连接 Service 后会要求创建至少 10 个字符的管理员密码。若 Service 不可用，GUI 会显示连接错误和重试按钮，不会把未知认证状态误显示成密码解锁页。Service 会自动在活动控制台会话启动 Agent，不需要另开窗口运行 `Start-KarmaTest.ps1`。

`Set-ExecutionPolicy` 只影响当前 PowerShell 进程，用于允许执行仓库内未签名脚本；它不会修改系统范围策略。启动脚本会先验证 `SHA256SUMS`、两个 JSON 模型清单和必要运行文件，验证失败时不会启动 Agent。可使用 `.\Start-KarmaTest.ps1 -OcrProfile lightweight` 显式选择轻量 OCR。

测试包目录包含：

```text
windows-x64-test\
├── karma-agent-windows.exe
├── karma-ui.exe
├── KarmaService.exe
├── KarmaControl.exe
├── DirectML.dll
├── SHA256SUMS
├── Install-Karma.ps1
├── Uninstall-Karma.ps1
├── Start-KarmaConsole.ps1
├── Start-KarmaTest.ps1
├── Verify-KarmaTestBundle.ps1
└── models\                         # Viddexa 图像模型和 PP-OCRv5 轻量模型
```

不要直接从 `target/` 复制编译缓存或将其提交到 Git；`target/` 只是开发机本地构建目录。

## 5. 测试包中的模型

克隆测试包已包含完整、已验证的图像模型和轻量 OCR 模型，正常测试不需要手工复制。`Start-KarmaTest.ps1` 会设置进程级环境变量并在启动前验证所有资产。以下目录结构和导出说明仅供重新构建测试包时使用。

推荐目录结构：

```text
C:\Karma-Test\
├── windows-x64-runtime\
│   ├── karma-agent-windows.exe
│   └── DirectML.dll
└── models\
    ├── image\
    │   └── viddexa-nano\
    │       ├── manifest.json
    │       ├── model.onnx
    │       ├── reference-output.json
    │       └── LICENSE
    └── ocr\
        └── pp-ocrv5-mobile\
            ├── manifest.json
            ├── detector.onnx
            ├── recognizer.onnx
            ├── dictionary.txt
            ├── LICENSE
            └── reference\
                ├── detector-input.bin
                ├── detector-output.json
                ├── recognizer-input.bin
                └── recognizer-output.json
```

### 5.1 图像模型

图像模型使用固定版本的 `viddexa/nsfw-detection-2-nano`。测试包提交的是开发测试所需的已导出 ONNX 资产，不是生产发布模型。

导出和验证方式见：

- [模型资产说明](model-assets.md)
- [Windows ONNX 图像推理验收](windows-onnx-acceptance.md)

在将模型复制到 Windows 前，开发机必须运行：

```bash
cargo run -p karma-onnx --example verify-model -- \
  target/model-assets/viddexa-nano/manifest.json
```

验证通过后，复制整个导出目录，不要只复制 `model.onnx`。Agent 会验证文件长度、SHA-256、输入输出名称、数据类型、静态形状和参考输出。

### 5.2 OCR 轻量模型

当前本地已验证的轻量模型目录是：

```text
/Users/wei/CodeProjects/Karma/.local-models/pp-ocrv5-mobile/
```

复制前必须在开发机执行两套独立验证：

```bash
.venv-ocr-export/bin/python tools/ocr-export/verify.py \
  .local-models/pp-ocrv5-mobile/manifest.json

cargo run -p karma-onnx --example verify_ocr_bundle -- \
  .local-models/pp-ocrv5-mobile/manifest.json
```

两条命令均输出 `status=verified` 后，再复制整个 `pp-ocrv5-mobile` 目录到 Windows。不要遗漏 `reference` 子目录、字典或许可证。

### 5.3 OCR 高精度模型

高精度模型是可选项。未安装高精度模型时：

- `lightweight` 正常使用轻量模型；
- `auto` 回退到轻量模型；
- `accurate` 当前不会自动联网下载；高精度包未配置时回退到轻量模型。当前控制台不会单独报告下载请求。

首次真机测试建议使用 `lightweight`，待轻量路径通过后再测试 `auto` 和 `accurate`。

## 6. 独立 Agent 诊断模式

正常安装不需要执行本节。只有排查模型或采集故障时才单独运行 `Start-KarmaTest.ps1`；此模式不会获得 Service 注入的 Agent 密钥，因此 GUI 不会显示 Agent 已连接。

### 6.2 可选高精度模型

准备好高精度 OCR 包后，可以增加：

```powershell
$env:KARMA_OCR_ACCURATE_MANIFEST = `
  "C:\Karma-Test\models\ocr\pp-ocrv5-server\manifest.json"

$env:KARMA_OCR_PROFILE = "auto"
```

允许值只有：

```text
auto
lightweight
accurate
```

其他值会产生稳定的 `profile_invalid` 错误，并让 OCR 进入降级状态。

### 6.3 停止

在独立诊断 Agent 的控制台中按 `Ctrl+C`。通过 Service 启动的 Agent 被结束后，watchdog 会重新启动它。

如果控制台已经关闭但进程仍在运行，可在测试阶段使用：

```powershell
Get-Process karma-agent-windows -ErrorAction SilentlyContinue
Stop-Process -Name karma-agent-windows
```

该命令只适合独立诊断。安装模式下 Service 会重新拉起活动会话 Agent；拥有 Windows 管理员权限的用户仍可通过更高权限手段绕过用户态保护。

## 7. 正常日志与状态判断

日志是控制台上的稳定状态记录。它不应包含截图、OCR 原文、关键词、类别、模型路径或 URL。

### 7.1 启动摘要

启动后应看到类似：

```text
status=ready monitors=2 wgc_ready=2 wgc_failed=0
```

字段含义：

- `monitors`：发现的活动显示器数量；
- `wgc_ready`：可以建立 Windows Graphics Capture 目标的显示器数量；
- `wgc_failed`：无法建立捕获目标的显示器数量。

### 7.2 图像推理健康

运行过程中会周期性输出类似：

```text
status=running component=image_inference inferences=120 failures=0 latency_total_us=... unavailable_monitors=0
```

重点观察：

- `inferences` 持续增加；
- `failures` 不持续快速增加；
- `unavailable_monitors=0`；
- 双屏或三屏时，一个显示器失败不阻塞其他显示器。

### 7.3 OCR 推理健康

```text
status=running component=ocr_inference inferences=60 failures=0 latency_total_us=... unavailable_monitors=0
```

如果 OCR 模型缺失或无效，图像推理仍可运行，并出现：

```text
status=degraded component=ocr_inference error=manifest_missing
```

OCR 连续失败三次后，对应显示器的 OCR 健康状态变为不可用；后续成功推理会恢复。

### 7.4 高精度性能警告

家长显式选择 `accurate` 且基准 P95 超过预算时会输出：

```text
status=warning component=ocr_profile profile=accurate reason=performance_budget_exceeded
```

这是性能警告，不是模型合同验证失败。

## 8. 首次真机测试流程

### 8.1 构建门禁

若在 Windows 源码目录原生构建，先执行：

```powershell
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p karma-agent-windows --release
```

任何命令失败都应停止测试。

如果直接使用 Mac 交叉编译生成的 ZIP，可跳过 Windows 编译，但不能跳过哈希、模型和运行库检查。

### 8.2 单显示器冒烟测试

1. 启动 Agent，确认发现一台显示器；
2. 显示普通桌面、滚动网页和正常视频；
3. 观察至少 5 分钟；
4. 确认图像和 OCR 推理计数增加；
5. 确认没有截图、临时图片或识别文本写入磁盘；
6. 确认 CPU 和内存没有持续无界增长。

### 8.3 多显示器测试

1. 连接两台或三台显示器；
2. 设置不同分辨率和 DPI；
3. 在每台显示器上展示不同的滚动页面或视频；
4. 持续运行至少 15 分钟；
5. 确认每台显示器独立处理；
6. 记录 CPU、专用工作集、句柄数量和推理延迟；
7. 拔出一个外接显示器，记录当前版本的停止行为。

当前切片尚未实现显示器热插拔后的自动重建，因此不要把自动恢复标记为已通过。

### 8.4 OCR 场景

按以下顺序测试：

- 简体中文；
- 繁体中文；
- 英文；
- 中英文混排；
- 视频字幕；
- 浏览器小字体；
- 医疗、教育、新闻、代码和游戏等负样本。

禁止在测试报告中保存截图或 OCR 原文。只记录耗时、资源、计数、状态和结果。

完整矩阵见：

- [Windows OCR 运行时验收](acceptance/windows-ocr-runtime.md)
- [Windows 帧管线真机验收](windows-frame-pipeline-acceptance.md)
- [Windows ONNX 图像推理验收](windows-onnx-acceptance.md)

## 9. 常见故障

### 9.1 缺少 VC++ DLL

现象：

```text
MSVCP140.dll was not found
```

处理：

1. 确认系统是 x64；
2. 安装 Microsoft Visual C++ Redistributable 2015–2022 x64；
3. 重新启动 PowerShell；
4. 不要将网上下载的单个 DLL 放入应用目录。

### 9.2 缺少 DirectML.dll

现象：

```text
DirectML.dll was not found
```

处理：

- 确认 `DirectML.dll` 与 `karma-agent-windows.exe` 位于同一目录；
- 重新解压完整 ZIP；
- 核对 DLL 哈希，避免使用其他来源的版本。

### 9.3 图像模型清单无效

可能日志：

```text
status=unavailable component=image_inference error=manifest_invalid
```

检查：

- 环境变量是否指向 `manifest.json`，而不是模型目录；
- 是否复制了完整模型目录；
- `model.onnx` 是否被修改；
- 清单中的文件名、长度和 SHA-256 是否匹配；
- 是否使用仓库固定版本的导出器。

图像模型无效时 Agent 会退出，因为图像推理是当前启动的必需条件。

### 9.4 OCR 模型缺失或无效

可能日志：

```text
status=degraded component=ocr_inference error=manifest_missing
```

或稳定的 OCR 清单、合同、参考输出错误类型。

OCR 失败不会阻止图像推理。检查整个 OCR 模型目录及 `reference` 子目录是否完整。

### 9.5 没有发现显示器

检查：

- 是否在真实交互式桌面中运行；
- 是否通过不支持 WGC 的远程会话启动；
- 显卡驱动是否正常；
- Windows 版本是否满足要求；
- 是否处于锁屏、UAC 安全桌面或受保护内容界面。

### 9.6 SmartScreen 阻止运行

当前 EXE 未签名。仅在确认来源和 SHA-256 后：

1. 查看 SmartScreen 详情；
2. 使用测试环境允许运行；
3. 不要将未签名测试包部署到孩子的正式账户；
4. 正式发布前必须使用受信任代码签名证书。

## 10. 当前测试版卸载

以管理员身份打开 PowerShell并执行：

```powershell
C:\Program Files\Karma\Uninstall-Karma.ps1
```

脚本要求输入 Karma 管理员密码，认证通过后才请求 Service 正常关闭并删除服务与程序目录。默认保留：

```text
C:\ProgramData\Karma
```

如确认不需要策略、审计记录、DPAPI 密钥和加密证据，可执行 `Uninstall-Karma.ps1 -PurgeData`。此操作不可恢复。

## 11. 在 Mac 上重新生成 Windows EXE

当前开发机采用：

- Homebrew LLVM 22；
- Homebrew LLD 22；
- `cargo-xwin 0.23.0`；
- 项目 Rust 1.85.1；
- Visual Studio 2022（xwin 版本 17）CRT；
- 目标 `x86_64-pc-windows-msvc`。

安装工具：

```bash
brew install llvm lld
rustup target add x86_64-pc-windows-msvc
```

`cargo-xwin 0.23.0` 需要 Rust 1.89 或更高版本来编译，但构建 Karma 时仍使用仓库固定的 Rust 1.85：

```bash
rustup toolchain install 1.89.0 --profile minimal
cargo +1.89.0 install cargo-xwin --version 0.23.0 --locked
rustup default 1.85-aarch64-apple-darwin
```

构建：

```bash
PATH="/opt/homebrew/opt/llvm/bin:/opt/homebrew/opt/lld/bin:$PATH" \
  cargo xwin build \
  --xwin-version 17 \
  -p karma-agent-windows \
  --target x86_64-pc-windows-msvc \
  --release
```

生成文件：

```text
target/x86_64-pc-windows-msvc/release/karma-agent-windows.exe
target/x86_64-pc-windows-msvc/release/DirectML.dll
```

`DirectML.dll` 可能是指向本机 Cargo 缓存的符号链接。打包时必须解引用复制：

```bash
mkdir -p target/windows-x64-runtime
cp target/x86_64-pc-windows-msvc/release/karma-agent-windows.exe \
  target/windows-x64-runtime/
cp -L target/x86_64-pc-windows-msvc/release/DirectML.dll \
  target/windows-x64-runtime/
```

### 11.1 生成单文件 Windows 安装器

安装 NSIS 3 后，构建脚本会先验证测试包契约和 `SHA256SUMS`，再生成自解压安装器：

```bash
brew install nsis
bash tools/package-windows-installer/test_installer_contract.sh
bash tools/package-windows-installer/build_installer.sh 0.1.1
```

生成文件：

```text
target/release-artifacts/Karma-windows-x64-test-v0.1.1-setup.exe
```

安装器自身包含 CRC，安装阶段还会运行包内 SHA-256 校验；两者均不能替代正式发布所需的 Authenticode 代码签名。

## 12. 未来正式签名安装器方案（尚未实现）

本节描述目标产品安装体验，不是当前可执行步骤。

### 12.1 计划的安装包

正式版计划提供签名的 MSI 或引导安装 EXE，负责：

- 检查 Windows 版本和 CPU 架构；
- 检查或安装 VC++ x64 运行库；
- 安装签名的 Agent、Service 和管理程序；
- 将只读模型资产放入受 ACL 保护的程序目录；
- 创建受限 IPC 密钥和本机设备配置；
- 注册 Windows Service 和交互式用户 Agent；
- 配置恢复策略；
- 验证安装文件签名和哈希；
- 创建开始菜单入口和受控卸载入口。

### 12.2 计划的目录

预计使用：

```text
C:\Program Files\Karma\
├── bin\
├── models\
└── licenses\

C:\ProgramData\Karma\
├── config\
├── state\
└── logs\
```

原则：

- `Program Files` 下的二进制和模型只允许受信任安装器更新；
- `ProgramData` 只保存版本化配置、计数型健康状态和审计记录；
- 不保存截图、OCR 原文或浏览内容；
- 密钥使用 Windows DPAPI 封装；
- 更新包、模型和策略使用 Ed25519 签名验证。

### 12.3 计划的安装流程

1. 家长以管理员身份启动签名安装器；
2. 安装器展示屏幕采集、进程控制和数据处理说明；
3. 家长创建管理密码；
4. 安装器部署 Windows Service 和登录会话 Agent；
5. Service 验证模型、策略和 Agent 签名；
6. 家长选择监控显示器、内容策略和上网时段；
7. 安装器执行自检；
8. 自检通过后启用保护；
9. 管理界面只通过受认证 IPC 修改策略。

### 12.4 计划的升级和卸载

正式版升级应：

- 先验证签名和版本；
- 使用临时目录完成完整校验；
- 原子切换到新版本；
- 保留一个可回滚版本；
- 升级失败时恢复上一版本。

正式版卸载应：

- 要求家长密码或恢复凭据；
- 停止保护策略；
- 停止并删除服务；
- 删除计划任务和安装目录；
- 根据家长选择保留或删除审计记录；
- 生成不含敏感内容的卸载结果。

当前仓库已实现开发测试用 PowerShell 安装和密码授权卸载，但尚未实现签名 MSI/安装 EXE、原子升级与回滚。

## 13. 发布前必须完成的工作

在把 Karma 作为家庭家长控制产品部署前，至少还需完成：

- 图像与 OCR 风险融合；
- 应用发布者核验及可配置的受保护系统进程规则；
- 高风险画面的本地遮罩；
- 上网时段和应用运行限制；
- 恢复码、密钥轮换和 IPC Windows 客户端身份校验强化；
- 安装器、代码签名、模型签名和更新签名；
- Windows 10/11 单屏、双屏、三屏真机验收；
- 准确率、误报率、性能、资源释放和稳定性验证；
- 安全评审和卸载恢复测试。

只有这些项目完成并通过验收后，才能将产品描述为具备完整保护能力。
