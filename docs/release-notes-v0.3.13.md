# English

Karma v0.3.13 is a Windows x64 **prerelease for testing** that adds application types and turns network schedules into blocking schedules that can also block applications from running. Windows hardware acceptance is pending; EXEs and setup still lack Authenticode signatures and may trigger SmartScreen.

**Application types**

- Applications → Application types lists the built-in Browser, Media player, Game, and Other categories and lets you add, rename, and delete your own (such as “Chat apps”).
- **Manage applications** adds programs to a category from the applications installed on the computer, or by entering an executable, and removes them again. A program that already has a rule moves into the category and keeps its action; a new one uses “Explicit content only”. Each rule in the application list also has a category selector.
- A category used by a blocking rule cannot be deleted; deleting a category moves its programs to Other.

**Network Schedules become Blocking Schedules (断网屏蔽)**

- Each rule chooses an action and targets: **Block network access** (Windows WFP filters, as before) or **Block running**, for one or more application categories, selected applications, or all networked applications (network blocking only).
- Block running: during a blocked period the protection Service ends a covered program, in every signed-in session, within about 2 seconds of it running, and again whenever it is relaunched. Unsaved work in that program is lost. Programs in the Windows directory or Karma's install directory are never ended. Each program is recorded once per period in the audit log.
- Rules saved by earlier versions with the Browsers target are migrated to the Browser category and enforced exactly as before.
- With a super password set, moving a program out of a blocked category or changing a rule from blocking running to blocking network access requires the super password, like any other weakening change. Lifting a block early lifts both network and launch blocking until the current period ends.

**Upgrading**

- v0.3.12 can update to this version from the console or automatically. v0.3.11 and earlier must run `Karma-windows-x64-test-v0.3.13-setup.exe` once and enter the administrator password. Existing local policy, evidence, and audit data are retained.

The Ed25519 signature authenticates upgrade contents; it does not replace Windows Authenticode. Karma remains user-mode protection: a technical Windows administrator can take ownership, use safe mode, or alter the system offline. Have the supervisor keep the Windows administrator account and use a standard account daily.

Real Windows testing must still cover the items in [the network enforcement acceptance checklist](https://github.com/csbuddhist/Karma/blob/v0.3.13/docs/windows-network-enforcement-acceptance.md), including the new custom-category, block-running, exemption, and lift cases.

# 简体中文

Karma v0.3.13 是 Windows x64 **测试预发布版**，新增应用类型管理，并把断网时段改为可同时禁止应用启动的断网屏蔽。Windows 真机验收仍待完成；EXE 和安装器仍无 Authenticode 签名，可能触发 SmartScreen。

**应用类型**

- “应用管控 → 应用类型”列出内置的浏览器、播放器、游戏和其他类别，并可新增、重命名和删除自定义类别（如“聊天软件”）。
- **管理应用**可从本机已安装的应用中选择或手动填写可执行文件，把程序加入类别或移出类别。已有规则的程序移入类别后保留原处置方式；新加入的程序为“仅色情内容处置”。应用列表中的每条规则也可直接选择类别。
- 正被屏蔽规则使用的类别不能删除；删除类别后，其中的程序归入“其他”。

**断网时段改为断网屏蔽**

- 每条规则可选择屏蔽方式和屏蔽对象：**禁止联网**（与之前相同，由 Windows WFP 过滤器执行）或**禁止启动**；对象为一个或多个应用类别、指定软件或全部联网应用（仅限禁止联网）。
- 禁止启动：屏蔽时段内，保护服务会在被屏蔽程序运行后约 2 秒内将其结束（覆盖所有登录会话），重新打开会再次被结束，程序中未保存的内容会丢失。Windows 目录和 Karma 安装目录中的程序永远不会被结束。每个程序在每个时段只写入一条审计记录。
- 早期版本以“浏览器软件”为对象保存的规则会自动迁移为“浏览器”类别，执行效果不变。
- 设置超级密码后，把程序移出被屏蔽的类别，或把规则从“禁止启动”改为“禁止联网”，与其他削弱保护的修改一样需要超级密码。提前解除屏蔽会同时解除联网和启动限制，直到当前时段结束。

**升级**

- v0.3.12 可通过控制台或自动更新升级到此版本。v0.3.11 及更早版本需先运行一次 `Karma-windows-x64-test-v0.3.13-setup.exe` 并输入管理员密码。默认保留现有本地策略、证据和审计数据。

Ed25519 签名验证升级内容，不能替代 Windows Authenticode。Karma 仍是用户态保护：懂技术的 Windows 管理员可夺取所有权、进入安全模式或离线修改系统。建议监督人保管 Windows 管理员账户，日常使用标准账户。

Windows 真机仍需完成[网络执行验收清单](https://github.com/csbuddhist/Karma/blob/v0.3.13/docs/windows-network-enforcement-acceptance.md)中的项目，包括新增的自定义类别、禁止启动、豁免和解除场景。
