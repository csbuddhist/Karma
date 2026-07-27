# Karma Windows ONNX OCR 设计规格

日期：2026-07-24
状态：已完成口头设计，待书面审阅
目标平台：Windows 10 22H2、Windows 11
OCR 系列：PP-OCRv5

## 1. 目标与完成边界

本切片为现有 Windows 多显示器帧管线增加跨平台 ONNX OCR，使每个显示器在画面发生变化
时最多每秒识别一次，并把短生命周期的识别文字直接转换成现有
`OcrMatchSummary`。它同时建立轻量/高精度模型档位、安全下载、验证、基准测试、回滚和
隐私健康指标。

本切片完成：

- 使用 PP-OCRv5 检测模型定位缩放后屏幕帧中的文字区域。
- 使用 PP-OCRv5 识别模型处理简体中文、繁体中文和英文。
- 在 Rust 中实现检测预处理、DB 后处理、文本框排序、透视裁剪、识别预处理和 CTC 解码。
- 在 OCR 内部调用现有 `WordPack`，只向调用方返回风险、类别和豁免状态。
- 默认使用随安装包提供的轻量模型；高精度模型由家长启用后通过 Agent 下载。
- 自动基准选择档位，并允许家长用 `auto`、`lightweight` 或 `accurate` 显式覆盖。
- 将 OCR 接入现有 `FrameWork.run_ocr` 调度边界，并记录不含文字的健康指标。

本切片不实现图像与 OCR 风险融合、窗口来源归属、关闭应用、Windows Service、管理 UI、
签名策略编辑器或 macOS 屏幕采集。后续风险融合切片消费本切片输出的
`OcrMatchSummary`。

## 2. 技术选择

采用 PaddleOCR 官方 PP-OCRv5 模型，不直接依赖 PaddlePaddle 运行时。开发工具从固定
PaddleOCR 发布版本取得推理资产，自行转换并验证 ONNX；产品运行时只依赖现有
ONNX Runtime CPU Provider。

模型档位：

| 档位 | 检测 | 识别 | 分发 |
|---|---|---|---|
| `lightweight` | `PP-OCRv5_mobile_det` | `PP-OCRv5_mobile_rec` | 随安装包提供 |
| `accurate` | `PP-OCRv5_server_det` | `PP-OCRv5_server_rec` | 家长启用后由 Agent 下载 |

PP-OCRv5 默认识别配置覆盖简体中文、拼音、繁体中文、英文和日文；当前产品词包只启用
简体中文、繁体中文和英文字符。官方语言说明：
<https://www.paddleocr.ai/latest/en/version3.x/algorithm/PP-OCRv5/PP-OCRv5_multi_languages.html>。

PaddleOCR 源码使用 Apache-2.0 许可证，仓库和每个发行模型包都必须附带许可证与来源记录：
<https://github.com/PaddlePaddle/PaddleOCR>。

不采用以下方案：

- Windows OCR API：无法复用到 macOS，且结果受系统语言包影响。
- RapidOCR 预转换资产：会增加一层模型转换与版本信任。
- PaddleOCR-VL：模型和运行成本不适合多显示器每秒 OCR。

## 3. 组件边界

### 3.1 `karma-ai`

`karma-ai` 增加平台无关 OCR 合同：

```rust
pub trait OcrEngine {
    type Error;

    fn classify(
        &mut self,
        frame: &PreparedFrame,
        word_pack: &WordPack,
    ) -> Result<OcrMatchSummary, Self::Error>;
}
```

该接口不返回原始文字。内部识别结果使用不可序列化的 `OcrTextBatch`，其中每条文字由
`Zeroizing<String>` 持有。`Debug` 只显示行数和字符数，不显示内容。

新增的便携类型包括：

- `OcrBundleManifest`：一个完整检测/识别/字典模型包的合同。
- `OcrModelProfile`：`Lightweight` 或 `Accurate`。
- `OcrTensorContract`：输入名、输出名、类型、动态维度约束和归一化参数。
- `TextQuadrilateral`：四点文本区域，坐标始终限制在准备帧边界内。
- `DetectionMap`：检测概率图，不实现 `Serialize`，释放时清零。
- `CtcDecoder`：使用清单绑定的字符字典执行空白符、重复字符和置信度解码。
- `OcrResourceLimits`：文本框数量、像素、批量和字符数上限。

现有 `WordPack` 继续负责 Unicode NFKC、小写转换、字面规则、正则规则和豁免规则。OCR
引擎在文字仍位于清零内存中时调用 `WordPack::classify`，之后立即释放原文。

### 3.2 `karma-onnx`

`karma-onnx` 增加：

- `VerifiedOcrBundle`：验证包清单、检测模型、识别模型和字符字典。
- `OnnxOcrEngine`：持有一个检测 Session、一个识别 Session 和只读字符字典。
- `DbPostProcessor`：把检测概率图转换为排序后的四边形。
- `OcrInferenceHealth`：次数、失败、跳过框、资源超限、最近耗时和总耗时。

模型文件最多各 256 MiB，字典最多 4 MiB，清单最多 1 MiB。加载时通过同一个文件句柄
读取、计算 SHA-256 并保留已验证内存字节；Session 必须从这些字节创建，禁止校验后
重新按路径打开。

每个显示器工作器拥有独立的 `OnnxOcrEngine`，避免跨线程共享可变 Session。模型字节和
字符字典可以使用 `Arc` 只读共享。

### 3.3 Windows Agent

现有 `ScheduledImageConsumer` 重构为 `ScheduledInferenceConsumer<I, O>`：

- `run_image=true` 时执行图像分类。
- `run_ocr=true` 时执行 OCR 分类。
- 同一准备帧可依次借用给两个引擎，消费者返回后立即释放。
- 任一引擎失败不会阻止另一个引擎，也不会终止帧采集工作器。

OCR 摘要通过隐私安全的内部 sink 交给下一阶段观察组装边界。本切片的默认 sink 只维护
健康计数，不记录命中原文或分类分数。

## 4. 模型合同与供应链

一个 OCR 包目录包含：

```text
manifest.json
detector.onnx
recognizer.onnx
dictionary.txt
LICENSE
reference/
  detector-input.bin
  detector-output.json
  recognizer-input.bin
  recognizer-output.json
```

`manifest.json` 至少固定：

- 包版本、档位、Apache-2.0 标识和格式版本。
- PaddleOCR 仓库 URL、不可变提交 ID、官方模型名称和原始下载 URL。
- 原始资产和导出 ONNX 的字节长度、SHA-256。
- PaddlePaddle、Paddle2ONNX、ONNX、ONNX Runtime 的精确导出版本。
- 检测/识别模型的 opset、输入输出名、元素类型和动态维度范围。
- 检测阈值、文本框阈值、扩框比例和识别置信度阈值。
- 字典 SHA-256、CTC 空白索引、最大字符数和启用语言。
- 非敏感参考输入及参考输出 SHA-256。

导出工具执行：

1. 只从固定 URL 和版本下载官方推理资产。
2. 验证记录在导出配置中的上游 SHA-256。
3. 使用 PaddleOCR 官方 ONNX 转换路径导出动态形状模型。官方文档要求 OCR ONNX 保留
   动态形状以避免结果差异：
   <https://github.com/PaddlePaddle/PaddleOCR/blob/main/deploy/paddle2onnx/readme.md>。
4. 运行 ONNX checker。
5. 使用程序生成的非敏感中英文图比较 Paddle 与 ONNX 输出。
6. 写入最终文件长度、SHA-256、参考输入和参考输出。

运行时不信任目录名、文件名、远端响应头或 TLS 本身；只有清单、Ed25519 签名和哈希
全部通过后才加载模型。

## 5. OCR 数据流

### 5.1 检测

输入使用已有最长边 640 的 `PreparedFrame`：

1. BGRA 转 RGB。
2. 保持纵横比缩放到边长不超过 640，并把宽高向上补齐到 32 的倍数。
3. 使用清单固定的均值、标准差和缩放参数生成 NCHW `f32`。
4. 执行检测 Session。
5. DB 后处理默认使用概率阈值 `0.3`、文本框阈值 `0.6`、扩框比例 `1.5`。
6. 把四边形坐标映射回准备帧并裁剪到边界。

每帧最多保留 64 个文本框。短边小于 6 像素、面积小于 48 像素或坐标非有限值的框直接
丢弃。候选框按从上到下、同行从左到右稳定排序；同行判断阈值为框平均高度的 0.5 倍。

### 5.2 裁剪与识别

每个文本框执行透视矫正，输出高度 48 的 RGB 裁剪图。宽度按纵横比缩放，最大 320；
不足部分使用零填充。每批最多 8 个文本框，超过部分进入后续批次。

识别模型输出 CTC logits：

- 字典索引和空白索引来自已验证清单。
- 连续重复索引合并，空白索引删除。
- 单行最多 128 个 Unicode 标量。
- 平均字符置信度低于 `0.5` 的行不进入词包。
- 总识别字符数达到 4,096 后停止处理剩余文本框并增加资源超限计数。

裁剪图、检测图、识别张量和解码文字均在释放时清零。

### 5.3 词包匹配

解码完成后，`OnnxOcrEngine` 在内部构造短生命周期的 `&str` 切片并调用
`WordPack::classify`。对外只返回：

```rust
OcrMatchSummary {
    risk: OcrRisk,
    categories: Vec<String>,
    exemption_context: bool,
}
```

类别只允许来自已验证词包。初始产品类别为 `explicit_term`、`adult_service` 和
`medical_education`；原始命中词不进入观察、日志或错误。

## 6. 档位选择与基准测试

配置枚举：

```text
auto
lightweight
accurate
```

选择顺序：

1. `lightweight` 始终使用安装包模型。
2. `accurate` 在高精度包未安装时触发家长授权下载；下载完成前继续使用轻量档。
3. `auto` 在高精度包存在时执行本机基准，否则使用轻量档。

基准使用程序生成的 640×360 非敏感中英文界面图。每个候选档位预热 3 次，测量 10 次
端到端检测与识别。高精度档满足以下全部条件时才由 `auto` 选择：

- P95 不超过 800 ms。
- 10 次运行无失败或资源超限。
- 输出与包内参考摘要一致。

基准在首次安装高精度包、包版本变化、CPU 逻辑核心数变化或活动显示器数量变化时重跑。
结果只保存在本机，包含档位、模型版本、CPU 架构、逻辑核心数、活动显示器数量和耗时，
不读取或保存硬件序列号、设备唯一 ID、文字或截图。

家长显式选择 `accurate` 时优先于性能结果；若 P95 超预算，健康状态报告
`performance_budget_exceeded`，但仍使用高精度档。模型合同或参考输出验证失败不能被
手动覆盖。

运行期间不根据瞬时 CPU 动态切换，避免结果和延迟抖动。

## 7. Agent 下载与更新

轻量档随安装包提供。家长首次启用高精度档时，Agent 启动独立后台下载任务；推理线程
不执行网络、磁盘写入或签名验证。

下载状态机：

```text
Bundled/Current
  → Downloading
  → VerifyingSignature
  → VerifyingAssets
  → VerifyingRuntime
  → Benchmarking
  → PendingActivation
  → Active
```

任何失败都回到 `Current`，继续使用当前可用档位。

更新清单使用 Ed25519 签名，Agent 内置更新根公钥。签名覆盖下载并保存的原始 UTF-8
清单字节；验证签名后才允许解析，解析器拒绝未知字段和重复字段。清单固定：

- 渠道、包版本、最低 Agent 版本和发布日期。
- HTTPS 下载 URL、压缩包长度和 SHA-256。
- 解压后每个文件的相对路径、长度和 SHA-256。
- 模型包清单 SHA-256。

下载约束：

- 只允许 HTTPS 和配置的主机白名单。
- 遵循系统 HTTP(S) 代理配置。
- 连接超时 10 秒，单次读取超时 30 秒，最多重试 3 次并指数退避。
- 包格式固定为 `tar.zst`；压缩包最大 600 MiB，解压后总大小最大 1 GiB，文件数最多 32。
- 支持基于 ETag 和 Range 的断点续传；ETag 变化时丢弃旧分片重新下载。
- 临时文件位于模型缓存目录，不使用系统宽泛临时目录。
- 解压拒绝绝对路径、父目录、符号链接、硬链接和重复文件名。

下载后先验证外层签名和哈希，再验证内部清单、每个资产、ONNX 合同和参考输出。通过后
写入版本目录并通过同目录临时文件加原子重命名更新 `current.json`。不使用需要额外
Windows 权限的符号链接。保留当前版本和一个上一版本；只有新版本成功创建 Session、
通过参考验证和基准后才激活。

自动更新只在家长启用高精度模式且允许后台更新时运行。检查更新可以联网，但不得上传
屏幕、OCR 原文、命中类别、窗口、应用、文件路径或硬件明细。请求只包含渠道、Agent
版本、平台架构和当前模型包版本。

新包激活采用安全检查点：下载完成后标记 `PendingActivation`，由 Agent 监督线程在
下一次工作器安全重建时切换；不在帧处理中替换 Session。若新包导致连续 3 次 OCR
失败，自动回滚到上一版本并在本次包版本上设置失败标记，等待更高版本或家长重试。

## 8. 错误与健康状态

稳定错误类别：

- `OcrManifestInvalid`
- `OcrSignatureInvalid`
- `OcrDownloadFailed`
- `OcrArchiveInvalid`
- `OcrHashMismatch`
- `OcrDictionaryMismatch`
- `OcrModelContractMismatch`
- `OcrDetectionFailed`
- `OcrRecognitionFailed`
- `OcrOutputInvalid`
- `OcrResourceLimit`
- `OcrReferenceMismatch`

单个异常文本框只跳过该框。Session 或输出级错误增加失败计数；同一显示器连续 3 次
OCR 失败后 OCR 状态变为不可用，后续成功后恢复。OCR 不可用时图像识别、时间表和应用
规则继续运行。

健康日志只允许：

- 组件、显示器 ID、包版本和档位。
- 推理次数、失败次数、跳过框数和资源超限次数。
- 检测框数量、最近耗时和累计耗时。
- 下载状态、下载字节数、稳定错误类别和回滚结果。

禁止记录原始文字、字符索引、概率、截图、裁剪图、窗口正文、完整 URL、下载令牌或本机
绝对路径。

## 9. 测试

### 9.1 便携单元测试

- BGRA→RGB、32 倍数补齐、归一化和动态形状约束。
- DB 概率阈值、框阈值、扩框、边界裁剪和 NaN/无穷拒绝。
- 文本框稳定排序、最多 64 框和小框过滤。
- 透视裁剪、高度 48、宽度 320 上限和批量 8。
- CTC 空白、重复、简体中文、繁体中文、英文和置信度阈值。
- 4,096 字符、128 单行字符和无效字典索引限制。
- OCR 原文类型不可序列化，`Debug`、错误和健康信息不含原文。

### 9.2 ONNX 与资产测试

- 极小非敏感 detector/recognizer fixture 验证 Session 与动态维度。
- 错误输入输出名、元素类型、字典哈希和参考输出在首帧前失败。
- 文件验证后替换路径不会改变 Session 加载的已验证字节。
- Python 导出输出与 Rust ONNX 输出在清单容差内逐元素一致。

### 9.3 Windows 消费者测试

- `run_ocr=false` 不调用 OCR。
- `run_image` 与 `run_ocr` 独立运行且互不阻断。
- OCR 单次失败不终止工作器。
- 连续 3 次失败标记不可用，成功后恢复。
- 每个显示器拥有独立 Session、健康和调度状态。

### 9.4 下载与回滚测试

本地模拟服务器覆盖：

- 正常下载、系统代理、Range 续传和 ETag 变化。
- 超时、重试、错误签名、错误哈希和版本不兼容。
- 压缩炸弹、路径穿越、符号链接、重复文件和大小上限。
- 原子激活、启动验证失败、连续失败回滚和失败版本抑制。
- 请求与日志不含屏幕或 OCR 数据。

### 9.5 真机验收

Windows 10 22H2 和 Windows 11 分别记录：

- 单屏、双屏和三屏 OCR FPS、P50/P95、CPU 和工作集。
- 轻量/高精度自动选择与家长覆盖。
- 简体中文、繁体中文、英文、混排、视频字幕和浏览器小字号。
- 医疗、教育、新闻、代码、游戏 HUD 和压缩视频困难负样本。
- 代理、断网、下载中断、磁盘不足、包更新和回滚。

完成内部准确率、误报率和性能验收前，不宣称达到生产 OCR 指标。

## 10. 隐私与安全边界

- 原始帧、检测图、裁剪图、张量和识别文字只存在于内存，不写磁盘、不上传。
- 下载功能只获取签名模型资产，不提供通用 URL 下载能力。
- TLS 不是资产信任根；Ed25519 签名、SHA-256、模型合同和参考输出缺一不可。
- 家长性能覆盖不能绕过签名、哈希、合同和参考验证。
- 默认词包只输出类别；审计和 UI 不显示原始 OCR 命中词。
- 模型许可证不自动消除训练数据来源风险，正式发布前保留法律与数据来源审查记录。

## 11. 后续切片

本规格包含两个可独立审阅、按顺序实施的子项目：

1. **OCR Runtime**：模型合同、导出、便携算法、ONNX detector/recognizer、档位基准和
   Windows 帧消费者接入。高精度包可以从已经验证的本地目录加载。
2. **Signed Model Delivery**：Agent 下载状态机、系统代理、断点续传、签名验证、安全
   解包、原子激活和回滚。它只向 OCR Runtime 发布已经验证的版本目录。

两个子项目分别生成实现计划和提交记录；Signed Model Delivery 完成前，高精度在线安装
能力不视为完成。这样可在不把网络状态机与 OCR 数值算法混在同一审阅单元的前提下实现
完整设计。

完成本规格后，产品后续顺序为：

1. 把图像与 OCR 摘要组装成 `RiskObservation` 并接入每显示器风险状态机。
2. 增加窗口来源归属和可靠性门槛。
3. 实现正常关闭、超时终止和冷却期。
4. 将模型配置、下载授权和健康状态接入家长管理 UI。
