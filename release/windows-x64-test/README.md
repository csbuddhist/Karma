# Karma Windows x64 测试包

这是未签名的开发测试包，不是正式发布版。它包含 `KarmaService.exe`、`KarmaControl.exe`、管理 GUI、会话 Agent、本地 ONNX/OCR 模型及 PowerShell 安装脚本。

## 推荐测试方式

1. 安装 Microsoft Visual C++ 2015–2022 Redistributable x64。
2. 以管理员身份打开 PowerShell，进入本目录。
3. 对当前 PowerShell 执行 `Set-ExecutionPolicy -Scope Process Bypass`。
4. 执行 `.\Install-Karma.ps1 -StartConsole`。
5. GUI 首次打开时创建至少 10 个字符的管理员密码。

安装脚本把测试包复制到 `C:\Program Files\Karma`，注册自动启动的 `KarmaService`，配置 SCM 崩溃恢复并启动服务。Service 会在当前活动控制台会话启动 Agent；Agent 退出后 watchdog 会重新启动它。GUI、Agent 和 Service 通过仅限本机的版本化命名管道通信。

卸载时以管理员身份执行：

```powershell
C:\Program Files\Karma\Uninstall-Karma.ps1
```

脚本会安全提示输入 Karma 管理员密码，通过 `KarmaControl.exe` 请求 Service 授权关闭，然后删除服务和程序。默认保留 `C:\ProgramData\Karma`；使用 `-PurgeData` 才会删除策略、审计记录、DPAPI 密钥和加密证据。

## 独立诊断方式

- `.\Start-KarmaConsole.ps1`：只启动 GUI；未安装 Service 时无法登录或保存服务策略。
- `.\Start-KarmaTest.ps1`：只以前台方式运行 Agent，不会取得 Service 注入的 Agent 密钥，因此 GUI 会保持 Agent 未连接。
- `KarmaService.exe --console`：只适合管理员诊断；正常安装必须由 SCM 启动。

## 已知限制

- 所有 EXE 和脚本均未签名，SmartScreen 会显示未知发布者。
- 已实现 Service、认证 IPC、心跳、策略、身份绑定处置执行端、DPAPI + AES-GCM 证据库和 Agent watchdog。启用“保存事件证据”后，达到立即处置阈值的缩放推理帧会自动加密保存并可在 GUI 中查看。
- 连续帧风险融合和来源窗口观察尚未接入，因此不会自动关闭来源应用；证据元数据暂时显示“来源应用待归属”。
- 应用时段限制、网络过滤和多用户同时登录会话尚未完成；watchdog 当前跟随活动控制台会话。
- 这是用户态家长控制。管理员仍可接管文件、服务配置或脱机修改系统；没有 ELAM/PPL/内核驱动时，不能诚实承诺对 Windows 管理员绝对不可终止。
