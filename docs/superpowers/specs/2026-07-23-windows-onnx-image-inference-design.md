# Karma Windows ONNX 图像推理设计规格

日期：2026-07-23  
状态：已口头确认，待书面审阅  
目标平台：Windows 10 22H2、Windows 11  
模型：`viddexa/nsfw-detection-2-nano`

## 1. 目标与完成边界

本切片将现有 `PreparedFrame` 接入 ONNX Runtime CPU Execution Provider，输出可交给
`ObservationAssembler` 和既有风险状态机处理的图像分类结果。

本切片完成以下能力：

- 从固定 Hugging Face revision 获取官方模型权重并自行导出 ONNX。
- 用清单固定模型来源、许可证、输入契约、标签顺序、文件大小和 SHA-256。
- 在加载 ONNX 会话前验证清单与模型文件。
- 将短生命周期 BGRA8 `PreparedFrame` 转换为模型所需的 RGB/NCHW 输入。
- 运行 CPU 推理，将五分类概率映射为 `Nudity`、`Suggestive` 和统一风险分数。
- 将推理接到 Windows 每显示器帧工作器，并记录不含像素的健康指标。

本切片不实现 OCR、不关闭应用、不下载运行时模型、不启用 GPU Provider，也不重新设计
风险策略阈值。Windows 真机性能与实际内容准确率仍需单独验收。

## 2. 模型与许可证

主模型采用 `viddexa/nsfw-detection-2-nano`：

- 架构：EfficientNet-B0，约 4.06M 参数。
- 许可证：Apache-2.0。
- 上游 revision：`913bc502e69fa3edfe2cfce72c98cad4ddc6149b`。
- 标签：`normal`、`hentai`、`porn`、`sexy`、`drawing`。
- 选择原因：相对 ViT 基线更轻，区分真人色情、色情绘画和暗示内容，适合多显示器 CPU 推理。

仓库只信任固定 commit/revision，不跟随 `main`。导出工具读取该 revision 的
`config.json` 和预处理配置，导出后执行 ONNX checker 与参考输出比对。最终发行资产包含
Apache-2.0 许可证和来源说明。

模型权重和导出的 `.onnx` 不直接提交普通 Git。开发脚本把它们放入忽略的本地资产目录；
打包流水线按清单校验后复制到安装包。产品运行时不会联网下载或更新模型。

模型许可证并不自动消除训练数据来源风险。首次正式发行前必须保留一次法律与数据来源
审查记录；该审查不阻塞本地技术 MVP。

## 3. 组件边界

### 3.1 `karma-ai` 便携分类契约

`karma-ai` 增加不依赖 ONNX Runtime 的类型：

- `ImageTensorSpec`：宽高、布局、颜色顺序、缩放和每通道归一化参数。
- `ImageTensorBuilder`：校验 BGRA 帧并生成紧凑 `f32` NCHW RGB 张量。
- `ClassifierOutput`：固定五类概率，不暴露运行时内部张量。
- `ViddexaRiskMapper`：把模型标签映射成 `ImageInference`。
- `ImageClassifier`：消费只读帧并返回分类结果的接口。

初始输入固定为 224×224。为避免裁掉宽屏两侧内容，整帧直接双线性缩放到模型输入大小；
这一选择可能产生纵横比形变，但不会漏掉屏幕区域。若验收显示误报或漏报不可接受，再以
独立切片评估分块推理，不在当前实现中预留复杂策略。

像素转换结果使用 `zeroize` 在释放时清理，不实现 `Serialize`，`Debug` 不输出数值。

### 3.2 ONNX Runtime 适配层

新增独立 `karma-onnx` crate，负责：

- 初始化一个进程级 ONNX Runtime 环境。
- 为每个工作器创建独立 CPU Session，避免跨线程共享可变推理状态。
- 按清单验证模型 SHA-256、输入/输出名、形状和标签顺序。
- 使用模型输出 logits 执行稳定 softmax。
- 将运行时错误转换成不含路径、像素和张量内容的稳定错误类别。

`karma-ai` 保持轻量和确定性；ONNX Runtime 依赖及其平台原生库只进入 `karma-onnx`。
Windows Agent 依赖两个 crate，但策略层不依赖 ONNX。

### 3.3 Windows 工作器接入

用 `OnnxFrameConsumer` 替换当前 `NoopFrameConsumer`。消费者仅在
`FrameWork.run_image == true` 时推理，保持每显示器最高 2 FPS 的既有调度。

每个显示器工作器拥有：

- 一个 ONNX Session。
- 一个可复用输入缓冲区。
- 独立推理健康计数。

推理成功后生成 `ImageInference`，后续交给观察组装边界。由于 OCR 尚未接入，本切片只
验证图像结果；不会伪造 OCR 结果或触发应用关闭。

## 4. 标签与分数映射

模型概率必须先按清单中的标签顺序解析，禁止依赖数组位置的隐式假设。

- `porn`：`RiskCategory::Nudity`
- `hentai`：`RiskCategory::Nudity`
- `sexy`：`RiskCategory::Suggestive`
- `normal`：无风险标签
- `drawing`：无风险标签

统一图像分数：

```text
explicit = porn + hentai
suggestive = sexy
risk = clamp(explicit + 0.35 × suggestive, 0, 1)
score_millis = round(risk × 1000)
```

若 `explicit >= 0.5`，加入 `Nudity`；若 `sexy >= 0.5`，加入 `Suggestive`。最终是否警告或
处置仍由既有连续帧风险状态机决定，分类器不直接执行系统动作。

## 5. 模型清单

扩展 `AssetManifest` 或增加专用 `ImageModelManifest`，至少固定：

- 资产类型和产品内模型版本。
- 上游仓库、固定 revision 和导出工具版本。
- SPDX 许可证标识。
- ONNX 文件名、字节长度和小写 SHA-256。
- 输入名、输出名、数据类型和静态形状。
- RGB/NCHW、224×224、像素缩放、mean/std。
- 五个标签及其输出索引。
- ONNX opset 和最低 ONNX Runtime 版本。

加载顺序固定为：解析清单、校验字段、流式计算文件哈希、创建 Session、核对图结构。任何
一步失败时，该工作器进入 AI 不可用状态，不使用未经验证的模型继续运行。

## 6. 错误与健康指标

稳定错误类别：

- `ManifestInvalid`
- `ModelMissing`
- `ModelHashMismatch`
- `RuntimeInitialization`
- `ModelContractMismatch`
- `InputPreparation`
- `InferenceFailed`
- `OutputInvalid`

健康指标只记录加载成功/失败、推理次数、失败次数、最近耗时和滑动平均耗时。不得记录
截图、张量、原始概率数组、窗口标题或模型绝对路径。

单次推理错误不会终止屏幕采集线程；消费者记录失败并继续接收后续帧。连续失败达到健康
阈值时报告 AI 不可用，由后续恢复管理切片决定是否重建 Session。

## 7. 测试与验收

便携测试：

- BGRA→RGB、通道顺序、NCHW 布局和 mean/std 的固定像素样例。
- 横屏输入缩放到 224×224，覆盖 stride padding 和异常长度。
- 输入张量 `Debug` 隐去数值，释放时清零。
- 标签重排仍按名称正确映射。
- porn/hentai/sexy 的风险分数、类别阈值和概率非法值。
- 清单缺字段、重复标签、错误形状和 SHA-256 校验。

适配层测试：

- 使用仓库内极小非敏感 ONNX fixture 验证 Session 加载和输出解析。
- 错误输入/输出名、形状和哈希在推理前失败。
- 参考张量的 Rust ONNX 输出与 Python 导出校验输出在容差内一致。

集成验证：

- macOS 运行全部便携测试、Clippy，并编译 ONNX CPU 适配层。
- `x86_64-pc-windows-msvc` 交叉编译 Windows Agent。
- Windows 真机记录每显示器推理 FPS、P50/P95 延迟、CPU、内存和连续运行稳定性。
- 使用内部合规测试集评估真人、动漫、艺术、医学、泳装、健身、视频压缩帧和普通桌面。

未完成 Windows 真机性能和自有测试集评估前，不宣称模型达到产品准确率或实时性能目标。

## 8. 隐私与供应链

- 原始帧、输入张量和分类样本只在内存中短暂存在，不写磁盘、不上传。
- 导出环境锁定 Python 依赖版本，禁止加载不可信 pickle；优先读取 safetensors。
- 导出产物生成 SHA-256、来源记录和许可证副本。
- 运行时只加载安装目录中由产品清单允许的模型文件。
- 后续模型更新必须通过签名更新包分发，不能由 Agent 临时下载替换。
