# macOS 真机验收清单

状态：待 macOS 真机执行  
适用版本：macOS 13 Ventura 及以上，Apple Silicon  
隐私约束：不得保存或附加截图；只记录版本、硬件、计数、状态和性能数据。

## 1. 验收前记录

- 日期与测试人：
- Mac 型号、芯片、内存：
- macOS 版本与 build：
- 显示器数量、分辨率与缩放：
- Karma 提交哈希与测试包版本：
- 签名方式（临时签名 / Developer ID）：

## 2. 安装与常驻

- [ ] `sudo ./install.sh` 成功；已安装时再次运行被拒绝。
- [ ] `/Library/Karma`、`/Applications/Karma Family Protection.app`、两个 plist 均为 `root:wheel` 且其他用户不可写。
- [ ] `/Library/Application Support/Karma` 为 0700，其中文件为 0600；`/var/run/karma` 为 root 拥有的 0755，套接字为 0666。
- [ ] `sudo kill -9 <KarmaService PID>` 后 10 秒内服务重新运行。
- [ ] 普通用户 `kill` Agent 后 5 秒内 Agent 重新运行。
- [ ] 普通用户 `launchctl bootout gui/$(id -u)/com.karma.agent` 后 60 秒内服务把 Agent 重新加载。
- [ ] 重启后服务与 Agent 自动运行。
- [ ] 快速用户切换后，新的控制台用户的 Agent 被接受，旧会话的 Agent 被拒绝。

## 3. IPC 与身份

- [ ] 复制 `karma-agent-macos` 到用户目录并运行：服务日志出现 `status=denied component=service_ipc client=Agent`，控制台不显示其心跳。
- [ ] 非 root 运行 `/Library/Karma/KarmaControl shutdown`：被拒绝，卸载未开始。
- [ ] 以普通用户在其他路径伪造 `/var/run/karma/service.sock` 不可行（目录不可写）。
- [ ] 控制台在服务停止时显示“KarmaService 尚未运行或本地套接字不可用”，服务恢复后可重新解锁。

## 4. 采集与识别

- [ ] 首次启动 Agent 时出现屏幕录制授权提示；授权前控制台显示器状态为“不可用”。
- [ ] 授权后每个显示器的采集、图像、OCR 状态在 30 秒内变为健康，推理计数持续增加。
- [ ] 接入第二台显示器后 5 秒内出现在控制台；断开后 5 秒内移除，其余显示器不受影响。
- [ ] 在系统设置中撤销授权后，对应显示器状态变为“不可用”，Agent 不退出；重新授权后恢复。
- [ ] 空闲时 Agent CPU 平均低于 3%（活动监视器 1 分钟平均）：
- [ ] 每个显示器的图像推理不超过 2 帧/秒，OCR 不超过 1 帧/秒。

## 5. 处置

- [ ] 前台浏览器显示测试用高风险图像（使用验收数据集，不保存截图）：3 秒内该浏览器被关闭，审计记录处置结果。
- [ ] 高风险内容在非前台应用的窗口中：不关闭任何应用（来源不可靠）。
- [ ] “始终允许”的应用显示高风险图像：不被关闭。
- [ ] 标题关键词命中：窗口出现后约 1 秒内被关闭。
- [ ] 同一应用 20 分钟内被关闭 5 次后进入封禁，重新打开即被终止；控制台列出封禁。
- [ ] 启用“保存事件证据”后，证据页出现条目，输入密码可查看；关闭后不再产生。
- [ ] root 进程、系统应用与 Karma 控制台自身不会被处置。

## 6. 卸载

- [ ] 错误密码：卸载取消，服务与 Agent 继续运行。
- [ ] 超级密码：服务退出且未被 launchd 重启，程序、plist、控制台被删除，数据目录保留。
- [ ] `--purge-data`：数据目录与日志一并删除。

## 7. 隐私

- [ ] 运行 30 分钟后，系统中不存在 Karma 生成的屏幕图像文件（`find / -newer <安装时间标记> -name '*.png' -o -name '*.jpg'` 只在证据目录内出现加密文件）。
- [ ] `/Library/Logs/Karma/service.log` 与 `log show` 中不出现窗口标题、网址、OCR 文本或分数。

## 8. 结论

- 通过 / 不通过：
- 问题与备注：
