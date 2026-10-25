# English

Karma v0.3.21 is a Windows x64 **prerelease for testing** that recognizes images in small player windows and makes application rules follow a program by its file name. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Small player windows are recognized**

- A player window shrunk to about 202×115 or smaller was not recognized. Its size was judged on the downscaled frame used for inference (long edge at most 640 px), where such a window is only a few dozen pixels and was dropped as too small; enlarging it would only have produced a blur.
- Moving regions are now measured at the display's native resolution and kept when their short edge is at least 64 px. A moving region that would be small for the model is copied straight from the native-resolution capture and then enlarged to the model input, so it stays sharp. Avatars, cursor trails, and tickers are still ignored.

**Application rules match by program file name**

- Application control now records only the program file name (.exe), whether you type it, pick an installed application, batch-add applications, use one-click allow, or allowlist an evidence item. A rule applies to the program wherever it is installed or copied, as long as the file name is the same (case-insensitive).
- Rules saved with a full path by earlier versions match by their file name from now on and show only the file name. Application categories (used by blocking schedules and browser recognition) resolve their members by file name too.

**Upgrading**

- v0.3.12 through v0.3.20 can update to this version from the console or automatically. v0.3.11 and earlier must run `Karma-windows-x64-test-v0.3.21-setup.exe` once and enter the administrator password. Existing local policy, evidence, and audit data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the ONNX inference acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.21/docs/windows-onnx-acceptance.md), including the new small-window items.

# 简体中文

Karma v0.3.21 是 Windows x64 **测试预发布版**，现在可以识别缩小的播放器小窗，应用规则也改为按程序文件名匹配。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**识别缩小的播放器小窗**

- 播放器窗口缩小到 202×115 左右及更小时无法识别。原因是尺寸在用于推理的缩小画面（长边最多 640 像素）上判断，这样的小窗只剩几十像素，被当作过小区域丢弃；即使放大，也只是一片模糊。
- 现在动态区域按显示器原始分辨率计算尺寸，短边至少 64 像素即单独识别。对模型来说偏小的动态区域会直接从原始分辨率的采集画面中拷贝出来，再放大到模型输入，画面保持清晰。头像、鼠标轨迹和滚动字幕仍会被排除。

**应用规则按程序文件名匹配**

- “应用管控”现在只记录程序文件名（.exe）：无论手动填写、从已安装应用中选择、批量添加、一键允许还是从证据加入白名单，都是如此。只要文件名相同（不区分大小写），程序换了安装位置或被复制到别处，规则照样生效。
- 早期版本按完整路径保存的规则，今后也按其文件名匹配，列表中只显示文件名。应用类别（供断网屏蔽和浏览器识别使用）的成员同样按文件名解析。

**升级**

- v0.3.12 至 v0.3.20 可通过控制台或自动更新升级到此版本。v0.3.11 及更早版本需先运行一次 `Karma-windows-x64-test-v0.3.21-setup.exe` 并输入管理员密码。默认保留现有本地策略、证据和审计数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成 [ONNX 推理验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.21/docs/windows-onnx-acceptance.md)中的项目，包括新增的小窗识别项目。
