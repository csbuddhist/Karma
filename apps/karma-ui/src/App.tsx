import { FormEvent, ReactNode, useEffect, useMemo, useState } from "react";
import {
  Activity,
  AppWindow,
  Archive,
  CalendarClock,
  Check,
  ChevronRight,
  CircleAlert,
  Eye,
  EyeOff,
  FileClock,
  Fingerprint,
  Gauge,
  Image,
  KeyRound,
  LayoutDashboard,
  Lock,
  LockKeyhole,
  LogOut,
  Monitor,
  Plus,
  Save,
  Search,
  Settings,
  Shield,
  ShieldCheck,
  SlidersHorizontal,
  Trash2,
  WifiOff,
  X,
} from "lucide-react";
import {
  authStatus,
  defaultConsoleState,
  enroll,
  loadConsole,
  lock,
  revealEvidence,
  saveConsole,
  unlock,
} from "./bridge";
import type {
  ApplicationRule,
  AuthMode,
  ConsoleState,
  EvidenceItem,
  KeywordRule,
  PageKey,
  ScheduleRule,
} from "./types";

const navItems: Array<{ key: PageKey; label: string; icon: typeof LayoutDashboard }> = [
  { key: "overview", label: "总览", icon: LayoutDashboard },
  { key: "monitors", label: "显示器", icon: Monitor },
  { key: "recognition", label: "内容识别", icon: Gauge },
  { key: "keywords", label: "OCR 词库", icon: Search },
  { key: "applications", label: "应用管控", icon: AppWindow },
  { key: "schedule", label: "使用时段", icon: CalendarClock },
  { key: "evidence", label: "事件证据", icon: Image },
  { key: "audit", label: "审计日志", icon: FileClock },
  { key: "settings", label: "系统设置", icon: Settings },
];

const pageMeta: Record<PageKey, { title: string; subtitle: string }> = {
  overview: { title: "保护总览", subtitle: "查看设备状态、今日事件和关键防护能力" },
  monitors: { title: "显示器监控", subtitle: "每块屏幕独立采集、推理与报告健康状态" },
  recognition: { title: "内容识别", subtitle: "配置本地图像模型、OCR 与证据保留策略" },
  keywords: { title: "OCR 关键词", subtitle: "维护高风险、敏感与豁免上下文规则" },
  applications: { title: "应用管控", subtitle: "为浏览器、播放器和自定义程序设置行为" },
  schedule: { title: "使用时段", subtitle: "按星期和时间段限制网络及指定应用" },
  evidence: { title: "事件证据", subtitle: "浏览加密保存的高风险画面；原图需再次验证" },
  audit: { title: "审计日志", subtitle: "查看配置变更、认证和处置结果" },
  settings: { title: "系统设置", subtitle: "管理保护状态、数据保留和运行模式" },
};

function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (value: boolean) => void; label: string }) {
  return (
    <button className={`toggle ${checked ? "is-on" : ""}`} type="button" role="switch" aria-checked={checked} aria-label={label} onClick={() => onChange(!checked)}>
      <span />
    </button>
  );
}

function StatusPill({ tone, children }: { tone: "good" | "warn" | "muted" | "danger"; children: ReactNode }) {
  return <span className={`status-pill ${tone}`}><i />{children}</span>;
}

function Card({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <section className={`card ${className}`}>{children}</section>;
}

function PasswordGate({ mode, onAuthenticated }: { mode: Exclude<AuthMode, "unlocked">; onAuthenticated: (token: string) => void }) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [visible, setVisible] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    if (mode === "setup" && password !== confirm) return setError("两次输入的密码不一致");
    if (password.length < 10) return setError("管理员密码至少需要 10 个字符");
    setBusy(true);
    try {
      const token = mode === "setup" ? await enroll(password) : await unlock(password);
      onAuthenticated(token);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="auth-shell">
      <div className="auth-ambient ambient-one" />
      <div className="auth-ambient ambient-two" />
      <section className="auth-card">
        <div className="brand-lockup">
          <div className="brand-mark"><Shield /></div>
          <div><strong>KARMA</strong><span>家庭保护控制台</span></div>
        </div>
        <div className="auth-copy">
          <span className="eyebrow">{mode === "setup" ? "安全初始化" : "管理区域"}</span>
          <h1>{mode === "setup" ? "创建管理员密码" : "验证后查看设置"}</h1>
          <p>{mode === "setup" ? "该密码用于解锁控制台、修改策略和查看加密事件证据。密码不会以明文保存。" : "防护服务继续在后台运行。输入管理员密码后才能查看状态、事件或修改任何设置。"}</p>
        </div>
        <form onSubmit={submit} className="auth-form">
          <input className="sr-only" tabIndex={-1} autoComplete="username" value="karma-administrator" readOnly aria-hidden="true" />
          <label>
            <span>管理员密码</span>
            <div className="password-field">
              <KeyRound size={18} />
              <input autoFocus type={visible ? "text" : "password"} value={password} onChange={(event) => setPassword(event.target.value)} autoComplete={mode === "setup" ? "new-password" : "current-password"} />
              <button type="button" onClick={() => setVisible(!visible)} aria-label="显示或隐藏密码">{visible ? <EyeOff size={18} /> : <Eye size={18} />}</button>
            </div>
          </label>
          {mode === "setup" && (
            <label>
              <span>确认密码</span>
              <div className="password-field">
                <LockKeyhole size={18} />
                <input type={visible ? "text" : "password"} value={confirm} onChange={(event) => setConfirm(event.target.value)} autoComplete="new-password" />
              </div>
            </label>
          )}
          {error && <div className="form-error"><CircleAlert size={16} />{error}</div>}
          <button className="primary-button auth-submit" disabled={busy} type="submit">{busy ? "正在验证…" : mode === "setup" ? "完成安全设置" : "解锁控制台"}<ChevronRight size={18} /></button>
        </form>
        <div className="auth-footnote"><Fingerprint size={16} /><span>本地验证 · 无云端账户 · 不上传屏幕内容</span></div>
      </section>
    </main>
  );
}

function Overview({ state }: { state: ConsoleState }) {
  const healthyMonitors = state.monitors.filter((monitor) => monitor.state === "healthy").length;
  return (
    <div className="page-stack">
      {!state.serviceConnected && (
        <div className="notice warning"><WifiOff size={19} /><div><strong>Windows Service 尚未连接</strong><span>界面设置可保存，但系统级执行、截图证据和应用关闭要在 Service 接入后生效。</span></div></div>
      )}
      <div className="metric-grid">
        <Card className="metric-card primary"><div className="metric-icon"><ShieldCheck /></div><div><span>保护状态</span><strong>{state.protectionEnabled ? "已启用" : "已暂停"}</strong><small>{state.protectionEnabled ? "策略将在服务连接后执行" : "当前不执行保护动作"}</small></div></Card>
        <Card className="metric-card"><div className="metric-icon green"><Monitor /></div><div><span>活动显示器</span><strong>{state.monitors.length}</strong><small>{healthyMonitors} 块状态健康</small></div></Card>
        <Card className="metric-card"><div className="metric-icon amber"><Activity /></div><div><span>今日风险事件</span><strong>{state.evidence.length}</strong><small>{state.evidence.filter((item) => item.risk === "critical").length} 个严重事件</small></div></Card>
        <Card className="metric-card"><div className="metric-icon slate"><Archive /></div><div><span>证据保留</span><strong>{state.recognition.evidenceEnabled ? `${state.recognition.evidenceRetentionDays} 天` : "未启用"}</strong><small>原图加密，查看需复验</small></div></Card>
      </div>
      <div className="two-column">
        <Card>
          <div className="card-heading"><div><span className="eyebrow">SYSTEM</span><h2>组件状态</h2></div><StatusPill tone={state.serviceConnected ? "good" : "warn"}>{state.serviceConnected ? "运行正常" : "等待连接"}</StatusPill></div>
          <div className="health-list">
            <div><span className={`health-dot ${state.serviceConnected ? "online" : "offline"}`} /><div><strong>保护服务</strong><small>策略、认证、持久化与应用处置</small></div><b>{state.serviceConnected ? "在线" : "未实现"}</b></div>
            <div><span className={`health-dot ${state.agentConnected ? "online" : "offline"}`} /><div><strong>屏幕监控 Agent</strong><small>多屏采集、图像模型与 OCR</small></div><b>{state.agentConnected ? "在线" : "未连接"}</b></div>
            <div><span className="health-dot online" /><div><strong>管理控制台</strong><small>本地密码保护与策略编辑</small></div><b>在线</b></div>
          </div>
        </Card>
        <Card>
          <div className="card-heading"><div><span className="eyebrow">RECENT</span><h2>最近活动</h2></div></div>
          {state.audit.length ? <div className="compact-list">{state.audit.slice(0, 5).map((item) => <div key={item.id}><span className={`event-mark ${item.outcome}`} /><div><strong>{item.detail}</strong><small>{item.occurredAt}</small></div></div>)}</div> : <EmptyState icon={<FileClock />} title="暂无审计事件" text="认证、设置变更与处置结果将在这里显示。" />}
        </Card>
      </div>
    </div>
  );
}

function EmptyState({ icon, title, text, action }: { icon: ReactNode; title: string; text: string; action?: ReactNode }) {
  return <div className="empty-state"><div>{icon}</div><strong>{title}</strong><p>{text}</p>{action}</div>;
}

function Monitors({ state }: { state: ConsoleState }) {
  return <div className="monitor-grid">{state.monitors.length ? state.monitors.map((monitor) => <Card key={monitor.id} className="monitor-card"><div className="monitor-preview"><Monitor size={48} /><span>{monitor.resolution}</span></div><div className="monitor-info"><div><strong>{monitor.name}</strong><StatusPill tone={monitor.state === "healthy" ? "good" : monitor.state === "degraded" ? "warn" : "muted"}>{monitor.state === "healthy" ? "正常" : monitor.state === "degraded" ? "降级" : "离线"}</StatusPill></div><dl><div><dt>处理帧率</dt><dd>{monitor.fps.toFixed(1)} FPS</dd></div><div><dt>推理延迟</dt><dd>{monitor.latencyMs} ms</dd></div></dl></div></Card>) : <Card className="full-span"><EmptyState icon={<Monitor />} title="尚未收到显示器状态" text="连接 Windows Agent 后，此处会按显示器展示采集、模型和 OCR 健康信息。" /></Card>}</div>;
}

function Recognition({ state, update }: { state: ConsoleState; update: (state: ConsoleState) => void }) {
  const settings = state.recognition;
  const patch = (next: Partial<ConsoleState["recognition"]>) => update({ ...state, recognition: { ...settings, ...next } });
  return <div className="settings-grid">
    <Card><div className="setting-row"><div className="setting-icon violet"><Image /></div><div><strong>图像色情识别</strong><p>使用本地 ONNX 模型分析每块显示器的缩放帧。</p></div><Toggle checked={settings.imageEnabled} onChange={(value) => patch({ imageEnabled: value })} label="图像识别" /></div></Card>
    <Card><div className="setting-row"><div className="setting-icon blue"><Search /></div><div><strong>OCR 关键词识别</strong><p>识别简体中文、繁体中文和英文风险词。</p></div><Toggle checked={settings.ocrEnabled} onChange={(value) => patch({ ocrEnabled: value })} label="OCR识别" /></div></Card>
    <Card className="full-span"><div className="card-heading"><div><h2>风险灵敏度</h2><p>连续帧阈值越高，误报更少；越低，响应更积极。</p></div><span className="value-badge">{settings.sensitivity}%</span></div><input className="range" type="range" min="60" max="95" value={settings.sensitivity} onChange={(event) => patch({ sensitivity: Number(event.target.value) })} /><div className="range-labels"><span>更少误报</span><span>平衡</span><span>更积极</span></div></Card>
    <Card className="full-span evidence-setting"><div className="setting-row"><div className="setting-icon amber"><Archive /></div><div><strong>保存高风险事件证据</strong><p>仅在高风险处置时保存缩放画面；加密落盘，默认模糊显示。</p></div><Toggle checked={settings.evidenceEnabled} onChange={(value) => patch({ evidenceEnabled: value })} label="保存事件证据" /></div>{settings.evidenceEnabled && <div className="inline-setting"><label>自动删除期限</label><select value={settings.evidenceRetentionDays} onChange={(event) => patch({ evidenceRetentionDays: Number(event.target.value) })}><option value={1}>1 天</option><option value={3}>3 天</option><option value={7}>7 天</option><option value={14}>14 天</option><option value={30}>30 天</option></select><span>到期后由保护服务安全删除</span></div>}</Card>
  </div>;
}

function Keywords({ state, update }: { state: ConsoleState; update: (state: ConsoleState) => void }) {
  const [phrase, setPhrase] = useState("");
  const [category, setCategory] = useState<KeywordRule["category"]>("sensitive");
  function add() { if (!phrase.trim()) return; update({ ...state, keywords: [...state.keywords, { id: crypto.randomUUID(), phrase: phrase.trim(), category, enabled: true }] }); setPhrase(""); }
  return <Card><div className="toolbar"><div className="input-with-icon"><Search size={17} /><input placeholder="输入关键词或短语" value={phrase} onChange={(event) => setPhrase(event.target.value)} onKeyDown={(event) => event.key === "Enter" && add()} /></div><select value={category} onChange={(event) => setCategory(event.target.value as KeywordRule["category"])}><option value="high_risk">高风险</option><option value="sensitive">普通敏感</option><option value="exemption">豁免上下文</option></select><button className="primary-button" onClick={add}><Plus size={17} />添加规则</button></div>{state.keywords.length ? <div className="table"><div className="table-head"><span>关键词</span><span>类别</span><span>状态</span><span /></div>{state.keywords.map((rule) => <div className="table-row" key={rule.id}><strong>{rule.phrase}</strong><span><span className={`tag ${rule.category}`}>{rule.category === "high_risk" ? "高风险" : rule.category === "sensitive" ? "普通敏感" : "豁免"}</span></span><span><Toggle checked={rule.enabled} onChange={(enabled) => update({ ...state, keywords: state.keywords.map((item) => item.id === rule.id ? { ...item, enabled } : item) })} label={`启用${rule.phrase}`} /></span><button className="icon-button danger" onClick={() => update({ ...state, keywords: state.keywords.filter((item) => item.id !== rule.id) })}><Trash2 size={17} /></button></div>)}</div> : <EmptyState icon={<Search />} title="还没有自定义关键词" text="内置词库仍会工作。自定义规则在保护服务签名并发布后生效。" />}</Card>;
}

function Applications({ state, update }: { state: ConsoleState; update: (state: ConsoleState) => void }) {
  const [showAdd, setShowAdd] = useState(false);
  const [draft, setDraft] = useState({ name: "", executable: "", category: "custom" as ApplicationRule["category"] });
  function add() { if (!draft.name.trim() || !draft.executable.trim()) return; update({ ...state, applications: [...state.applications, { id: crypto.randomUUID(), ...draft, action: "content_only", enabled: true }] }); setShowAdd(false); setDraft({ name: "", executable: "", category: "custom" }); }
  return <div className="page-stack"><div className="page-actions"><button className="primary-button" onClick={() => setShowAdd(true)}><Plus size={17} />添加应用</button></div><Card>{state.applications.map((rule) => <div className="app-rule" key={rule.id}><div className="app-avatar"><AppWindow /></div><div className="app-main"><strong>{rule.name}</strong><small>{rule.executable}</small></div><select value={rule.action} onChange={(event) => update({ ...state, applications: state.applications.map((item) => item.id === rule.id ? { ...item, action: event.target.value as ApplicationRule["action"] } : item) })}><option value="content_only">仅色情内容处置</option><option value="block">始终禁止</option><option value="allow">允许使用</option></select><Toggle checked={rule.enabled} onChange={(enabled) => update({ ...state, applications: state.applications.map((item) => item.id === rule.id ? { ...item, enabled } : item) })} label={`启用${rule.name}`} /></div>)}</Card>{showAdd && <Modal title="添加受控应用" onClose={() => setShowAdd(false)}><label className="field"><span>显示名称</span><input value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} placeholder="例如：Chrome" /></label><label className="field"><span>可执行文件</span><input value={draft.executable} onChange={(event) => setDraft({ ...draft, executable: event.target.value })} placeholder="chrome.exe 或完整路径" /></label><label className="field"><span>应用类别</span><select value={draft.category} onChange={(event) => setDraft({ ...draft, category: event.target.value as ApplicationRule["category"] })}><option value="browser">浏览器</option><option value="player">播放器</option><option value="game">游戏</option><option value="custom">自定义</option></select></label><button className="primary-button modal-submit" onClick={add}>保存应用规则</button></Modal>}</div>;
}

const dayLabels = ["一", "二", "三", "四", "五", "六", "日"];
function Schedule({ state, update }: { state: ConsoleState; update: (state: ConsoleState) => void }) {
  const [draft, setDraft] = useState<Omit<ScheduleRule, "id">>({ name: "学习时段", days: [0, 1, 2, 3, 4], start: "21:00", end: "07:00", target: "browser_player", enabled: true });
  function add() { update({ ...state, schedules: [...state.schedules, { ...draft, id: crypto.randomUUID() }] }); }
  return <div className="two-column schedule-layout"><Card><div className="card-heading"><div><h2>新增限制时段</h2><p>时间使用设备本地时区，支持跨午夜。</p></div></div><label className="field"><span>规则名称</span><input value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} /></label><div className="field"><span>生效星期</span><div className="day-picker">{dayLabels.map((label, index) => <button className={draft.days.includes(index) ? "active" : ""} key={label} onClick={() => setDraft({ ...draft, days: draft.days.includes(index) ? draft.days.filter((day) => day !== index) : [...draft.days, index].sort() })}>{label}</button>)}</div></div><div className="time-grid"><label className="field"><span>开始</span><input type="time" step="900" value={draft.start} onChange={(event) => setDraft({ ...draft, start: event.target.value })} /></label><label className="field"><span>结束</span><input type="time" step="900" value={draft.end} onChange={(event) => setDraft({ ...draft, end: event.target.value })} /></label></div><label className="field"><span>限制范围</span><select value={draft.target} onChange={(event) => setDraft({ ...draft, target: event.target.value as ScheduleRule["target"] })}><option value="browser_player">浏览器和播放器</option><option value="internet">上网应用</option><option value="all_restricted">全部受控应用</option></select></label><button className="primary-button modal-submit" onClick={add}><Plus size={17} />添加时段</button></Card><Card><div className="card-heading"><div><h2>已配置时段</h2><p>{state.schedules.length} 条策略</p></div></div>{state.schedules.length ? <div className="schedule-list">{state.schedules.map((rule) => <div key={rule.id}><div className="schedule-time"><strong>{rule.start}</strong><span>至</span><strong>{rule.end}</strong></div><div className="schedule-info"><strong>{rule.name}</strong><small>{rule.days.map((day) => `周${dayLabels[day]}`).join(" · ")}</small></div><Toggle checked={rule.enabled} onChange={(enabled) => update({ ...state, schedules: state.schedules.map((item) => item.id === rule.id ? { ...item, enabled } : item) })} label={`启用${rule.name}`} /><button className="icon-button danger" onClick={() => update({ ...state, schedules: state.schedules.filter((item) => item.id !== rule.id) })}><Trash2 size={16} /></button></div>)}</div> : <EmptyState icon={<CalendarClock />} title="还没有限制时段" text="在左侧创建第一条每周使用规则。" />}</Card></div>;
}

function Evidence({ state, sessionToken }: { state: ConsoleState; sessionToken: string }) {
  const [selected, setSelected] = useState<EvidenceItem | null>(null);
  const [password, setPassword] = useState("");
  const [imageUrl, setImageUrl] = useState<string>();
  const [error, setError] = useState("");
  async function reveal() { if (!selected) return; setError(""); try { setImageUrl(await revealEvidence(sessionToken, selected.id, password)); } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); } }
  return <div className="page-stack"><div className="privacy-banner"><Lock size={18} /><span><strong>受保护的事件证据</strong> 缩略图默认模糊，原图每次查看都需要重新输入管理员密码。截图不会上传。</span></div>{state.evidence.length ? <div className="evidence-grid">{state.evidence.map((item) => <button className="evidence-card" key={item.id} onClick={() => { setSelected(item); setImageUrl(undefined); setPassword(""); }}><div className="evidence-image">{item.thumbnailUrl ? <img src={item.thumbnailUrl} alt="已模糊的事件证据" /> : <Image />}<span><Eye size={16} />验证后查看</span></div><div><StatusPill tone={item.risk === "critical" ? "danger" : "warn"}>{item.risk === "critical" ? "严重" : "高风险"}</StatusPill><strong>{item.applicationName}</strong><small>{item.capturedAt} · {item.monitorName}</small></div></button>)}</div> : <Card><EmptyState icon={<Lock />} title="没有已保存的事件证据" text={state.recognition.evidenceEnabled ? "连接保护服务后，高风险处置画面会加密保存并在这里显示。" : "当前未启用证据保存。可在“内容识别”中开启并设置自动删除期限。"} /></Card>}{selected && <Modal title="查看事件证据" onClose={() => setSelected(null)} wide>{imageUrl ? <img className="revealed-image" src={imageUrl} alt="高风险事件证据" /> : <div className="reveal-panel"><div className="locked-preview"><LockKeyhole /></div><strong>{selected.reason}</strong><p>{selected.capturedAt} · {selected.applicationName} · {selected.monitorName}</p><label className="field"><span>再次输入管理员密码</span><input type="password" value={password} onChange={(event) => setPassword(event.target.value)} onKeyDown={(event) => event.key === "Enter" && reveal()} autoFocus /></label>{error && <div className="form-error"><CircleAlert size={16} />{error}</div>}<button className="primary-button modal-submit" onClick={reveal}><Eye size={17} />解密并查看原图</button></div>}</Modal>}</div>;
}

function Audit({ state }: { state: ConsoleState }) {
  return <Card>{state.audit.length ? <div className="audit-list">{state.audit.map((item) => <div key={item.id}><span className={`audit-icon ${item.outcome}`}>{item.outcome === "success" ? <Check /> : <CircleAlert />}</span><div><strong>{item.detail}</strong><small>{item.kind} · {item.occurredAt}</small></div><StatusPill tone={item.outcome === "success" ? "good" : item.outcome === "warning" ? "warn" : "danger"}>{item.outcome === "success" ? "成功" : item.outcome === "warning" ? "警告" : "拒绝"}</StatusPill></div>)}</div> : <EmptyState icon={<FileClock />} title="暂无审计日志" text="管理认证、策略变更、应用处置和服务故障会记录到这里。" />}</Card>;
}

function SettingsPage({ state, update }: { state: ConsoleState; update: (state: ConsoleState) => void }) {
  return <div className="settings-grid"><Card className="full-span"><div className="setting-row"><div className="setting-icon green"><ShieldCheck /></div><div><strong>总保护开关</strong><p>暂停需要管理员认证；服务接入后，此设置控制所有监控和处置。</p></div><Toggle checked={state.protectionEnabled} onChange={(value) => update({ ...state, protectionEnabled: value })} label="总保护" /></div></Card><Card><div className="card-heading"><div><h2>保护能力</h2><p>当前仓库实现进度</p></div></div><div className="capability-list"><div><Check />多显示器采集</div><div><Check />本地 ONNX 图像识别</div><div><Check />OCR 与关键词摘要</div><div className="pending"><CircleAlert />Windows Service 待接入</div><div className="pending"><CircleAlert />进程处置待接入</div><div className="pending"><CircleAlert />加密截图写入待接入</div></div></Card><Card><div className="card-heading"><div><h2>安全说明</h2><p>家庭家长控制边界</p></div></div><div className="security-note"><Lock /><p>控制台密码不能替代 Windows 账户隔离。若孩子拥有本机管理员权限，仍可能通过系统级手段绕过未完成的服务保护。</p></div></Card></div>;
}

function Modal({ title, children, onClose, wide = false }: { title: string; children: ReactNode; onClose: () => void; wide?: boolean }) {
  return <div className="modal-backdrop" onMouseDown={onClose}><section className={`modal ${wide ? "wide" : ""}`} onMouseDown={(event) => event.stopPropagation()}><div className="modal-head"><h2>{title}</h2><button className="icon-button" onClick={onClose}><X /></button></div>{children}</section></div>;
}

export function App() {
  const [authMode, setAuthMode] = useState<AuthMode>("locked");
  const [sessionToken, setSessionToken] = useState("");
  const [page, setPage] = useState<PageKey>("overview");
  const [state, setState] = useState<ConsoleState>(defaultConsoleState);
  const [loading, setLoading] = useState(true);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState("");

  useEffect(() => { authStatus().then((status) => setAuthMode(status)).finally(() => setLoading(false)); }, []);
  useEffect(() => {
    if (authMode !== "unlocked" || !sessionToken) return;
    let timeout = window.setTimeout(() => void signOut(), 15 * 60 * 1000);
    const reset = () => {
      window.clearTimeout(timeout);
      timeout = window.setTimeout(() => void signOut(), 15 * 60 * 1000);
    };
    const events: Array<keyof WindowEventMap> = ["pointerdown", "keydown", "wheel"];
    events.forEach((eventName) => window.addEventListener(eventName, reset, { passive: true }));
    return () => {
      window.clearTimeout(timeout);
      events.forEach((eventName) => window.removeEventListener(eventName, reset));
    };
  }, [authMode, sessionToken]);
  async function authenticated(token: string) { setSessionToken(token); setLoading(true); try { setState(await loadConsole(token)); setAuthMode("unlocked"); } finally { setLoading(false); } }
  function update(next: ConsoleState) { setState(next); setDirty(true); setSaveMessage(""); }
  async function save() { setSaving(true); try { await saveConsole(sessionToken, state); setDirty(false); setSaveMessage("设置已保存"); window.setTimeout(() => setSaveMessage(""), 2200); } finally { setSaving(false); } }
  async function signOut() { await lock(sessionToken); setSessionToken(""); setState(defaultConsoleState); setAuthMode("locked"); setPage("overview"); }

  const content = useMemo(() => {
    if (page === "overview") return <Overview state={state} />;
    if (page === "monitors") return <Monitors state={state} />;
    if (page === "recognition") return <Recognition state={state} update={update} />;
    if (page === "keywords") return <Keywords state={state} update={update} />;
    if (page === "applications") return <Applications state={state} update={update} />;
    if (page === "schedule") return <Schedule state={state} update={update} />;
    if (page === "evidence") return <Evidence state={state} sessionToken={sessionToken} />;
    if (page === "audit") return <Audit state={state} />;
    return <SettingsPage state={state} update={update} />;
  }, [page, state, sessionToken]);

  if (loading) return <div className="loading-screen"><div className="brand-mark"><Shield /></div><span>正在打开安全控制台…</span></div>;
  if (authMode !== "unlocked") return <PasswordGate mode={authMode} onAuthenticated={authenticated} />;
  return <div className="app-shell"><aside className="sidebar"><div className="brand-lockup sidebar-brand"><div className="brand-mark"><Shield /></div><div><strong>KARMA</strong><span>家庭保护</span></div></div><nav>{navItems.map((item) => { const Icon = item.icon; return <button className={page === item.key ? "active" : ""} key={item.key} onClick={() => setPage(item.key)}><Icon size={19} /><span>{item.label}</span>{item.key === "evidence" && state.evidence.length > 0 && <b>{state.evidence.length}</b>}</button>; })}</nav><div className="sidebar-foot"><div className="mini-health"><span className={state.serviceConnected ? "online" : "offline"} /><div><strong>{state.serviceConnected ? "保护服务在线" : "服务尚未连接"}</strong><small>{state.agentConnected ? "Agent 正常运行" : "仅控制台模式"}</small></div></div><button onClick={signOut}><LogOut size={18} />锁定控制台</button></div></aside><main className="main"><header><div><span className="eyebrow">KARMA CONTROL</span><h1>{pageMeta[page].title}</h1><p>{pageMeta[page].subtitle}</p></div><div className="header-actions">{saveMessage && <span className="saved-message"><Check size={15} />{saveMessage}</span>}<button className="secondary-button" onClick={signOut}><Lock size={16} />锁定</button><button className="primary-button" disabled={!dirty || saving} onClick={save}><Save size={17} />{saving ? "保存中…" : "保存设置"}</button></div></header><div className="page-content">{content}</div></main></div>;
}
