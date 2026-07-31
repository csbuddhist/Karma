# Karma 管理控制台

`karma-ui` 是面向家长的 Tauri + React 本地管理界面。生产窗口在展示任何设备状态、策略或事件前，必须先通过 Rust 后端验证管理员密码。

## 当前能力

- 首次运行创建管理员密码，使用 Argon2id 加盐哈希保存；
- 失败认证限速、单一内存会话、15 分钟无操作自动锁定和主动锁定；
- 总览、显示器、图像/OCR 设置、关键词、应用规则、每周时段、事件证据、审计和系统设置页面；
- 设置保存在 Tauri 应用数据目录，不把密码明文写入磁盘；
- 事件证据入口默认模糊，并要求再次验证管理员密码后才能请求原图。

当前 Windows Agent 尚未连接 Windows Service，也不会保存截图。因此显示器实时状态、系统级应用处置、审计写入以及加密证据原图仍显示为“未连接/待接入”。界面不会用模拟数据冒充这些能力已经工作。

浏览器开发模式提供本地降级桥接，只用于设计和交互测试；正式运行必须使用 Tauri 窗口，不能把浏览器模式的本地存储当成安全边界。

## 本地运行

```bash
cd apps/karma-ui
npm install
npm run tauri -- dev
```

只检查前端时可运行：

```bash
npm run dev
```

## 验证

```bash
npm run build
cargo test -p karma-ui
cargo clippy -p karma-ui --all-targets -- -D warnings
```

## 后续接入边界

下一阶段由 `KarmaService` 持有策略、密码验证、DPAPI 保护的证据密钥、审计库和命名管道。UI 后端应从本地文件状态迁移为受认证的 Service IPC 客户端；Agent 只能向 Service 提交有界的高风险证据，不能向 UI 或任意路径直接写文件。
