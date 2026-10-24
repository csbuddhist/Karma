# English

Karma v0.3.16 is a Windows x64 **prerelease for testing** that detects explicit video playing in only part of the screen, such as a video embedded in a web page or a shrunken player window. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Focused recognition of moving regions**

- Image recognition used to classify only the whole screen, squeezed into the 224×224 model input. A video covering a quarter of the screen became a small patch surrounded by unrelated page content, so its risk score stayed below the enforcement threshold.
- The Agent now compares consecutive frames of each display on an 8-pixel grid. Areas that keep changing, such as an embedded video, a small player window, or a rotating image carousel, are cropped and classified on their own, in addition to the whole screen. The highest score is used for closing the window and for evidence; the evidence image is still the whole screen.
- Avatars, emoji, cursor movement, tickers, and full-screen video (which the whole-screen inference already sees clearly) are not cropped. At most two regions per frame are classified, so image inference costs up to three times as much per display while such a video plays, and nothing extra while the screen is still.
- A small **still** image in a web page is still judged only as part of the whole screen and can be missed when it covers too little of it.
- The motion grid keeps only per-cell brightness in zeroized memory; it is never logged, saved, or sent.

**Upgrading**

- v0.3.12 through v0.3.15 can update to this version from the console or automatically. v0.3.11 and earlier must run `Karma-windows-x64-test-v0.3.16-setup.exe` once and enter the administrator password. Existing local policy, evidence, and audit data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Karma remains user-mode protection: a technical Windows administrator can take ownership, use safe mode, or alter the system offline. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the ONNX inference acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.16/docs/windows-onnx-acceptance.md), including the new embedded-video and small-player checks.

# 简体中文

Karma v0.3.16 是 Windows x64 **测试预发布版**，新增对只占屏幕一部分的色情视频的识别，例如网页中内嵌的视频或缩小的播放器窗口。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**动态区域重点识别**

- 此前图像识别只对整个屏幕进行，整屏画面被压缩到 224×224 的模型输入。只占屏幕 1/4 的视频因此只剩一小块，周围又是大量无关网页内容，风险分数达不到处置阈值。
- Agent 现在按 8 像素网格比较每个显示器的相邻帧。持续变化的区域（如网页内嵌视频、小窗播放器、轮播图片）除整屏识别外还会被裁剪出来单独识别，取最高分用于关闭窗口和保存证据；证据图片仍为整屏画面。
- 头像、表情、鼠标移动、滚动字幕以及全屏视频（整屏识别已能看清）不会被裁剪。每帧最多额外识别 2 个区域，因此播放这类视频时每个显示器的图像推理开销最多约为原来的 3 倍，画面静止时不增加。
- 网页中静止不动的小尺寸**图片**仍只按整屏识别，占比过小时可能漏检。
- 动态网格只在会清零的内存中保存每格平均亮度，不写入日志、不保存、不发送。

**升级**

- v0.3.12 至 v0.3.15 可通过控制台或自动更新升级到此版本。v0.3.11 及更早版本需先运行一次 `Karma-windows-x64-test-v0.3.16-setup.exe` 并输入管理员密码。默认保留现有本地策略、证据和审计数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。Karma 仍是用户态保护：懂技术的 Windows 管理员可夺取所有权、进入安全模式或离线修改系统。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成 [ONNX 推理验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.16/docs/windows-onnx-acceptance.md)中的项目，包括新增的内嵌视频与小窗播放器检查。
