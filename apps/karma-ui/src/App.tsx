import {
  createContext,
  FormEvent,
  ReactNode,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
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
  Globe2,
  Image,
  KeyRound,
  LayoutDashboard,
  Lock,
  LockKeyhole,
  LogOut,
  Monitor,
  Power,
  Plus,
  Save,
  Search,
  Settings,
  Trash2,
  WifiOff,
  X,
} from "lucide-react";
import {
  authStatus,
  changePassword,
  configureLaunchAtStartup,
  defaultConsoleState,
  enroll,
  loadConsole,
  lock,
  revealEvidence,
  saveConsole,
  unlock,
} from "./bridge";
import {
  createTranslator,
  getInitialLocale,
  isSessionExpiredError,
  localizeError,
  persistLocale,
} from "./i18n";
import type { Locale, MessageKey, Translate } from "./i18n";
import type {
  ApplicationRule,
  AuthMode,
  ConsoleState,
  EvidenceItem,
  KeywordRule,
  PageKey,
  ScheduleRule,
  WebsiteRule,
} from "./types";

type I18nContextValue = {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: Translate;
};

const I18nContext = createContext<I18nContextValue | null>(null);

function KarmaShieldIcon({ size = 24 }: { size?: number }) {
  return (
    <svg
      aria-hidden="true"
      fill="none"
      height={size}
      viewBox="0 0 24 24"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M20 13c0 5-3.5 7.5-8 9-4.5-1.5-8-4-8-9V5l8-3 8 3v8Z"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
      <path
        d="M9.5 7.5v9m0-4.5 5-4.5m-5 4.5 5 4.5"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
    </svg>
  );
}

const navItems: Array<{
  key: PageKey;
  label: MessageKey;
  icon: typeof LayoutDashboard;
}> = [
  { key: "overview", label: "nav.overview", icon: LayoutDashboard },
  { key: "monitors", label: "nav.monitors", icon: Monitor },
  { key: "recognition", label: "nav.recognition", icon: Gauge },
  { key: "keywords", label: "nav.keywords", icon: Search },
  { key: "websites", label: "nav.websites", icon: Globe2 },
  { key: "applications", label: "nav.applications", icon: AppWindow },
  { key: "schedule", label: "nav.schedule", icon: CalendarClock },
  { key: "evidence", label: "nav.evidence", icon: Image },
  { key: "audit", label: "nav.audit", icon: FileClock },
  { key: "settings", label: "nav.settings", icon: Settings },
];

const pageMeta: Record<PageKey, { title: MessageKey; subtitle: MessageKey }> = {
  overview: { title: "page.overview.title", subtitle: "page.overview.subtitle" },
  monitors: { title: "page.monitors.title", subtitle: "page.monitors.subtitle" },
  recognition: { title: "page.recognition.title", subtitle: "page.recognition.subtitle" },
  keywords: { title: "page.keywords.title", subtitle: "page.keywords.subtitle" },
  websites: { title: "page.websites.title", subtitle: "page.websites.subtitle" },
  applications: { title: "page.applications.title", subtitle: "page.applications.subtitle" },
  schedule: { title: "page.schedule.title", subtitle: "page.schedule.subtitle" },
  evidence: { title: "page.evidence.title", subtitle: "page.evidence.subtitle" },
  audit: { title: "page.audit.title", subtitle: "page.audit.subtitle" },
  settings: { title: "page.settings.title", subtitle: "page.settings.subtitle" },
};

function useI18n(): I18nContextValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error("I18nContext is not available");
  return value;
}

function LanguageSelect({ compact = false }: { compact?: boolean }) {
  const { locale, setLocale, t } = useI18n();
  return (
    <label className={`language-select ${compact ? "compact" : ""}`}>
      {compact && <Globe2 size={15} />}
      {!compact && <span>{t("language.label")}</span>}
      <select
        aria-label={t("language.label")}
        value={locale}
        onChange={(event) => setLocale(event.target.value as Locale)}
      >
        <option value="zh-CN">{t("language.chinese")}</option>
        <option value="en">{t("language.english")}</option>
      </select>
    </label>
  );
}

function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
}) {
  return (
    <button
      className={`toggle ${checked ? "is-on" : ""}`}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
    >
      <span />
    </button>
  );
}

function StatusPill({
  tone,
  children,
}: {
  tone: "good" | "warn" | "muted" | "danger";
  children: ReactNode;
}) {
  return <span className={`status-pill ${tone}`}><i />{children}</span>;
}

function Card({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <section className={`card ${className}`}>{children}</section>;
}

function PasswordGate({
  mode,
  onAuthenticated,
}: {
  mode: Exclude<AuthMode, "unlocked">;
  onAuthenticated: (token: string) => void;
}) {
  const { t } = useI18n();
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [visible, setVisible] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    if (mode === "setup" && password !== confirm) return setError(t("auth.passwordMismatch"));
    if (password.length < 10) return setError(t("auth.passwordTooShort"));
    setBusy(true);
    try {
      const token = mode === "setup" ? await enroll(password) : await unlock(password);
      onAuthenticated(token);
    } catch (reason) {
      setError(localizeError(reason, t));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="auth-shell">
      <div className="auth-ambient ambient-one" />
      <div className="auth-ambient ambient-two" />
      <div className="auth-language"><LanguageSelect compact /></div>
      <section className="auth-card">
        <div className="brand-lockup">
          <div className="brand-mark"><KarmaShieldIcon /></div>
          <div><strong>KARMA</strong><span>{t("brand.console")}</span></div>
        </div>
        <div className="auth-copy">
          <span className="eyebrow">{t(mode === "setup" ? "auth.setupEyebrow" : "auth.lockedEyebrow")}</span>
          <h1>{t(mode === "setup" ? "auth.setupTitle" : "auth.lockedTitle")}</h1>
          <p>{t(mode === "setup" ? "auth.setupDescription" : "auth.lockedDescription")}</p>
        </div>
        <form onSubmit={submit} className="auth-form">
          <input className="sr-only" tabIndex={-1} autoComplete="username" value="karma-administrator" readOnly aria-hidden="true" />
          <label>
            <span>{t("auth.password")}</span>
            <div className="password-field">
              <KeyRound size={18} />
              <input autoFocus type={visible ? "text" : "password"} value={password} onChange={(event) => setPassword(event.target.value)} autoComplete={mode === "setup" ? "new-password" : "current-password"} />
              <button type="button" onClick={() => setVisible(!visible)} aria-label={t("auth.togglePassword")}>
                {visible ? <EyeOff size={18} /> : <Eye size={18} />}
              </button>
            </div>
          </label>
          {mode === "setup" && (
            <label>
              <span>{t("auth.confirmPassword")}</span>
              <div className="password-field">
                <LockKeyhole size={18} />
                <input type={visible ? "text" : "password"} value={confirm} onChange={(event) => setConfirm(event.target.value)} autoComplete="new-password" />
              </div>
            </label>
          )}
          {error && <div className="form-error"><CircleAlert size={16} />{error}</div>}
          <button className="primary-button auth-submit" disabled={busy} type="submit">
            {t(busy ? "auth.verifying" : mode === "setup" ? "auth.finishSetup" : "auth.unlock")}<ChevronRight size={18} />
          </button>
        </form>
        <div className="auth-footnote"><Fingerprint size={16} /><span>{t("auth.footnote")}</span></div>
      </section>
    </main>
  );
}

function ServiceConnectionError({ detail, onRetry }: { detail: string; onRetry: () => void }) {
  const { t } = useI18n();
  return (
    <main className="auth-shell">
      <div className="auth-language"><LanguageSelect compact /></div>
      <section className="auth-card connection-error-card">
        <div className="brand-lockup">
          <div className="brand-mark"><KarmaShieldIcon /></div>
          <div><strong>KARMA</strong><span>{t("brand.console")}</span></div>
        </div>
        <div className="connection-error-icon"><WifiOff /></div>
        <div className="auth-copy">
          <span className="eyebrow">{t("connection.eyebrow")}</span>
          <h1>{t("connection.title")}</h1>
          <p>{t("connection.description")}</p>
        </div>
        <div className="form-error"><CircleAlert size={16} /><span>{detail}</span></div>
        <button className="primary-button auth-submit" type="button" onClick={onRetry}>{t("connection.retry")}<ChevronRight size={18} /></button>
      </section>
    </main>
  );
}

function Overview({ state }: { state: ConsoleState }) {
  const { t } = useI18n();
  const healthyMonitors = state.monitors.filter((monitor) => monitor.state === "healthy").length;
  const criticalEvents = state.evidence.filter((item) => item.risk === "critical").length;
  return (
    <div className="page-stack">
      {!state.serviceConnected && (
        <div className="notice warning"><WifiOff size={19} /><div><strong>{t("overview.serviceDisconnected")}</strong><span>{t("overview.serviceDisconnectedDescription")}</span></div></div>
      )}
      <div className="metric-grid">
        <Card className="metric-card primary"><div className="metric-icon"><KarmaShieldIcon /></div><div><span>{t("overview.protectionStatus")}</span><strong>{t(state.protectionEnabled ? "overview.enabled" : "overview.paused")}</strong><small>{t(state.protectionEnabled ? "overview.policyOnConnect" : "overview.noProtectionActions")}</small></div></Card>
        <Card className="metric-card"><div className="metric-icon green"><Monitor /></div><div><span>{t("overview.activeMonitors")}</span><strong>{state.monitors.length}</strong><small>{t(healthyMonitors === 1 ? "overview.healthyMonitor" : "overview.healthyMonitors", { count: healthyMonitors })}</small></div></Card>
        <Card className="metric-card"><div className="metric-icon amber"><Activity /></div><div><span>{t("overview.riskEventsToday")}</span><strong>{state.evidence.length}</strong><small>{t(criticalEvents === 1 ? "overview.criticalEvent" : "overview.criticalEvents", { count: criticalEvents })}</small></div></Card>
        <Card className="metric-card"><div className="metric-icon slate"><Archive /></div><div><span>{t("overview.evidenceRetention")}</span><strong>{state.recognition.evidenceEnabled ? t(state.recognition.evidenceRetentionDays === 1 ? "overview.day" : "overview.days", { count: state.recognition.evidenceRetentionDays }) : t("overview.notEnabled")}</strong><small>{t("overview.encryptedEvidence")}</small></div></Card>
      </div>
      <div className="two-column">
        <Card>
          <div className="card-heading"><div><span className="eyebrow">SYSTEM</span><h2>{t("overview.componentStatus")}</h2></div><StatusPill tone={state.serviceConnected ? "good" : "warn"}>{t(state.serviceConnected ? "overview.healthy" : "overview.waiting")}</StatusPill></div>
          <div className="health-list">
            <div><span className={`health-dot ${state.serviceConnected ? "online" : "offline"}`} /><div><strong>{t("overview.protectionService")}</strong><small>{t("overview.protectionServiceDescription")}</small></div><b>{t(state.serviceConnected ? "overview.online" : "overview.notImplemented")}</b></div>
            <div><span className={`health-dot ${state.agentConnected ? "online" : "offline"}`} /><div><strong>{t("overview.monitorAgent")}</strong><small>{t("overview.monitorAgentDescription")}</small></div><b>{t(state.agentConnected ? "overview.online" : "overview.notConnected")}</b></div>
            <div><span className="health-dot online" /><div><strong>{t("overview.console")}</strong><small>{t("overview.consoleDescription")}</small></div><b>{t("overview.online")}</b></div>
          </div>
        </Card>
        <Card>
          <div className="card-heading"><div><span className="eyebrow">RECENT</span><h2>{t("overview.recentActivity")}</h2></div></div>
          {state.audit.length ? <div className="compact-list">{state.audit.slice(0, 5).map((item) => <div key={item.id}><span className={`event-mark ${item.outcome}`} /><div><strong>{item.detail}</strong><small>{item.occurredAt}</small></div></div>)}</div> : <EmptyState icon={<FileClock />} title={t("overview.noAuditEvents")} text={t("overview.noAuditEventsDescription")} />}
        </Card>
      </div>
    </div>
  );
}

function EmptyState({ icon, title, text, action }: { icon: ReactNode; title: string; text: string; action?: ReactNode }) {
  return <div className="empty-state"><div>{icon}</div><strong>{title}</strong><p>{text}</p>{action}</div>;
}

function Monitors({ state }: { state: ConsoleState }) {
  const { t } = useI18n();
  return <div className="monitor-grid">{state.monitors.length ? state.monitors.map((monitor) => <Card key={monitor.id} className="monitor-card"><div className="monitor-preview"><Monitor size={48} /><span>{monitor.resolution}</span></div><div className="monitor-info"><div><strong>{monitor.name}</strong><StatusPill tone={monitor.state === "healthy" ? "good" : monitor.state === "degraded" ? "warn" : "muted"}>{t(`monitors.${monitor.state}`)}</StatusPill></div><dl><div><dt>{t("monitors.fps")}</dt><dd>{monitor.fps.toFixed(1)} FPS</dd></div><div><dt>{t("monitors.latency")}</dt><dd>{monitor.latencyMs} ms</dd></div></dl></div></Card>) : <Card className="full-span"><EmptyState icon={<Monitor />} title={t("monitors.emptyTitle")} text={t("monitors.emptyDescription")} /></Card>}</div>;
}

function Recognition({ state, update }: { state: ConsoleState; update: (state: ConsoleState) => void }) {
  const { t } = useI18n();
  const settings = state.recognition;
  const patch = (next: Partial<ConsoleState["recognition"]>) => update({ ...state, recognition: { ...settings, ...next } });
  return <div className="settings-grid">
    <Card><div className="setting-row"><div className="setting-icon violet"><Image /></div><div><strong>{t("recognition.imageTitle")}</strong><p>{t("recognition.imageDescription")}</p></div><Toggle checked={settings.imageEnabled} onChange={(value) => patch({ imageEnabled: value })} label={t("recognition.imageToggle")} /></div></Card>
    <Card><div className="setting-row"><div className="setting-icon blue"><Search /></div><div><strong>{t("recognition.ocrTitle")}</strong><p>{t("recognition.ocrDescription")}</p></div><Toggle checked={settings.ocrEnabled} onChange={(value) => patch({ ocrEnabled: value })} label={t("recognition.ocrToggle")} /></div></Card>
    <Card className="full-span"><div className="setting-row"><div className="setting-icon green"><AppWindow /></div><div><strong>{t("recognition.titleTitle")}</strong><p>{t("recognition.titleDescription")}</p></div><Toggle checked={settings.titleMatchingEnabled} onChange={(value) => patch({ titleMatchingEnabled: value })} label={t("recognition.titleToggle")} /></div></Card>
    <Card className="full-span"><div className="card-heading"><div><h2>{t("recognition.sensitivity")}</h2><p>{t("recognition.sensitivityDescription")}</p></div><span className="value-badge">{settings.sensitivity}%</span></div><input className="range" type="range" min="60" max="95" value={settings.sensitivity} onChange={(event) => { const value = Number(event.target.value); patch({ sensitivity: value, immediateThreshold: value }); }} /><div className="range-labels"><span>{t("recognition.moreResponsive")}</span><span>{t("recognition.balanced")}</span><span>{t("recognition.fewerFalsePositives")}</span></div></Card>
    <Card className="full-span evidence-setting"><div className="setting-row"><div className="setting-icon amber"><Archive /></div><div><strong>{t("recognition.evidenceTitle")}</strong><p>{t("recognition.evidenceDescription")}</p></div><Toggle checked={settings.evidenceEnabled} onChange={(value) => patch({ evidenceEnabled: value })} label={t("recognition.evidenceToggle")} /></div>{settings.evidenceEnabled && <div className="inline-setting"><label>{t("recognition.retention")}</label><select value={settings.evidenceRetentionDays} onChange={(event) => patch({ evidenceRetentionDays: Number(event.target.value) })}>{[1, 3, 7, 14, 30].map((days) => <option value={days} key={days}>{t(days === 1 ? "overview.day" : "overview.days", { count: days })}</option>)}</select><span>{t("recognition.retentionDescription")}</span></div>}</Card>
  </div>;
}

function Keywords({ state, update }: { state: ConsoleState; update: (state: ConsoleState) => void }) {
  const { t } = useI18n();
  const [phrase, setPhrase] = useState("");
  const [category, setCategory] = useState<KeywordRule["category"]>("sensitive");
  function add() { if (!phrase.trim()) return; update({ ...state, keywords: [...state.keywords, { id: crypto.randomUUID(), phrase: phrase.trim(), category, enabled: true }] }); setPhrase(""); }
  const categoryLabel = (value: KeywordRule["category"]) => t(value === "high_risk" ? "keywords.highRisk" : value === "sensitive" ? "keywords.sensitive" : "keywords.exemptionShort");
  return <Card><div className="toolbar"><div className="input-with-icon"><Search size={17} /><input placeholder={t("keywords.placeholder")} value={phrase} onChange={(event) => setPhrase(event.target.value)} onKeyDown={(event) => event.key === "Enter" && add()} /></div><select value={category} onChange={(event) => setCategory(event.target.value as KeywordRule["category"])}><option value="high_risk">{t("keywords.highRisk")}</option><option value="sensitive">{t("keywords.sensitive")}</option><option value="exemption">{t("keywords.exemption")}</option></select><button className="primary-button" onClick={add}><Plus size={17} />{t("keywords.add")}</button></div>{state.keywords.length ? <div className="table"><div className="table-head"><span>{t("keywords.keyword")}</span><span>{t("keywords.category")}</span><span>{t("keywords.status")}</span><span /></div>{state.keywords.map((rule) => <div className="table-row" key={rule.id}><strong>{rule.phrase}</strong><span><span className={`tag ${rule.category}`}>{categoryLabel(rule.category)}</span></span><span><Toggle checked={rule.enabled} onChange={(enabled) => update({ ...state, keywords: state.keywords.map((item) => item.id === rule.id ? { ...item, enabled } : item) })} label={t("keywords.enable", { name: rule.phrase })} /></span><button className="icon-button danger" aria-label={t("keywords.delete", { name: rule.phrase })} onClick={() => update({ ...state, keywords: state.keywords.filter((item) => item.id !== rule.id) })}><Trash2 size={17} /></button></div>)}</div> : <EmptyState icon={<Search />} title={t("keywords.emptyTitle")} text={t("keywords.emptyDescription")} />}</Card>;
}

function normalizeWebsitePattern(value: string): string | null {
  try {
    const url = new URL(/^https?:\/\//i.test(value) ? value : `https://${value}`);
    if (!/^https?:$/.test(url.protocol) || url.username || url.password || url.port || url.search || url.hash || url.hostname.endsWith(".") || (url.pathname !== "/" && url.pathname !== "")) return null;
    return url.hostname;
  } catch {
    return null;
  }
}

function Websites({ state, update }: { state: ConsoleState; update: (state: ConsoleState) => void }) {
  const { t } = useI18n();
  const [pattern, setPattern] = useState("");
  const [action, setAction] = useState<WebsiteRule["action"]>("allow");
  const [error, setError] = useState("");
  function add() {
    const normalized = normalizeWebsitePattern(pattern.trim());
    if (!normalized) return setError(t("websites.invalid"));
    if (state.websites.some((rule) => rule.pattern === normalized && rule.action === action)) return setError(t("websites.duplicate"));
    update({ ...state, websites: [...state.websites, { id: crypto.randomUUID(), pattern: normalized, action, enabled: true }] });
    setPattern("");
    setError("");
  }
  return <div className="page-stack"><div className="privacy-banner"><Globe2 size={18} /><span><strong>{t("websites.priorityTitle")}</strong> {t("websites.priorityDescription")}</span></div><Card><div className="toolbar"><div className="input-with-icon"><Globe2 size={17} /><input placeholder={t("websites.placeholder")} value={pattern} onChange={(event) => { setPattern(event.target.value); setError(""); }} onKeyDown={(event) => event.key === "Enter" && add()} /></div><select value={action} onChange={(event) => { setAction(event.target.value as WebsiteRule["action"]); setError(""); }}><option value="allow">{t("websites.allow")}</option><option value="block">{t("websites.block")}</option></select><button className="primary-button" onClick={add}><Plus size={17} />{t("websites.add")}</button></div>{error && <div className="form-error website-error"><CircleAlert size={16} />{error}</div>}{state.websites.length ? <div className="table"><div className="table-head"><span>{t("websites.domain")}</span><span>{t("websites.action")}</span><span>{t("websites.status")}</span><span /></div>{state.websites.map((rule) => <div className="table-row" key={rule.id}><strong>{rule.pattern}</strong><span><span className={`tag ${rule.action}`}>{t(rule.action === "allow" ? "websites.allow" : "websites.block")}</span></span><span><Toggle checked={rule.enabled} onChange={(enabled) => update({ ...state, websites: state.websites.map((item) => item.id === rule.id ? { ...item, enabled } : item) })} label={t("websites.enable", { name: rule.pattern })} /></span><button className="icon-button danger" aria-label={t("websites.delete", { name: rule.pattern })} onClick={() => update({ ...state, websites: state.websites.filter((item) => item.id !== rule.id) })}><Trash2 size={17} /></button></div>)}</div> : <EmptyState icon={<Globe2 />} title={t("websites.emptyTitle")} text={t("websites.emptyDescription")} />}</Card></div>;
}

function localizedApplicationName(rule: ApplicationRule, t: Translate): string {
  if (rule.id === "browser" && rule.name === "浏览器") return t("applications.browser");
  if (rule.id === "player" && rule.name === "播放器") return t("applications.player");
  return rule.name;
}

function localizedExecutable(rule: ApplicationRule, t: Translate): string {
  if (rule.id === "browser" && rule.executable === "受支持浏览器") return t("applications.supportedBrowser");
  if (rule.id === "player" && rule.executable === "受支持播放器") return t("applications.supportedPlayer");
  return rule.executable;
}

function Applications({ state, update }: { state: ConsoleState; update: (state: ConsoleState) => void }) {
  const { t } = useI18n();
  const [showAdd, setShowAdd] = useState(false);
  const [draft, setDraft] = useState({ name: "", executable: "", category: "custom" as ApplicationRule["category"] });
  function add() { if (!draft.name.trim() || !draft.executable.trim()) return; update({ ...state, applications: [...state.applications, { id: crypto.randomUUID(), ...draft, action: "content_only", enabled: true }] }); setShowAdd(false); setDraft({ name: "", executable: "", category: "custom" }); }
  return <div className="page-stack"><div className="page-actions"><button className="primary-button" onClick={() => setShowAdd(true)}><Plus size={17} />{t("applications.add")}</button></div><Card>{state.applications.map((rule) => { const name = localizedApplicationName(rule, t); return <div className="app-rule" key={rule.id}><div className="app-avatar"><AppWindow /></div><div className="app-main"><strong>{name}</strong><small>{localizedExecutable(rule, t)}</small></div><select value={rule.action} onChange={(event) => update({ ...state, applications: state.applications.map((item) => item.id === rule.id ? { ...item, action: event.target.value as ApplicationRule["action"] } : item) })}><option value="content_only">{t("applications.contentOnly")}</option><option value="block">{t("applications.block")}</option><option value="allow">{t("applications.allow")}</option></select><Toggle checked={rule.enabled} onChange={(enabled) => update({ ...state, applications: state.applications.map((item) => item.id === rule.id ? { ...item, enabled } : item) })} label={t("applications.enable", { name })} /></div>; })}</Card>{showAdd && <Modal title={t("applications.modalTitle")} onClose={() => setShowAdd(false)}><label className="field"><span>{t("applications.displayName")}</span><input value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} placeholder={t("applications.namePlaceholder")} /></label><label className="field"><span>{t("applications.executable")}</span><input value={draft.executable} onChange={(event) => setDraft({ ...draft, executable: event.target.value })} placeholder={t("applications.executablePlaceholder")} /></label><label className="field"><span>{t("applications.category")}</span><select value={draft.category} onChange={(event) => setDraft({ ...draft, category: event.target.value as ApplicationRule["category"] })}><option value="browser">{t("applications.browser")}</option><option value="player">{t("applications.player")}</option><option value="game">{t("applications.game")}</option><option value="custom">{t("applications.custom")}</option></select></label><button className="primary-button modal-submit" onClick={add}>{t("applications.save")}</button></Modal>}</div>;
}

const dayKeys: MessageKey[] = ["schedule.dayMon", "schedule.dayTue", "schedule.dayWed", "schedule.dayThu", "schedule.dayFri", "schedule.daySat", "schedule.daySun"];

function Schedule({ state, update }: { state: ConsoleState; update: (state: ConsoleState) => void }) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<Omit<ScheduleRule, "id">>({ name: t("schedule.defaultName"), days: [0, 1, 2, 3, 4], start: "21:00", end: "07:00", target: "browser_player", enabled: true });
  function add() { update({ ...state, schedules: [...state.schedules, { ...draft, id: crypto.randomUUID() }] }); }
  return <div className="two-column schedule-layout"><Card><div className="card-heading"><div><h2>{t("schedule.newTitle")}</h2><p>{t("schedule.description")}</p></div></div><label className="field"><span>{t("schedule.name")}</span><input value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} /></label><div className="field"><span>{t("schedule.weekdays")}</span><div className="day-picker">{dayKeys.map((key, index) => <button type="button" className={draft.days.includes(index) ? "active" : ""} key={key} onClick={() => setDraft({ ...draft, days: draft.days.includes(index) ? draft.days.filter((day) => day !== index) : [...draft.days, index].sort() })}>{t(key)}</button>)}</div></div><div className="time-grid"><label className="field"><span>{t("schedule.start")}</span><input type="time" step="900" value={draft.start} onChange={(event) => setDraft({ ...draft, start: event.target.value })} /></label><label className="field"><span>{t("schedule.end")}</span><input type="time" step="900" value={draft.end} onChange={(event) => setDraft({ ...draft, end: event.target.value })} /></label></div><label className="field"><span>{t("schedule.target")}</span><select value={draft.target} onChange={(event) => setDraft({ ...draft, target: event.target.value as ScheduleRule["target"] })}><option value="browser_player">{t("schedule.browserPlayer")}</option><option value="internet">{t("schedule.internet")}</option><option value="all_restricted">{t("schedule.allRestricted")}</option></select></label><button className="primary-button modal-submit" onClick={add}><Plus size={17} />{t("schedule.add")}</button></Card><Card><div className="card-heading"><div><h2>{t("schedule.configured")}</h2><p>{t(state.schedules.length === 1 ? "schedule.policyCountOne" : "schedule.policyCount", { count: state.schedules.length })}</p></div></div>{state.schedules.length ? <div className="schedule-list">{state.schedules.map((rule) => <div key={rule.id}><div className="schedule-time"><strong>{rule.start}</strong><span>{t("schedule.to")}</span><strong>{rule.end}</strong></div><div className="schedule-info"><strong>{rule.name}</strong><small>{rule.days.map((day) => t("schedule.weekdayPrefix", { day: t(dayKeys[day]) })).join(" · ")}</small></div><Toggle checked={rule.enabled} onChange={(enabled) => update({ ...state, schedules: state.schedules.map((item) => item.id === rule.id ? { ...item, enabled } : item) })} label={t("schedule.enable", { name: rule.name })} /><button className="icon-button danger" aria-label={t("schedule.delete", { name: rule.name })} onClick={() => update({ ...state, schedules: state.schedules.filter((item) => item.id !== rule.id) })}><Trash2 size={16} /></button></div>)}</div> : <EmptyState icon={<CalendarClock />} title={t("schedule.emptyTitle")} text={t("schedule.emptyDescription")} />}</Card></div>;
}

function Evidence({ state, sessionToken }: { state: ConsoleState; sessionToken: string }) {
  const { t } = useI18n();
  const [selected, setSelected] = useState<EvidenceItem | null>(null);
  const [password, setPassword] = useState("");
  const [imageUrl, setImageUrl] = useState<string>();
  const [error, setError] = useState("");
  async function reveal() { if (!selected) return; setError(""); try { setImageUrl(await revealEvidence(sessionToken, selected.id, password)); } catch (reason) { setError(localizeError(reason, t)); } }
  return <div className="page-stack"><div className="privacy-banner"><Lock size={18} /><span><strong>{t("evidence.bannerTitle")}</strong> {t("evidence.bannerDescription")}</span></div>{state.evidence.length ? <div className="evidence-grid">{state.evidence.map((item) => <button className="evidence-card" key={item.id} onClick={() => { setSelected(item); setImageUrl(undefined); setPassword(""); }}><div className="evidence-image">{item.thumbnailUrl ? <img src={item.thumbnailUrl} alt={t("evidence.blurredAlt")} /> : <Image />}<span><Eye size={16} />{t("evidence.viewAfterVerification")}</span></div><div><StatusPill tone={item.risk === "critical" ? "danger" : "warn"}>{t(item.risk === "critical" ? "evidence.critical" : "evidence.highRisk")}</StatusPill><strong>{item.applicationName}</strong><small>{item.capturedAt} · {item.monitorName}</small></div></button>)}</div> : <Card><EmptyState icon={<Lock />} title={t("evidence.emptyTitle")} text={t(state.recognition.evidenceEnabled ? "evidence.emptyEnabled" : "evidence.emptyDisabled")} /></Card>}{selected && <Modal title={t("evidence.modalTitle")} onClose={() => setSelected(null)} wide>{imageUrl ? <img className="revealed-image" src={imageUrl} alt={t("evidence.originalAlt")} /> : <div className="reveal-panel"><div className="locked-preview"><LockKeyhole /></div><strong>{selected.reason}</strong><p>{selected.capturedAt} · {selected.applicationName} · {selected.monitorName}</p><label className="field"><span>{t("evidence.passwordAgain")}</span><input type="password" value={password} onChange={(event) => setPassword(event.target.value)} onKeyDown={(event) => event.key === "Enter" && reveal()} autoFocus /></label>{error && <div className="form-error"><CircleAlert size={16} />{error}</div>}<button className="primary-button modal-submit" onClick={reveal}><Eye size={17} />{t("evidence.decrypt")}</button></div>}</Modal>}</div>;
}

function Audit({ state }: { state: ConsoleState }) {
  const { t } = useI18n();
  return <Card>{state.audit.length ? <div className="audit-list">{state.audit.map((item) => <div key={item.id}><span className={`audit-icon ${item.outcome}`}>{item.outcome === "success" ? <Check /> : <CircleAlert />}</span><div><strong>{item.detail}</strong><small>{item.kind} · {item.occurredAt}</small></div><StatusPill tone={item.outcome === "success" ? "good" : item.outcome === "warning" ? "warn" : "danger"}>{t(item.outcome === "success" ? "audit.success" : item.outcome === "warning" ? "audit.warning" : "audit.denied")}</StatusPill></div>)}</div> : <EmptyState icon={<FileClock />} title={t("audit.emptyTitle")} text={t("audit.emptyDescription")} />}</Card>;
}

function PasswordChangeCard({ sessionToken }: { sessionToken: string }) {
  const { t } = useI18n();
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    setSaved(false);
    if (newPassword.length < 10) return setError(t("auth.passwordTooShort"));
    if (newPassword !== confirmPassword) return setError(t("auth.passwordMismatch"));
    setBusy(true);
    try {
      await changePassword(sessionToken, currentPassword, newPassword);
      setCurrentPassword("");
      setNewPassword("");
      setConfirmPassword("");
      setSaved(true);
    } catch (reason) {
      setError(localizeError(reason, t));
    } finally {
      setBusy(false);
    }
  }

  return <Card className="full-span">
    <div className="card-heading"><div><h2>{t("settings.passwordTitle")}</h2><p>{t("settings.passwordDescription")}</p></div></div>
    <form className="password-form" onSubmit={submit}>
      <label className="field"><span>{t("settings.currentPassword")}</span><input type="password" autoComplete="current-password" value={currentPassword} onChange={(event) => { setCurrentPassword(event.target.value); setSaved(false); }} /></label>
      <label className="field"><span>{t("settings.newPassword")}</span><input type="password" autoComplete="new-password" value={newPassword} onChange={(event) => { setNewPassword(event.target.value); setSaved(false); }} /></label>
      <label className="field"><span>{t("settings.confirmNewPassword")}</span><input type="password" autoComplete="new-password" value={confirmPassword} onChange={(event) => { setConfirmPassword(event.target.value); setSaved(false); }} /></label>
      <div className="password-form-footer">
        {error && <div className="form-error"><CircleAlert size={16} />{error}</div>}
        {saved && !error && <div className="form-success"><Check size={16} />{t("settings.passwordChanged")}</div>}
        <button className="primary-button" disabled={busy || !currentPassword || !newPassword || !confirmPassword} type="submit"><KeyRound size={17} />{t(busy ? "common.saving" : "settings.changePassword")}</button>
      </div>
    </form>
  </Card>;
}

function SettingsPage({ state, update, sessionToken }: { state: ConsoleState; update: (state: ConsoleState) => void; sessionToken: string }) {
  const { t } = useI18n();
  return <div className="settings-grid"><Card className="full-span"><div className="setting-row"><div className="setting-icon green"><KarmaShieldIcon /></div><div><strong>{t("settings.protectionTitle")}</strong><p>{t("settings.protectionDescription")}</p></div><Toggle checked={state.protectionEnabled} onChange={(value) => update({ ...state, protectionEnabled: value })} label={t("settings.protectionToggle")} /></div></Card><Card className="full-span"><div className="setting-row"><div className="setting-icon violet"><Power /></div><div><strong>{t("settings.autostartTitle")}</strong><p>{t("settings.autostartDescription")}</p></div><Toggle checked={state.launchAtStartup} onChange={(value) => update({ ...state, launchAtStartup: value })} label={t("settings.autostartToggle")} /></div></Card><Card className="full-span language-card"><div className="setting-row"><div className="setting-icon blue"><Globe2 /></div><div><strong>{t("settings.languageTitle")}</strong><p>{t("language.description")}</p></div><LanguageSelect /></div></Card><PasswordChangeCard sessionToken={sessionToken} /><Card><div className="card-heading"><div><h2>{t("settings.capabilities")}</h2><p>{t("settings.progress")}</p></div></div><div className="capability-list"><div><Check />{t("settings.capability1")}</div><div><Check />{t("settings.capability2")}</div><div><Check />{t("settings.capability3")}</div><div><Check />{t("settings.capability4")}</div><div><Check />{t("settings.capability5")}</div><div className="pending"><CircleAlert />{t("settings.capabilityPending")}</div></div></Card><Card><div className="card-heading"><div><h2>{t("settings.security")}</h2><p>{t("settings.securitySubtitle")}</p></div></div><div className="security-note"><Lock /><p>{t("settings.securityDescription")}</p></div></Card></div>;
}

function Modal({ title, children, onClose, wide = false }: { title: string; children: ReactNode; onClose: () => void; wide?: boolean }) {
  const { t } = useI18n();
  return <div className="modal-backdrop" onMouseDown={onClose}><section className={`modal ${wide ? "wide" : ""}`} onMouseDown={(event) => event.stopPropagation()}><div className="modal-head"><h2>{title}</h2><button className="icon-button" aria-label={t("common.close")} onClick={onClose}><X /></button></div>{children}</section></div>;
}

function AppContent() {
  const { locale, t } = useI18n();
  const [authMode, setAuthMode] = useState<AuthMode>("locked");
  const [sessionToken, setSessionToken] = useState("");
  const [page, setPage] = useState<PageKey>("overview");
  const [state, setState] = useState<ConsoleState>(defaultConsoleState);
  const [loading, setLoading] = useState(true);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState("");
  const [saveError, setSaveError] = useState("");
  const [startupError, setStartupError] = useState("");

  async function refreshAuthStatus() { setLoading(true); setStartupError(""); try { setAuthMode(await authStatus()); } catch (reason) { setStartupError(reason instanceof Error ? reason.message : String(reason)); } finally { setLoading(false); } }
  useEffect(() => { document.documentElement.lang = locale; document.title = t("document.title"); }, [locale, t]);
  useEffect(() => { void refreshAuthStatus(); }, []);
  useEffect(() => {
    if (authMode !== "unlocked" || !sessionToken) return;
    let timeout = window.setTimeout(() => void signOut(), 15 * 60 * 1000);
    const reset = () => { window.clearTimeout(timeout); timeout = window.setTimeout(() => void signOut(), 15 * 60 * 1000); };
    const events: Array<keyof WindowEventMap> = ["pointerdown", "keydown", "wheel"];
    events.forEach((eventName) => window.addEventListener(eventName, reset, { passive: true }));
    return () => { window.clearTimeout(timeout); events.forEach((eventName) => window.removeEventListener(eventName, reset)); };
  }, [authMode, sessionToken]);
  useEffect(() => {
    if (authMode !== "unlocked" || !sessionToken || dirty) return;
    const refresh = () => {
      void loadConsole(sessionToken).then(setState).catch((reason) => {
        // A restarted service invalidates every in-memory session; return to the
        // unlock screen so a fresh token is issued instead of reporting offline.
        if (isSessionExpiredError(reason)) { void signOut(); return; }
        setState((current) => ({ ...current, serviceConnected: false, agentConnected: false, monitors: [] }));
      });
    };
    const interval = window.setInterval(refresh, 5000);
    return () => window.clearInterval(interval);
  }, [authMode, sessionToken, dirty]);
  async function authenticated(token: string) { setSessionToken(token); setLoading(true); try { const next = await loadConsole(token); setState(next); setAuthMode("unlocked"); void configureLaunchAtStartup(next.launchAtStartup).catch((reason) => console.error("Failed to synchronize autostart", reason)); } finally { setLoading(false); } }
  function update(next: ConsoleState) { setState(next); setDirty(true); setSaveMessage(""); setSaveError(""); }
  async function save() {
    setSaving(true);
    setSaveError("");
    try {
      const next = { ...state, recognition: { ...state.recognition, immediateThreshold: state.recognition.sensitivity } };
      await configureLaunchAtStartup(next.launchAtStartup);
      await saveConsole(sessionToken, next);
      setState(next);
      setDirty(false);
      setSaveMessage(t("common.settingsSaved"));
      window.setTimeout(() => setSaveMessage(""), 2200);
    } catch (reason) {
      if (isSessionExpiredError(reason)) { await signOut(); return; }
      setSaveError(localizeError(reason, t));
    } finally {
      setSaving(false);
    }
  }
  async function signOut() {
    // The service may be down or the session already invalid; always reset locally.
    await lock(sessionToken).catch(() => undefined);
    setSessionToken("");
    setState(defaultConsoleState);
    setAuthMode("locked");
    setPage("overview");
    setSaveMessage("");
    setSaveError("");
  }

  const content = useMemo(() => {
    if (page === "overview") return <Overview state={state} />;
    if (page === "monitors") return <Monitors state={state} />;
    if (page === "recognition") return <Recognition state={state} update={update} />;
    if (page === "keywords") return <Keywords state={state} update={update} />;
    if (page === "websites") return <Websites state={state} update={update} />;
    if (page === "applications") return <Applications state={state} update={update} />;
    if (page === "schedule") return <Schedule state={state} update={update} />;
    if (page === "evidence") return <Evidence state={state} sessionToken={sessionToken} />;
    if (page === "audit") return <Audit state={state} />;
    return <SettingsPage state={state} update={update} sessionToken={sessionToken} />;
  }, [page, state, sessionToken, locale]);

  if (loading) return <div className="loading-screen"><div className="brand-mark"><KarmaShieldIcon /></div><span>{t("common.loading")}</span></div>;
  if (startupError) return <ServiceConnectionError detail={localizeError(startupError, t)} onRetry={() => void refreshAuthStatus()} />;
  if (authMode !== "unlocked") return <PasswordGate mode={authMode} onAuthenticated={authenticated} />;
  const meta = pageMeta[page];
  return <div className="app-shell"><aside className="sidebar"><div className="brand-lockup sidebar-brand"><div className="brand-mark"><KarmaShieldIcon /></div><div><strong>KARMA</strong><span>{t("brand.short")}</span></div></div><nav>{navItems.map((item) => { const Icon = item.icon; return <button className={page === item.key ? "active" : ""} key={item.key} onClick={() => setPage(item.key)}><Icon size={19} /><span>{t(item.label)}</span>{item.key === "evidence" && state.evidence.length > 0 && <b>{state.evidence.length}</b>}</button>; })}</nav><div className="sidebar-foot"><div className="mini-health"><span className={state.serviceConnected ? "online" : "offline"} /><div><strong>{t(state.serviceConnected ? "sidebar.serviceOnline" : "sidebar.serviceOffline")}</strong><small>{t(state.agentConnected ? "sidebar.agentOnline" : "sidebar.consoleOnly")}</small></div></div><button onClick={signOut}><LogOut size={18} /><span>{t("common.lockConsole")}</span></button></div></aside><main className="main"><header><div><span className="eyebrow">KARMA CONTROL</span><h1>{t(meta.title)}</h1><p>{t(meta.subtitle)}</p></div><div className="header-actions">{saveMessage && <span className="saved-message"><Check size={15} />{saveMessage}</span>}{saveError && <span className="save-error-message"><CircleAlert size={15} />{saveError}</span>}<button className="secondary-button" onClick={signOut}><Lock size={16} />{t("common.lock")}</button><button className="primary-button" disabled={!dirty || saving} onClick={save}><Save size={17} />{t(saving ? "common.saving" : "common.saveSettings")}</button></div></header><div className="page-content">{content}</div></main></div>;
}

export function App() {
  const [locale, setLocaleState] = useState<Locale>(getInitialLocale);
  const t = useMemo(() => createTranslator(locale), [locale]);
  const setLocale = (nextLocale: Locale) => { persistLocale(nextLocale); setLocaleState(nextLocale); };
  const value = useMemo(() => ({ locale, setLocale, t }), [locale, t]);
  return <I18nContext.Provider value={value}><AppContent /></I18nContext.Provider>;
}
