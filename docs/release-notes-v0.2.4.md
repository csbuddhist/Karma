# English

Karma v0.2.4 is a Windows x64 **prerelease for testing**. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

- Adds an optional supervisor-held super password, at least 10 characters and different from the administrator password. The service independently detects weakening policy changes and requires the super password for them, deleting remembered titles, early network restore, and changing the super password. Tightening protection uses the administrator session. Without a super password, existing behavior remains.
- Adds audited, cancellable uninstall requests: the administrator password starts a 72-hour wait, followed by a 24-hour administrator-only removal window. The super password authorizes immediate uninstall. This is also the recovery path for a forgotten super password. Requests survive restarts; expired requests must be renewed. The new uninstall script aborts if the service cannot authorize removal.
- Adds signed in-place upgrades and automatic update checks. Automatic installation defaults on; System Settings has Check now and shows progress/failures. Both setup and automatic updates require the embedded developer key's Ed25519 manifest signature, per-file SHA-256, and a strictly higher version. The service revalidates SYSTEM-only staged copies, replaces files with rollback/recovery, and restarts both protection services without unregistering them. Same-version reinstall and downgrade are rejected. v0.2.3 and earlier still use the legacy password-authorized uninstall/reinstall migration.
- Adds console settings for the super password, uninstall countdown/cancellation, installed version, and automatic updates. Pending policy edits survive refresh and authorization cancellation.

Download `Karma-windows-x64-test-v0.2.4-setup.exe` for installation, or the ZIP for diagnostics. Setup embeds the official Microsoft Visual C++ x64 Redistributable and checks its Microsoft signature before installing it when needed. Existing local policy and evidence data are retained by default.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Karma remains user-mode protection: a technical Windows administrator can take ownership, use safe mode, or alter the system offline. Have the supervisor keep the Windows administrator account and use a standard account daily.

Portable Rust tests, formatting, Clippy, Windows-target cross-checks, frontend build, installer/bundle contracts, and manifest verification are the development gates. Real Windows testing must still cover two-level authorization, uninstall time boundaries, live in-place upgrades, locked files, interruption/rollback, WFP persistence, service/guardian/Agent recovery, and post-upgrade uninstall. See [the acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.2.4/docs/windows-update-acceptance.md).

# 简体中文

Karma v0.2.4 是 Windows x64 **测试预发布版**。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

- 新增可选的监督人超级密码，至少 10 个字符且与管理员密码不同。服务独立判断策略是否削弱保护，削弱保护、删除已记住标题、提前恢复网络及修改超级密码均需超级密码。加强保护使用普通管理员会话；未设置超级密码时保持原有行为。
- 新增可取消、带审计的卸载申请：管理员密码提交申请后等待 72 小时，在随后的 24 小时内可仅用管理员密码卸载；超级密码可立即卸载。这也是忘记超级密码的恢复途径。申请跨重启保留，过期需重新申请。新卸载脚本在服务无法授权时中止。
- 新增签名原地升级和自动更新检查。自动安装默认开启，系统设置可立即检查并查看进度/失败原因。安装器和自动更新都必须通过内置开发者公钥的 Ed25519 清单签名、逐文件 SHA-256 和严格更高版本校验。服务在仅 SYSTEM 可访问的暂存区复验文件，带回滚/恢复地替换程序，再重启两个保护服务，全程不注销服务。拒绝同版本重装与降级。v0.2.3 及更早版本仍使用密码授权卸载后重装的迁移流程。
- 控制台增加超级密码、卸载倒计时/取消、当前版本及自动更新设置。刷新和取消授权不会丢失尚未保存的策略编辑。

安装请下载 `Karma-windows-x64-test-v0.2.4-setup.exe`，诊断可使用 ZIP。安装器内置微软官方 VC++ x64 运行库，仅在需要时校验微软签名后安装。默认保留现有本地策略和证据数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。Karma 仍是用户态保护：懂技术的 Windows 管理员可夺取所有权、进入安全模式或离线修改系统。建议监督人保管 Windows 管理员账户，日常使用标准账户。

开发门禁包括 Rust 便携测试、格式、Clippy、Windows 目标交叉检查、前端构建、安装器/测试包契约及签名验证。Windows 真机仍需验证两级授权、卸载时间边界、运行中原地升级、文件锁定、中断/回滚、WFP 持续限制、服务/守护服务/Agent 恢复及升级后卸载。详见[验收清单](https://github.com/csbuddhist/Karma/blob/v0.2.4/docs/windows-update-acceptance.md)。
