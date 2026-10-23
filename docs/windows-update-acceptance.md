# Windows 0.2.4 password and update acceptance / 密码与升级验收

Status: **pending real Windows hardware evidence**. Cross-compilation and portable tests do not establish runtime acceptance. Record Windows build, account type, installed/offered versions, commands, outcomes, and audit event names; never record passwords, screenshots, browsing titles, or private keys.

状态：**待 Windows 真机证据**。交叉编译及便携测试不能替代运行时验收。记录系统版本、账户类型、当前/待升级版本、命令、结果和审计事件名，不记录密码、截图、浏览标题或私钥。

## Authorization / 授权

- [ ] Without a super password, existing administrator workflows still work. / 未设置超级密码时，原有管理员操作可用。
- [ ] First setup rejects incorrect administrator confirmation, fewer than 10 characters, or the same password; setting and changing survive restart. / 首次设置拒绝错误管理员确认、少于 10 个字符或相同密码；设置和修改跨重启保留。
- [ ] The supervisor keeps the super password. Ordinary administrator sessions can tighten rules, view evidence, and configure updates. / 监督人保管超级密码；普通管理员会话可加强规则、查看证据和设置更新。
- [ ] Direct authenticated IPC cannot disable any recognition switch or console launch at sign-in, raise the detection threshold, relax website/application/keyword/repeat rules, shorten/delete schedules, forget titles, or restore network access without the super password. Mixed tightening/weakening requests also require it. / 直接认证 IPC 不能绕过超级密码关闭识别或控制台登录后自动启动、提高识别阈值、放宽网站/应用/关键词/重复限制、缩短或删除时段、删除标题或恢复网络；混合加强/削弱也需超级密码。
- [ ] Wrong super passwords are rate-limited and audited without secrets. / 错误超级密码受到限速并写入不含秘密的审计。
- [ ] Changing the administrator password cannot make it equal to the super password. / 修改管理员密码不能使其与超级密码相同。

## Delayed uninstall / 延时卸载

- [ ] Correct super password authorizes immediate uninstall; the administrator password alone cannot. / 超级密码可立即卸载；普通管理员密码不能立即卸载。
- [ ] Administrator request shows a 72-hour countdown, survives reboot, and cannot be restarted to shorten the wait. / 管理员申请显示 72 小时倒计时，跨重启保留，不能重申请缩短等待。
- [ ] Before 72 hours admin uninstall fails; at 72 hours it succeeds; at 96 hours it fails and a new request is required. Use an isolated test fixture/build for boundary time control; do not alter a live protected device. / 72 小时前拒绝，72 小时起允许，96 小时起拒绝并需重新申请；仅在隔离测试构建中控制时间，不改动真实受保护设备。
- [ ] Cancel removes authorization immediately, survives restart, and is audited. Request, readiness, expiry, and uninstall approval produce audit records. / 取消立即失效且跨重启保留并审计；申请、生效、过期和卸载授权均有记录。
- [ ] An unreachable service aborts the new uninstall script before ACL changes, service deletion, WFP cleanup, or file removal. / 服务不可达时，新卸载脚本在改权限、删服务、清理 WFP 和删文件前中止。

## Upgrade and recovery / 升级与恢复

- [ ] Install v0.2.4 over v0.2.3 with its administrator password using the explicitly legacy uninstall/reinstall branch; verify policy, passwords, evidence and bans survive. / v0.2.3 使用明确的旧版卸载重装分支升到 v0.2.4，确认策略、密码、证据和封禁保留。
- [ ] For in-place testing, use a genuinely higher developer-signed test release. v0.2.4 over v0.2.4 and older signed bundles must fail. Do not alter the production signing key or publish dummy releases to test. / 原地升级测试需真正更高的开发者签名测试版；v0.2.4 同版本重装和已签名旧包都须失败；不修改生产签名密钥或发布虚假版本做测试。
- [ ] Setup copies to `%ProgramData%\Karma\update-inbox` and supplies only the administrator password. The service copies verified files to SYSTEM-only staging and checks again. Cancelling setup never removes either service. / 安装器复制至固定收件箱并仅提交管理员密码，服务复制到仅 SYSTEM 可访问的暂存区后再次校验；取消安装器不删除服务。
- [ ] Reject missing/bad signatures, altered files or SHA256SUMS, missing required executables, path traversal, symlinks/junctions, oversized ZIPs/files, duplicate/conflicting Windows paths, and manifest/version mismatches. / 拒绝缺失或伪造签名、文件或哈希表篡改、缺少程序、路径穿越、链接/联接、超大 ZIP/文件、Windows 重复冲突路径和版本不匹配。
- [ ] Mutate inbox files during staging; installed files must match the signed hashes or the update must abort. Administrators cannot write private staging. / 暂存期间修改收件箱文件，安装文件须匹配签名哈希或升级中止；管理员不可写私有暂存区。
- [ ] Auto installation defaults on, disabling it stops periodic installation, and Check now still works. No-update, network errors, and signature rejection appear in console status. / 自动安装默认开启，关闭后停止定期安装，立即检查仍可用；无更新、网络错误和签名拒绝均显示状态。
- [ ] Update with the console and Agent running. Verify file replacement, service and guardian restart, Agent reconnect, policy retention, new displayed version, and registry DisplayVersion. / 控制台和 Agent 运行时升级，验证文件替换、两服务重启、Agent 重连、策略保留、新版本显示和注册表版本。
- [ ] WFP restrictions persist through update restart; no uninstall path or WFP-clear action runs. / 更新重启期间 WFP 限制保留，不走卸载或清理网络限制路径。
- [ ] Inject locked-file, disk-full, failed service-start, and interrupted replacement failures in a disposable VM. Verify rollback/reboot recovery, both services registered/running, and no mixed release installed. / 在可丢弃虚拟机注入文件锁定、磁盘满、服务启动失败及替换中断，确认回滚/重启恢复、服务保留运行且无混合版本。
- [ ] Next successful startup cleans old files only after recovery has completed. An authenticated uninstall still works after an update hardened file ACLs. / 后续成功启动仅在恢复完成后清理旧文件；升级收紧文件权限后，授权卸载仍可进行。

This is user-mode protection. A technical Windows administrator can take ownership or use safe mode. Keep the Windows administrator account with the supervisor and use a standard account daily.

这是用户态保护。懂技术的 Windows 管理员可夺取所有权或使用安全模式绕过。建议监督人保管 Windows 管理员账户，日常使用标准账户。
