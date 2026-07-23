# Karma Windows WGC/D3D11 帧管线设计规格

日期：2026-07-23  
状态：已确认，待书面审阅  
目标平台：Windows 10 22H2、Windows 11

## 1. 目标与完成边界

本切片为 `KarmaAgent` 增加真实屏幕帧管线：以每显示器独立的 Windows.Graphics.Capture 会话接收 D3D11 帧，将帧缩放为有界内存图像，计算变化指纹，并交给现有 `FrameScheduler` 决定图像与 OCR 工作。

本切片不加载 ONNX Runtime、不运行色情模型、不实现 OCR、不处理显示器热插拔，也不实施应用关闭。它只建立经过资源约束和隐私约束的帧输入边界。ONNX 图像推理是紧随其后的独立切片。

完成条件分两层：

- macOS 开发机必须通过全部便携单元测试、Clippy 和 `x86_64-pc-windows-msvc` 交叉编译。
- Windows 真机必须验证 WGC 帧到达、尺寸变化、会话关闭、SDR 多显示器资源释放和空闲性能；未完成真机验证前不得宣称运行时采集已经验收。

## 2. 方案选择

采用 `Direct3D11CaptureFramePool::CreateFreeThreaded`，避免 Agent 依赖 UI 线程或 `DispatcherQueue`。FramePool 内部使用两个缓冲区；应用侧只保留一个最新待处理帧，旧帧被新帧替换并立即释放。

普通 `Create` FramePool 需要消息调度基础设施，不适合后台会话 Agent。DXGI Desktop Duplication 保留为未来降级适配，不进入本切片。

Windows 10 22H2 高于 `CreateFreeThreaded` 所需的 Windows 10 1809，因此不需要为更早版本添加兼容路径。

## 3. 组件边界

### 3.1 便携帧核心

在 `karma-ai` 中增加不依赖 Windows 的帧类型与算法：

- `FrameDimensions`：非零宽高和安全像素计数。
- `BgraFrame`：拥有 BGRA8 像素、行跨度、显示器 ID 和捕获时间；不实现 `Serialize`，`Debug` 隐去像素。
- `PreparedFrame`：缩放后的紧凑 BGRA8 数据、尺寸、显示器 ID、捕获时间和 64 位感知指纹。
- `FramePreparationConfig`：默认最长边 640 像素，保持纵横比，不放大小图。
- `FramePreparer`：校验 stride/缓冲区边界，生成有界帧和指纹。
- `LatestFrameMailbox<T>`：容量固定为 1；`push` 返回被替换的旧值，`take` 原子取得最新值。

便携参考缩放器使用确定性的双线性 BGRA8 缩放，作为测试基准和 GPU 路径不可用时的正确性降级。生产首选路径在 GPU 上生成同样的目标尺寸，只将缩小后的 staging texture 映射到 CPU；不得把全分辨率截图写入磁盘。

### 3.2 变化指纹

指纹使用 9×8 灰度差值哈希：

1. 从已缩放帧均匀采样到 9×8 灰度网格。
2. 每行比较相邻像素亮度。
3. 64 个比较结果组成 `u64`。

相同画面产生相同指纹；小范围色彩噪声通常不改变大部分位。当前 `FrameScheduler` 只用“是否相等”决定 OCR 是否需要重新运行，不把指纹持久化或作为内容标识上传。

### 3.3 Windows D3D11 设备

`karma-windows` 增加 `D3d11CaptureDevice`：

- 调用 `D3D11CreateDevice`，首选硬件设备并启用 BGRA support。
- 硬件设备创建失败时允许 WARP 正确性降级，并产生结构化健康状态。
- 通过 `CreateDirect3D11DeviceFromDXGIDevice` 包装为 WinRT `IDirect3DDevice`，供 WGC FramePool 使用。
- 设备、立即上下文、缩放目标和 staging texture 由处理线程拥有；不跨线程共享裸 COM 指针。

GPU 缩放由独立 `GpuFrameScaler` 边界封装。第一实现使用 D3D11 视频处理器完成 BGRA8 缩放；能力探测或运行失败时使用便携 CPU 参考缩放器。CPU 降级必须记录健康计数，但不得记录像素。

### 3.4 每显示器捕获会话

`WgcCaptureSession` 的输入是现有 `WgcCaptureTarget`、`MonitorId` 和共享 D3D11 设备。启动顺序固定为：

1. 读取 capture item 尺寸并验证非零。
2. 以 `B8G8R8A8UIntNormalized`、两个缓冲区和当前尺寸创建 FreeThreaded FramePool。
3. 注册 `FrameArrived` 与 capture item `Closed` 事件。
4. 创建 `GraphicsCaptureSession` 并调用 `StartCapture`。

`FrameArrived` 回调只执行 `TryGetNextFrame` 和容量一邮箱替换，不执行缩放、指纹、OCR、ONNX、日志写入或 IPC。保留帧时同时保留 `Direct3D11CaptureFrame`，处理完成后显式关闭，使缓冲区归还 FramePool；不得在帧关闭后保留底层 surface 引用。

### 3.5 帧处理工作器

每显示器工作器循环取得最新帧：

1. 读取 `ContentSize` 和 `SystemRelativeTime`。
2. 若尺寸与 FramePool 不一致，丢弃当前帧并发送 `RecreateRequired`。
3. 取得 `IDirect3DSurface`，在帧仍存活时完成 GPU 缩放和缩小 staging texture 复制。
4. Map 缩小 texture，复制有效行到紧凑 `PreparedFrame`。
5. 计算指纹并调用现有 `FrameScheduler::select`。
6. 将需要执行的短生命周期帧交给后续推理接口；本切片使用空实现消费者验证生命周期。
7. 清理 CPU 像素并关闭捕获帧。

处理速度落后于捕获速度时，邮箱覆盖旧帧，不形成无界队列。

## 4. 尺寸、像素和内存规则

- 输入格式限定 BGRA8；目标最长边默认 640，另一边按比例四舍五入且至少为 1。
- `width × 4`、`stride × height` 和目标分配全部使用 checked arithmetic。
- 输入 stride 可以大于紧凑行宽，缩放器不得读取 padding 之外的数据。
- 单个 640×640 BGRA8 缓冲最大约 1.64 MB；每显示器应用侧最多保留一个待处理帧和一个处理结果。
- `PreparedFrame` 不实现 serde；像素由 `zeroize` 在 Drop 时清理。
- 不创建截图文件、临时图片、崩溃附件或像素日志。

## 5. 状态与错误处理

稳定状态：

- `Starting`
- `Running`
- `RecreateRequired`
- `TargetClosed`
- `DeviceLost`
- `AccessDenied`
- `Failed`
- `Stopped`

错误分类必须区分：无效尺寸、帧池创建失败、会话启动失败、surface 互操作失败、GPU 缩放失败、staging map 失败和目标关闭。错误仅包含操作名、HRESULT/稳定错误码、显示器 ID 和计数，不包含窗口标题、像素或 OCR 数据。

尺寸变化时，控制线程先停止接收新工作并释放所有待处理帧，再调用 `Recreate`。设备移除时销毁该 D3D11 设备关联的全部会话，后续恢复管理器负责限速重建；本切片只报告状态，不实现无限重试。

`Drop` 顺序固定为：停止 capture session、移除事件 token、清空邮箱、关闭 FramePool、释放 D3D11 资源。重复停止必须幂等。

## 6. 测试设计

macOS 可执行测试：

- 非零尺寸、整数溢出和异常 stride 校验。
- 横屏、竖屏、方形、小图不放大的目标尺寸计算。
- 双线性缩放的固定像素样例与 padding stride。
- 相同图像指纹稳定，明显结构变化导致指纹变化。
- 邮箱最新值覆盖、旧值返回和并发生产/消费不积压。
- `PreparedFrame` 序列化不可用，`Debug` 不输出像素。
- FrameScheduler 只在既定频率选择图像/OCR工作。

Windows 编译测试：

- `windows` crate feature 集完整，D3D11、Direct3D11 interop、WGC FramePool 和事件签名可编译。
- `WgcCaptureSession`、设备 guard 和事件 token 的线程约束满足 Rust 类型系统。

Windows 真机验收：

- 单屏持续收到帧且内存保持有界。
- 视频播放时处理落后不会积压。
- 分辨率、缩放、旋转变化后会话进入 `RecreateRequired` 并可重建。
- 目标关闭后进入 `TargetClosed`，无忙循环。
- SDR BGRA8 颜色和尺寸正确；HDR 暂只记录兼容性限制，不作为本切片完成条件。
- 停止后事件不再触发，D3D11/WGC 对象可释放。

## 7. 安全与隐私

本能力仍是明确可见、经设备所有者授权的家长控制功能，不绕过 WGC 权限、UAC 安全桌面、DRM 或系统采集限制。原始帧仅在当前登录用户会话内短暂存在，不上传、不保存、不用于训练。

## 8. 后续切片

下一个切片在 `PreparedFrame` 边界接入 ONNX Runtime CPU Provider，实现模型输入归一化、图像分类接口、模型哈希验证和推理性能指标。OCR 模型、显示器热插拔恢复和 Windows Service 心跳仍保持独立。
