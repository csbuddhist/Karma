# English

Karma v0.4.1 is a Windows x64 **prerelease for testing** that reopens the console after an update and also checks the Gitee mirror for updates. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**The console reopens after an update**

- Updating has to close the console to replace its files, and until now it stayed closed after a successful update. Once the update is applied (or rolled back), the service now reopens the console for the signed-in user.
- This already works for the update to this version: the newly installed service reopens the console itself. If the console is already open again or nobody is signed in, nothing is started.

**Updates are also checked on Gitee**

- Besides GitHub Releases, the service now checks the [Gitee mirror](https://gitee.com/karmakarma/Karma/releases), which is useful where GitHub is slow or unreachable. The newest version found on either source is installed; that version is downloaded from GitHub first and from Gitee if the download fails.
- Packages from Gitee pass exactly the same checks as packages from GitHub: the Ed25519 manifest signature, per-file hashes, and a strictly higher version.
- The version you upgrade from performs the check, so Gitee is used from the next update after v0.4.1 is installed.

**Upgrading**

- v0.3.12 through v0.4.0 can update to this version from the console or automatically. v0.3.11 and earlier must run `Karma-windows-x64-test-v0.4.1-setup.exe` once and enter the administrator password. Existing local policy, evidence, and audit data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the update acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.4.1/docs/windows-update-acceptance.md) and [the ONNX inference acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.4.1/docs/windows-onnx-acceptance.md).

# 简体中文

Karma v0.4.1 是 Windows x64 **测试预发布版**，升级后会自动重新打开控制台，并且也会从 Gitee 镜像检查更新。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**升级后自动重新打开控制台**

- 升级时必须关闭控制台才能替换文件，此前升级成功后控制台一直保持关闭。现在升级完成（或回滚）后，服务会为当前登录的用户重新打开控制台。
- 升级到本版本时即已生效：新安装的服务会自行重新打开控制台。如果控制台已被重新打开或当前无人登录，则不会再启动。

**也从 Gitee 检查更新**

- 除 GitHub Releases 外，服务现在也会检查 [Gitee 镜像](https://gitee.com/karmakarma/Karma/releases)，适合 GitHub 访问缓慢或无法访问的网络。两个来源中版本最高者会被安装；该版本先从 GitHub 下载，下载失败时改用 Gitee。
- 来自 Gitee 的升级包与 GitHub 的完全一样要通过校验：Ed25519 清单签名、逐文件哈希和严格更高的版本号。
- 检查更新由升级前的版本执行，因此安装 v0.4.1 之后的下一次升级才会用到 Gitee。

**升级**

- v0.3.12 至 v0.4.0 可通过控制台或自动更新升级到此版本。v0.3.11 及更早版本需先运行一次 `Karma-windows-x64-test-v0.4.1-setup.exe` 并输入管理员密码。默认保留现有本地策略、证据和审计数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成 [升级验收清单](https://github.com/csbuddhist/Karma/blob/v0.4.1/docs/windows-update-acceptance.md) 和 [ONNX 推理验收清单](https://github.com/csbuddhist/Karma/blob/v0.4.1/docs/windows-onnx-acceptance.md)中的项目。
