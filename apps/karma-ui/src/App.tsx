import {
  ChangeEvent,
  createContext,
  FormEvent,
  ReactNode,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  Activity,
  AppWindow,
  Archive,
  Ban,
  CalendarClock,
  Check,
  Copy,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  Download,
  Eye,
  EyeOff,
  FileClock,
  Fingerprint,
  Gauge,
  Globe2,
  History,
  Image,
  KeyRound,
  LayoutDashboard,
  ListChecks,
  Lock,
  LockKeyhole,
  Monitor,
  Pencil,
  Power,
  Plus,
  Save,
  Search,
  Tags,
  Settings,
  ShieldCheck,
  RotateCcw,
  Trash2,
  Upload,
  Wifi,
  WifiOff,
  X,
} from "lucide-react";
import {
  authStatus,
  changePassword,
  changeSuperPassword,
  checkForUpdates,
  cancelUninstall,
  chooseExportPath,
  chooseImportPath,
  configureLaunchAtStartup,
  defaultConsoleState,
  enroll,
  exportSettings,
  forgetRememberedTitle,
  markTitleMisjudged,
  restoreMisjudgedTitle,
  allowBannedApplication,
  allowEvidenceApplication,
  isBrowserExecutable,
  allowApplications,
  applicationEffect,
  assignApplicationsToCategory,
  applicationCategoryOf,
  knownBrowserExecutables,
  isPlaceholderApplication,
  removeFromCategory,
  listInstalledApplications,
  getAppVersion,
  importSettings,
  isTauriConsole,
  isSuperPasswordRequired,
  isAuthorizationCancelled,
  loadAuthorizationStatus,
  listRememberedTitles,
  loadConsole,
  lock,
  maxApplicationRules,
  mergeImportedPolicy,
  parseImportedBackupFile,
  rememberedTitlePageSize,
  repeatOffenderLimits,
  restoreNetwork,
  revealEvidence,
  deleteEvidence,
  saveConsole,
  setSuperPassword,
  requestUninstall,
  unlock,
} from "./bridge";
import {
  bundledAmbiguousCount,
  bundledKeywordCount,
  bundledKeywordGroups,
  bundledLanguages,
  bundledStrictCount,
  coveredByStrictKeyword,
  defaultMatchFor,
  isCodePrefix,
  maxTitleKeywordAdditions,
  maxTitleKeywordTermChars,
  normalizeKeyword,
} from "./bundledKeywords";
import type { BundledKeyword } from "./bundledKeywords";
import {
  createTranslator,
  getInitialLocale,
  isMessageKey,
  isSessionExpiredError,
  localizeError,
  persistLocale,
} from "./i18n";
import type { Locale, MessageKey, Translate } from "./i18n";
import type {
  ActiveBan,
  ApplicationRule,
  AuditDetail,
  BuiltInApplicationCategory,
  AuditItem,
  AuthMode,
  ConsoleState,
  EvidenceItem,
  InstalledApplication,
  KeywordRule,
  TitleKeywordAddition,
  TitleKeywordCategory,
  TitleKeywordMatch,
  NetworkStatus,
  NetworkTarget,
  PageKey,
  RememberedTitlePage,
  TitleReason,
  ScheduleAction,
  ScheduleBlock,
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

function Overview({ state, update, sessionToken, onNetworkChange }: { state: ConsoleState; update: (next: ConsoleState) => void; sessionToken: string; onNetworkChange: (network: NetworkStatus) => void }) {
  const { t } = useI18n();
  const auditDateTime = useAuditDateTime();
  const healthyMonitors = state.monitors.filter((monitor) => monitor.state === "healthy").length;
  const criticalEvents = state.evidence.filter((item) => item.risk === "critical").length;
  return (
    <div className="page-stack">
      {!state.serviceConnected && (
        <div className="notice warning"><WifiOff size={19} /><div><strong>{t("overview.serviceDisconnected")}</strong><span>{t("overview.serviceDisconnectedDescription")}</span></div></div>
      )}
      <Card>
        <div className="setting-row">
          <div className="setting-icon green"><Power /></div>
          <div><strong>{t("settings.protectionTitle")}</strong><p>{t("overview.protectionSwitchDescription")}</p></div>
          <Toggle checked={state.protectionEnabled} onChange={(value) => update({ ...state, protectionEnabled: value })} label={t("settings.protectionToggle")} />
        </div>
      </Card>
      <NetworkStatusCard state={state} sessionToken={sessionToken} onNetworkChange={onNetworkChange} />
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
          {state.audit.length ? <div className="compact-list">{state.audit.slice(0, 5).map((item) => <div key={item.id}><span className={`event-mark ${auditLevel(item)}`} /><div><strong>{auditText(item, t).title}</strong><small>{auditDateTime.format(item.occurredAtMs)}</small></div></div>)}</div> : <EmptyState icon={<FileClock />} title={t("overview.noAuditEvents")} text={t("overview.noAuditEventsDescription")} />}
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

function executableFileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/** The Service's comparison form of an executable path. */
function normalizeExecutable(path: string): string {
  return path.trim().replace(/\//g, "\\").toLowerCase();
}

/** Whether evidence names an absolute `.exe` path, as the Service requires to allowlist it. */
function isAttributedExecutable(path: string): boolean {
  const normalized = normalizeExecutable(path);
  return (/^[a-z]:\\./.test(normalized) || normalized.startsWith("\\\\")) && normalized.endsWith(".exe");
}

function useShortDateTime(): Intl.DateTimeFormat {
  const { locale } = useI18n();
  return useMemo(() => new Intl.DateTimeFormat(locale, { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false }), [locale]);
}

/** Risk scores within this many points of the threshold are flagged as possible misjudgments. */
const nearThresholdMillis = 50;

function titleReason(reason: TitleReason | null | undefined, t: Translate): string {
  if (!reason) return t("recognition.reasonUnknown");
  const values = { risk: Math.round(reason.riskMillis / 10), threshold: Math.round(reason.thresholdMillis / 10) };
  const text = reason.reasonCode.startsWith("image") ? t("recognition.reasonImage", values) : t("recognition.reasonOther", { ...values, code: reason.reasonCode });
  return reason.riskMillis - reason.thresholdMillis < nearThresholdMillis ? `${text} · ${t("recognition.reasonNearThreshold")}` : text;
}

function RememberedTitlesCard({ state, update, sessionToken }: { state: ConsoleState; update: (state: ConsoleState) => void; sessionToken: string }) {
  const { t } = useI18n();
  const dateTime = useShortDateTime();
  const [query, setQuery] = useState("");
  const [appliedQuery, setAppliedQuery] = useState("");
  const [pageIndex, setPageIndex] = useState(0);
  const [showMisjudged, setShowMisjudged] = useState(false);
  const [titles, setTitles] = useState<RememberedTitlePage>({ items: [], total: 0 });
  const [reload, setReload] = useState(0);
  const [busyId, setBusyId] = useState<string>();
  const authorize = useSuperAuthorization();
  const [error, setError] = useState("");
  const settings = state.recognition;
  const pageCount = Math.max(1, Math.ceil(titles.total / rememberedTitlePageSize));
  useEffect(() => {
    const timeout = window.setTimeout(() => { setAppliedQuery(query); setPageIndex(0); }, 250);
    return () => window.clearTimeout(timeout);
  }, [query]);
  // The Service owns the titles: refetch when the page, query, or the total
  // reported by the periodic console refresh changes.
  useEffect(() => {
    let current = true;
    listRememberedTitles(sessionToken, pageIndex * rememberedTitlePageSize, appliedQuery, showMisjudged)
      .then((page) => {
        if (!current) return;
        if (!page.items.length && pageIndex > 0) { setPageIndex(Math.max(0, Math.ceil(page.total / rememberedTitlePageSize) - 1)); return; }
        setTitles(page);
        setError("");
      })
      .catch((reason) => { if (current) setError(localizeError(reason, t)); });
    return () => { current = false; };
  }, [sessionToken, pageIndex, appliedQuery, showMisjudged, state.rememberedTitleTotal, state.misjudgedTitleTotal, reload, t]);
  function showList(misjudged: boolean) { setShowMisjudged(misjudged); setPageIndex(0); setTitles({ items: [], total: 0 }); setError(""); }
  async function run(id: string, action: () => Promise<void>) {
    setBusyId(id);
    setError("");
    try {
      await action();
      setReload((value) => value + 1);
    } catch (reason) {
      if (!isAuthorizationCancelled(reason)) setError(localizeError(reason, t));
    } finally {
      setBusyId(undefined);
    }
  }
  return <Card className="full-span">
    <div className="setting-row"><div className="setting-icon violet"><History /></div><div><strong>{t("recognition.rememberTitle")}</strong><p>{t("recognition.rememberDescription")}</p></div><Toggle checked={settings.rememberTitlesEnabled} onChange={(value) => update({ ...state, recognition: { ...settings, rememberTitlesEnabled: value } })} label={t("recognition.rememberToggle")} /></div>
    <div className="learned-list">
      <div className="learned-toolbar"><div className="learned-tabs" role="tablist"><button role="tab" aria-selected={!showMisjudged} className={showMisjudged ? "" : "active"} onClick={() => showList(false)}>{t("recognition.rememberedCount", { count: state.rememberedTitleTotal })}</button><button role="tab" aria-selected={showMisjudged} className={showMisjudged ? "active" : ""} onClick={() => showList(true)}>{t("recognition.misjudgedCount", { count: state.misjudgedTitleTotal })}</button></div><label className="learned-search"><Search size={15} /><input value={query} onChange={(event) => setQuery(event.target.value)} maxLength={128} placeholder={t("recognition.searchTitles")} aria-label={t("recognition.searchTitles")} /></label></div>
      {appliedQuery.trim() && <span className="learned-heading">{t("recognition.searchCount", { count: titles.total })}</span>}
      {showMisjudged && <p className="learned-empty">{t("recognition.misjudgedDescription")}</p>}
      {titles.items.length ? titles.items.map((item) => <div className="app-rule" key={item.id}><div className="app-avatar">{item.browserHost ? <Globe2 /> : <AppWindow />}</div><div className="app-main"><strong>{item.title}</strong><small>{[executableFileName(item.executable), item.browserHost, t(showMisjudged ? "recognition.misjudgedAt" : "recognition.rememberedAt", { time: dateTime.format(item.rememberedAtMs) })].filter(Boolean).join(" · ")}</small>{!showMisjudged && <small className="title-reason">{t("recognition.reasonLabel")}{titleReason(item.reason, t)}</small>}</div>{showMisjudged
        ? <button className="secondary-button compact" disabled={busyId !== undefined} title={t("recognition.restoreMisjudgedHint")} onClick={() => void run(item.id, () => restoreMisjudgedTitle(sessionToken, item.id))}><RotateCcw size={14} />{t("recognition.restoreMisjudged")}</button>
        : <><button className="secondary-button compact" disabled={busyId !== undefined} title={t("recognition.markMisjudgedHint")} onClick={() => void run(item.id, () => authorize((superPassword) => markTitleMisjudged(sessionToken, item.id, superPassword)))}>{t("recognition.markMisjudged")}</button><button className="icon-button danger" disabled={busyId !== undefined} aria-label={t("recognition.forgetTitle")} title={t("recognition.forgetTitle")} onClick={() => void run(item.id, () => authorize((superPassword) => forgetRememberedTitle(sessionToken, item.id, superPassword)))}><Trash2 size={17} /></button></>}</div>) : <p className="learned-empty">{t(appliedQuery.trim() ? "recognition.searchEmpty" : showMisjudged ? "recognition.misjudgedEmpty" : "recognition.rememberedEmpty")}</p>}
      {pageCount > 1 && <div className="learned-pager"><button className="secondary-button" disabled={pageIndex === 0} onClick={() => setPageIndex(pageIndex - 1)}>{t("recognition.previousPage")}</button><span>{t("recognition.pageStatus", { page: pageIndex + 1, pages: pageCount })}</span><button className="secondary-button" disabled={pageIndex + 1 >= pageCount} onClick={() => setPageIndex(pageIndex + 1)}>{t("recognition.nextPage")}</button></div>}
      {error && <div className="form-error"><CircleAlert size={16} />{error}</div>}
    </div>
  </Card>;
}

function Recognition({ state, update, sessionToken }: { state: ConsoleState; update: (state: ConsoleState) => void; sessionToken: string }) {
  const { t } = useI18n();
  const settings = state.recognition;
  const patch = (next: Partial<ConsoleState["recognition"]>) => update({ ...state, recognition: { ...settings, ...next } });
  return <div className="settings-grid">
    <Card><div className="setting-row"><div className="setting-icon violet"><Image /></div><div><strong>{t("recognition.imageTitle")}</strong><p>{t("recognition.imageDescription")}</p></div><Toggle checked={settings.imageEnabled} onChange={(value) => patch({ imageEnabled: value })} label={t("recognition.imageToggle")} /></div></Card>
    <Card><div className="setting-row"><div className="setting-icon blue"><Search /></div><div><strong>{t("recognition.ocrTitle")}</strong><p>{t("recognition.ocrDescription")}</p></div><Toggle checked={settings.ocrEnabled} onChange={(value) => patch({ ocrEnabled: value })} label={t("recognition.ocrToggle")} /></div></Card>
    <Card className="full-span"><div className="setting-row"><div className="setting-icon green"><AppWindow /></div><div><strong>{t("recognition.titleTitle")}</strong><p>{t("recognition.titleDescription")}</p></div><Toggle checked={settings.titleMatchingEnabled} onChange={(value) => patch({ titleMatchingEnabled: value })} label={t("recognition.titleToggle")} /></div></Card>
    <RememberedTitlesCard state={state} update={update} sessionToken={sessionToken} />
    <Card className="full-span"><div className="card-heading"><div><h2>{t("recognition.sensitivity")}</h2><p>{t("recognition.sensitivityDescription")}</p></div><span className="value-badge">{settings.sensitivity}%</span></div><input className="range" type="range" min="60" max="95" value={settings.sensitivity} onChange={(event) => { const value = Number(event.target.value); patch({ sensitivity: value, immediateThreshold: value }); }} /><div className="range-labels"><span>{t("recognition.moreResponsive")}</span><span>{t("recognition.balanced")}</span><span>{t("recognition.fewerFalsePositives")}</span></div></Card>
    <Card className="full-span evidence-setting"><div className="setting-row"><div className="setting-icon amber"><Archive /></div><div><strong>{t("recognition.evidenceTitle")}</strong><p>{t("recognition.evidenceDescription")}</p></div><Toggle checked={settings.evidenceEnabled} onChange={(value) => patch({ evidenceEnabled: value })} label={t("recognition.evidenceToggle")} /></div>{settings.evidenceEnabled && <div className="inline-setting"><label>{t("recognition.retention")}</label><select value={settings.evidenceRetentionDays} onChange={(event) => patch({ evidenceRetentionDays: Number(event.target.value) })}>{[1, 3, 7, 14, 30].map((days) => <option value={days} key={days}>{t(days === 1 ? "overview.day" : "overview.days", { count: days })}</option>)}</select><span>{t("recognition.retentionDescription")}</span></div>}</Card>
  </div>;
}

function useLanguageLabel(): (language: string) => string {
  const { locale } = useI18n();
  return useMemo(() => {
    let names: Intl.DisplayNames | undefined;
    try { names = new Intl.DisplayNames([locale], { type: "language" }); } catch { names = undefined; }
    return (language: string) => names?.of(language) ?? language;
  }, [locale]);
}

/** Group key for video codes; it cannot collide with an ISO 639 language code. */
const codeGroupKey = "#code";

type BuiltInEntry = { term: string; category: TitleKeywordCategory; match: TitleKeywordMatch; confidence: BundledKeyword["confidence"] | "added"; addition?: TitleKeywordAddition };

function BuiltInKeywordsCard({ state, update }: { state: ConsoleState; update: (state: ConsoleState) => void }) {
  const { t } = useI18n();
  const languageLabel = useLanguageLabel();
  const [term, setTerm] = useState("");
  const [language, setLanguage] = useState("zh");
  const [category, setCategory] = useState<TitleKeywordCategory>("sexual");
  const [match, setMatch] = useState<TitleKeywordMatch>(defaultMatchFor("zh"));
  const [addError, setAddError] = useState("");
  const [search, setSearch] = useState("");
  const [languageFilter, setLanguageFilter] = useState("");
  const [categoryFilter, setCategoryFilter] = useState<"" | TitleKeywordCategory>("");
  const [confidenceFilter, setConfidenceFilter] = useState<"" | BuiltInEntry["confidence"]>("");
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const additions = state.titleKeywordAdditions;
  const ambiguousEnabled = state.recognition.ambiguousTitleKeywordsEnabled;
  const categoryLabel = (value: TitleKeywordCategory) => t(value === "organ" ? "keywords.categoryOrgan" : value === "code" ? "keywords.categoryCode" : "keywords.categorySexual");
  const matchLabel = (value: TitleKeywordMatch) => t(value === "word" ? "keywords.matchWord" : value === "substring" ? "keywords.matchSubstring" : value === "prefix" ? "keywords.matchPrefix" : "keywords.matchCode");
  const confidenceLabel = (value: BuiltInEntry["confidence"]) => t(value === "strict" ? "keywords.confidenceStrict" : value === "ambiguous" ? "keywords.confidenceAmbiguous" : "keywords.confidenceAdded");
  const languages = useMemo(() => [...bundledLanguages, ...additions.filter((item) => item.category !== "code").map((item) => item.language).filter((code) => !bundledLanguages.includes(code))], [additions]);
  const groups = useMemo(() => {
    const needle = normalizeKeyword(search);
    const keep = (entry: BuiltInEntry) => (!needle || normalizeKeyword(entry.term).includes(needle)) && (!categoryFilter || entry.category === categoryFilter) && (!confidenceFilter || entry.confidence === confidenceFilter);
    const addedEntry = (item: TitleKeywordAddition): BuiltInEntry => ({ term: item.term, category: item.category, match: item.match, confidence: "added", addition: item });
    // Video codes match in titles of any language, so they form one group of their own.
    const languageGroups = languages
      .filter((code) => !languageFilter || code === languageFilter)
      .map((code) => {
        const added = additions.filter((item) => item.language === code && item.category !== "code").map(addedEntry);
        const bundled: BuiltInEntry[] = (bundledKeywordGroups.find((group) => group.language === code)?.keywords ?? []).filter((entry) => entry.category !== "code");
        const all = [...added, ...bundled];
        return { key: code, total: all.length, entries: all.filter(keep) };
      });
    const codes: BuiltInEntry[] = [...additions.filter((item) => item.category === "code").map(addedEntry), ...bundledKeywordGroups.flatMap((group) => group.keywords.filter((entry) => entry.category === "code"))];
    const codeGroups = languageFilter ? [] : [{ key: codeGroupKey, total: codes.length, entries: codes.filter(keep) }];
    return [...languageGroups, ...codeGroups].filter((group) => group.entries.length > 0);
  }, [languages, additions, search, languageFilter, categoryFilter, confidenceFilter]);
  const filtering = Boolean(search.trim() || languageFilter || categoryFilter || confidenceFilter);
  function chooseLanguage(code: string) { setLanguage(code); if (category !== "code") setMatch(defaultMatchFor(code)); setAddError(""); }
  // Product codes, and only they, use code matching.
  function chooseCategory(value: TitleKeywordCategory) { setCategory(value); setMatch(value === "code" ? "code" : defaultMatchFor(language)); setAddError(""); }
  function add() {
    const trimmed = term.trim();
    if (!trimmed) return;
    if ([...trimmed].length > maxTitleKeywordTermChars) return setAddError(t("keywords.additionTooLong", { count: maxTitleKeywordTermChars }));
    if (category === "code" && !isCodePrefix(trimmed)) return setAddError(t("keywords.additionInvalidCode"));
    if (additions.length >= maxTitleKeywordAdditions) return setAddError(t("keywords.additionLimit", { count: maxTitleKeywordAdditions }));
    const normalized = normalizeKeyword(trimmed);
    if (coveredByStrictKeyword(trimmed, match) || additions.some((item) => normalizeKeyword(item.term) === normalized && item.match === match)) return setAddError(t("keywords.additionDuplicate"));
    update({ ...state, titleKeywordAdditions: [...additions, { id: crypto.randomUUID(), term: trimmed, language, category, match }] });
    setTerm("");
    setAddError("");
    setExpanded((current) => new Set(current).add(category === "code" ? codeGroupKey : language));
  }
  function remove(id: string) { update({ ...state, titleKeywordAdditions: additions.filter((item) => item.id !== id) }); }
  function toggleGroup(code: string) { setExpanded((current) => { const next = new Set(current); if (next.has(code)) next.delete(code); else next.add(code); return next; }); }
  return <Card>
    <div className="card-heading">
      <div><h2>{t("keywords.builtInTitle")}</h2><p>{t("keywords.builtInDescription", { languages: bundledLanguages.length, count: bundledKeywordCount })}</p></div>
      <div className="card-heading-side keyword-stats">
        <span className="value-badge">{t("keywords.strictCount", { count: bundledStrictCount })}</span>
        <span className="value-badge muted">{t("keywords.ambiguousCount", { count: bundledAmbiguousCount })}</span>
        {additions.length > 0 && <span className="value-badge">{t("keywords.addedCount", { count: additions.length })}</span>}
      </div>
    </div>
    <div className="setting-row keyword-ambiguous-row">
      <div><strong>{t("keywords.ambiguousTitle")}</strong><p>{t("keywords.ambiguousDescription", { count: bundledAmbiguousCount })}</p></div>
      <Toggle checked={ambiguousEnabled} onChange={(value) => update({ ...state, recognition: { ...state.recognition, ambiguousTitleKeywordsEnabled: value } })} label={t("keywords.ambiguousTitle")} />
    </div>
    <div className="keyword-add">
      <strong>{t("keywords.additionTitle")}</strong>
      <div className="toolbar">
        <div className="input-with-icon"><Plus size={17} /><input placeholder={t("keywords.additionPlaceholder")} value={term} maxLength={maxTitleKeywordTermChars * 2} onChange={(event) => { setTerm(event.target.value); setAddError(""); }} onKeyDown={(event) => event.key === "Enter" && add()} /></div>
        {category !== "code" && <select aria-label={t("keywords.language")} value={language} onChange={(event) => chooseLanguage(event.target.value)}>{bundledLanguages.map((code) => <option value={code} key={code}>{languageLabel(code)}</option>)}</select>}
        <select aria-label={t("keywords.category")} value={category} onChange={(event) => chooseCategory(event.target.value as TitleKeywordCategory)}><option value="sexual">{categoryLabel("sexual")}</option><option value="organ">{categoryLabel("organ")}</option><option value="code">{categoryLabel("code")}</option></select>
        <select aria-label={t("keywords.matchMode")} value={match} disabled={category === "code"} onChange={(event) => { setMatch(event.target.value as TitleKeywordMatch); setAddError(""); }}>{category === "code" ? <option value="code">{matchLabel("code")}</option> : <><option value="word">{matchLabel("word")}</option><option value="substring">{matchLabel("substring")}</option><option value="prefix">{matchLabel("prefix")}</option></>}</select>
        <button className="primary-button" onClick={add}><Plus size={17} />{t("keywords.additionAdd")}</button>
      </div>
      {addError && <div className="form-error keyword-error"><CircleAlert size={16} />{addError}</div>}
      <p className="keyword-add-hint">{t("keywords.additionHint")}</p>
    </div>
    <div className="toolbar keyword-filters">
      <div className="input-with-icon"><Search size={16} /><input placeholder={t("keywords.searchPlaceholder")} value={search} onChange={(event) => setSearch(event.target.value)} /></div>
      <select aria-label={t("keywords.language")} value={languageFilter} onChange={(event) => setLanguageFilter(event.target.value)}><option value="">{t("keywords.allLanguages")}</option>{languages.map((code) => <option value={code} key={code}>{languageLabel(code)}</option>)}</select>
      <select aria-label={t("keywords.category")} value={categoryFilter} onChange={(event) => setCategoryFilter(event.target.value as "" | TitleKeywordCategory)}><option value="">{t("keywords.allCategories")}</option><option value="sexual">{categoryLabel("sexual")}</option><option value="organ">{categoryLabel("organ")}</option><option value="code">{categoryLabel("code")}</option></select>
      <select aria-label={t("keywords.confidence")} value={confidenceFilter} onChange={(event) => setConfidenceFilter(event.target.value as "" | BuiltInEntry["confidence"])}><option value="">{t("keywords.allConfidences")}</option><option value="strict">{confidenceLabel("strict")}</option><option value="ambiguous">{confidenceLabel("ambiguous")}</option><option value="added">{confidenceLabel("added")}</option></select>
    </div>
    {groups.length ? (
      <div className="keyword-groups">
        {groups.map((group) => {
          const open = filtering || expanded.has(group.key);
          return <div className="keyword-group" key={group.key}>
            <button type="button" className="keyword-group-head" aria-expanded={open} onClick={() => toggleGroup(group.key)}>
              {open ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
              <strong>{group.key === codeGroupKey ? categoryLabel("code") : languageLabel(group.key)}</strong>
              <small>{filtering ? t("keywords.filteredCount", { shown: group.entries.length, total: group.total }) : t("keywords.count", { count: group.total })}</small>
            </button>
            {open && <div className="keyword-chips">{group.entries.map((entry) => {
              const inactive = entry.confidence === "ambiguous" && !ambiguousEnabled;
              const detail = `${categoryLabel(entry.category)} · ${matchLabel(entry.match)} · ${confidenceLabel(entry.confidence)}${inactive ? ` · ${t("keywords.notEnforced")}` : ""}${entry.match === "code" ? `\n${t("keywords.codeChipHint", { prefix: entry.term })}` : ""}`;
              return <span className={`keyword-chip ${entry.confidence}${inactive ? " inactive" : ""}`} key={entry.addition?.id ?? `${entry.match}:${entry.term}`} title={detail}>
                {entry.term}
                {entry.match === "prefix" && <i>…</i>}
                {entry.match === "code" && <i>-###</i>}
                {entry.addition && <button type="button" aria-label={t("keywords.delete", { name: entry.term })} onClick={() => remove(entry.addition!.id)}><X size={12} /></button>}
              </span>;
            })}</div>}
          </div>;
        })}
      </div>
    ) : <EmptyState icon={<Search />} title={t("keywords.searchNoMatch")} text={t("keywords.builtInEmpty")} />}
    <p className="card-footnote">{t("keywords.builtInNote")}</p>
  </Card>;
}

function Keywords({ state, update }: { state: ConsoleState; update: (state: ConsoleState) => void }) {
  const { t } = useI18n();
  const [phrase, setPhrase] = useState("");
  const [category, setCategory] = useState<KeywordRule["category"]>("sensitive");
  const [addError, setAddError] = useState("");
  const [filter, setFilter] = useState("");
  const [editing, setEditing] = useState<KeywordRule | null>(null);
  const [editPhrase, setEditPhrase] = useState("");
  const [editCategory, setEditCategory] = useState<KeywordRule["category"]>("sensitive");
  const [editError, setEditError] = useState("");
  const categoryLabel = (value: KeywordRule["category"]) => t(value === "high_risk" ? "keywords.highRisk" : value === "sensitive" ? "keywords.sensitive" : "keywords.exemptionShort");
  const duplicateOf = (candidatePhrase: string, candidateCategory: KeywordRule["category"], ignoreId?: string) => state.keywords.some((rule) => rule.id !== ignoreId && rule.phrase === candidatePhrase && rule.category === candidateCategory);
  function add() {
    const trimmed = phrase.trim();
    if (!trimmed) return;
    if (duplicateOf(trimmed, category)) return setAddError(t("keywords.duplicate"));
    update({ ...state, keywords: [...state.keywords, { id: crypto.randomUUID(), phrase: trimmed, category, enabled: true }] });
    setPhrase("");
    setAddError("");
  }
  function openEdit(rule: KeywordRule) { setEditing(rule); setEditPhrase(rule.phrase); setEditCategory(rule.category); setEditError(""); }
  function saveEdit() {
    if (!editing) return;
    const trimmed = editPhrase.trim();
    if (!trimmed) return;
    if (duplicateOf(trimmed, editCategory, editing.id)) return setEditError(t("keywords.duplicate"));
    update({ ...state, keywords: state.keywords.map((item) => item.id === editing.id ? { ...item, phrase: trimmed, category: editCategory } : item) });
    setEditing(null);
  }
  const normalizedFilter = filter.trim().toLowerCase();
  const visibleKeywords = normalizedFilter ? state.keywords.filter((rule) => rule.phrase.toLowerCase().includes(normalizedFilter)) : state.keywords;
  return <div className="page-stack">
    <div className="privacy-banner"><Search size={18} /><span><strong>{t("keywords.bannerTitle")}</strong> {t("keywords.bannerDescription")}</span></div>
    <Card>
      <div className="card-heading">
        <div><h2>{t("keywords.customTitle")}</h2></div>
        {state.keywords.length > 0 && (
          <div className="input-with-icon heading-search">
            <Search size={16} />
            <input placeholder={t("keywords.searchPlaceholder")} value={filter} onChange={(event) => setFilter(event.target.value)} />
          </div>
        )}
      </div>
      <div className="toolbar">
        <div className="input-with-icon"><Search size={17} /><input placeholder={t("keywords.placeholder")} value={phrase} onChange={(event) => { setPhrase(event.target.value); setAddError(""); }} onKeyDown={(event) => event.key === "Enter" && add()} /></div>
        <select value={category} onChange={(event) => setCategory(event.target.value as KeywordRule["category"])}><option value="high_risk">{t("keywords.highRisk")}</option><option value="sensitive">{t("keywords.sensitive")}</option><option value="exemption">{t("keywords.exemption")}</option></select>
        <button className="primary-button" onClick={add}><Plus size={17} />{t("keywords.add")}</button>
      </div>
      {addError && <div className="form-error keyword-error"><CircleAlert size={16} />{addError}</div>}
      {state.keywords.length ? (
        visibleKeywords.length ? (
          <div className="table">
            <div className="table-head"><span>{t("keywords.keyword")}</span><span>{t("keywords.category")}</span><span>{t("keywords.status")}</span><span /></div>
            {visibleKeywords.map((rule) => (
              <div className="table-row" key={rule.id}>
                <strong>{rule.phrase}</strong>
                <span><span className={`tag ${rule.category}`}>{categoryLabel(rule.category)}</span></span>
                <span><Toggle checked={rule.enabled} onChange={(enabled) => update({ ...state, keywords: state.keywords.map((item) => item.id === rule.id ? { ...item, enabled } : item) })} label={t("keywords.enable", { name: rule.phrase })} /></span>
                <span className="row-actions">
                  <button className="icon-button" aria-label={t("keywords.edit", { name: rule.phrase })} onClick={() => openEdit(rule)}><Pencil size={16} /></button>
                  <button className="icon-button danger" aria-label={t("keywords.delete", { name: rule.phrase })} onClick={() => update({ ...state, keywords: state.keywords.filter((item) => item.id !== rule.id) })}><Trash2 size={16} /></button>
                </span>
              </div>
            ))}
          </div>
        ) : <EmptyState icon={<Search />} title={t("keywords.searchNoMatch")} text={t("keywords.emptyDescription")} />
      ) : <EmptyState icon={<Search />} title={t("keywords.emptyTitle")} text={t("keywords.emptyDescription")} />}
    </Card>
    <BuiltInKeywordsCard state={state} update={update} />
    {editing && (
      <Modal title={t("keywords.editTitle")} onClose={() => setEditing(null)}>
        <label className="field"><span>{t("keywords.phraseLabel")}</span><input autoFocus value={editPhrase} onChange={(event) => { setEditPhrase(event.target.value); setEditError(""); }} onKeyDown={(event) => event.key === "Enter" && saveEdit()} /></label>
        <label className="field"><span>{t("keywords.category")}</span><select value={editCategory} onChange={(event) => setEditCategory(event.target.value as KeywordRule["category"])}><option value="high_risk">{t("keywords.highRisk")}</option><option value="sensitive">{t("keywords.sensitive")}</option><option value="exemption">{t("keywords.exemption")}</option></select></label>
        {editError && <div className="form-error"><CircleAlert size={16} />{editError}</div>}
        <button className="primary-button modal-submit" disabled={!editPhrase.trim()} onClick={saveEdit}>{t("keywords.save")}</button>
      </Modal>
    )}
  </div>;
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

function NumberField({ label, value, min, max, onCommit }: { label: string; value: number; min: number; max: number; onCommit: (value: number) => void }) {
  const { t } = useI18n();
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const accepts = (candidate: string) => /^\d+$/.test(candidate) && Number(candidate) >= min && Number(candidate) <= max;
  return <label className="field"><span>{label}</span><input type="number" inputMode="numeric" min={min} max={max} step={1} value={text} onChange={(event) => { const next = event.target.value; setText(next); if (accepts(next)) onCommit(Number(next)); }} onBlur={() => setText(String(value))} />{!accepts(text) && <small className="field-error">{t("applications.repeatRange", { min, max })}</small>}</label>;
}

function RepeatOffenderCard({ state, update, sessionToken, onBannedAllowed }: { state: ConsoleState; update: (state: ConsoleState) => void; sessionToken: string; onBannedAllowed: ApplicationAllowedHandler }) {
  const { t } = useI18n();
  const dateTime = useShortDateTime();
  const authorize = useSuperAuthorization();
  const [busyExecutable, setBusyExecutable] = useState<string>();
  const [error, setError] = useState("");
  const settings = state.repeatOffender;
  async function allow(executable: string) {
    setBusyExecutable(executable);
    setError("");
    try {
      await authorize(async (superPassword) => onBannedAllowed(executable, await allowBannedApplication(sessionToken, executable, superPassword)));
    } catch (reason) {
      if (!isAuthorizationCancelled(reason)) setError(localizeError(reason, t));
    } finally {
      setBusyExecutable(undefined);
    }
  }
  const patch = (next: Partial<ConsoleState["repeatOffender"]>) => update({ ...state, repeatOffender: { ...settings, ...next } });
  return <Card>
    <div className="setting-row"><div className="setting-icon amber"><Ban /></div><div><strong>{t("applications.repeatTitle")}</strong><p>{t("applications.repeatDescription", { minutes: settings.windowMinutes, count: settings.strikeLimit })}</p></div></div>
    <div className="repeat-fields">
      <NumberField label={t("applications.repeatWindow")} value={settings.windowMinutes} min={repeatOffenderLimits.windowMinutes.min} max={repeatOffenderLimits.windowMinutes.max} onCommit={(windowMinutes) => patch({ windowMinutes })} />
      <NumberField label={t("applications.repeatCount")} value={settings.strikeLimit} min={repeatOffenderLimits.strikeLimit.min} max={repeatOffenderLimits.strikeLimit.max} onCommit={(strikeLimit) => patch({ strikeLimit })} />
    </div>
    <div className="learned-list">
      <span className="learned-heading">{t("applications.activeBans")}</span>
      {state.activeBans.length ? state.activeBans.map((ban) => <div className="app-rule" key={ban.executable}><div className="app-avatar"><AppWindow /></div><div className="app-main"><strong>{executableFileName(ban.executable)}</strong><small>{ban.executable}</small><small>{banStrikeSummary(state.audit, ban, settings.windowMinutes, t)}</small></div><StatusPill tone="danger">{t("applications.bannedUntil", { time: dateTime.format(ban.bannedUntilMs) })}</StatusPill><button className="secondary-button" disabled={busyExecutable !== undefined || !state.serviceConnected || isBrowserExecutable(state.applications, ban.executable)} title={t(isBrowserExecutable(state.applications, ban.executable) ? "applications.allowBannedBrowser" : "applications.allowBannedHint")} onClick={() => void allow(ban.executable)}><Check size={16} />{t(busyExecutable === ban.executable ? "common.saving" : "applications.allowBanned")}</button></div>) : <p className="learned-empty">{t("applications.noActiveBans")}</p>}
      {error && <div className="form-error"><CircleAlert size={16} />{error}</div>}
    </div>
  </Card>;
}

/** Applies application rules the Service saved directly; `executable` is the program now allowed. */
type ApplicationAllowedHandler = (executable: string, applications: ApplicationRule[]) => void;

/** Most installed applications listed at once; a search narrows the rest. */
const installedListLimit = 200;

function InstalledApplicationPicker({ applications, error, onPick }: { applications?: InstalledApplication[]; error: string; onPick: (application: InstalledApplication) => void }) {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const needle = query.trim().toLowerCase();
  const matches = (applications ?? []).filter((item) => !needle || [item.name, item.executable, item.publisher ?? ""].some((value) => value.toLowerCase().includes(needle)));
  const shown = matches.slice(0, installedListLimit);
  let body: ReactNode;
  if (error) body = <div className="form-error"><CircleAlert size={16} />{error}</div>;
  else if (!applications) body = <p className="learned-empty">{t("applications.installedLoading")}</p>;
  else if (!shown.length) body = <p className="learned-empty">{t(applications.length ? "applications.installedNoMatch" : "applications.installedEmpty")}</p>;
  else body = <>
    <div className="installed-list">{shown.map((item) => <button type="button" className="installed-item" key={item.executable} title={item.executable} onClick={() => onPick(item)}><AppWindow size={16} /><span><strong>{item.name}</strong><small>{item.publisher ? `${item.publisher} · ${item.executable}` : item.executable}</small></span></button>)}</div>
    {matches.length > shown.length && <small className="field-hint">{t("applications.installedShown", { shown: shown.length, total: matches.length })}</small>}
  </>;
  return <div className="installed-picker"><label className="field"><span>{t("applications.installedSearch")}</span><input value={query} autoFocus disabled={!applications?.length} onChange={(event) => setQuery(event.target.value)} placeholder={t("applications.installedSearchPlaceholder")} /></label>{body}</div>;
}

/** Scans installed applications and adds the checked ones as "Allow" rules to the draft. */
function BatchAllowApplications({ state, update, sessionToken, onClose }: { state: ConsoleState; update: (state: ConsoleState) => void; sessionToken: string; onClose: () => void }) {
  const { t } = useI18n();
  const [found, setFound] = useState<InstalledApplication[]>();
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [checked, setChecked] = useState<Set<string>>(() => new Set());
  useEffect(() => {
    let current = true;
    listInstalledApplications(sessionToken)
      .then((applications) => { if (current) setFound(applications); })
      .catch((reason) => { if (current) { setFound([]); setError(localizeError(reason, t)); } });
    return () => { current = false; };
  }, [sessionToken]);
  // Browsers can never be allowed here: allowing one exempts every page it shows.
  const rows = useMemo(() => (found ?? []).map((application) => ({ application, effect: applicationEffect(state.applications, application.executable), browser: Boolean(application.browser) || isBrowserExecutable(state.applications, application.executable) })), [found, state.applications]);
  const needle = query.trim().toLowerCase();
  const visible = rows.filter(({ application }) => !needle || [application.name, application.executable, application.publisher ?? ""].some((value) => value.toLowerCase().includes(needle)));
  const bulk = visible.filter(({ effect, browser }) => effect !== "allow" && !browser).map(({ application }) => application.executable);
  const allChecked = bulk.length > 0 && bulk.every((executable) => checked.has(executable));
  const chosen = rows.filter(({ application, effect, browser }) => effect !== "allow" && !browser && checked.has(application.executable)).map(({ application }) => application);
  const nextRules = useMemo(() => allowApplications(state.applications, chosen), [state.applications, rows, checked]);
  const overLimit = nextRules.length > maxApplicationRules;
  function toggle(executable: string, value: boolean) { setChecked((current) => { const next = new Set(current); if (value) next.add(executable); else next.delete(executable); return next; }); }
  function toggleAll(value: boolean) { setChecked((current) => { const next = new Set(current); bulk.forEach((executable) => value ? next.add(executable) : next.delete(executable)); return next; }); }
  function confirm() { if (!chosen.length || overLimit) return; update({ ...state, applications: nextRules }); onClose(); }
  return <Modal title={t("applications.batchTitle")} onClose={onClose} wide>
    <p className="modal-description">{t("applications.batchDescription")}</p>
    {found === undefined ? <p className="batch-status">{t("applications.batchScanning")}</p> : <>
      <div className="batch-toolbar">
        <div className="learned-search"><Search size={14} /><input value={query} placeholder={t("applications.batchSearch")} onChange={(event) => setQuery(event.target.value)} /></div>
        <label className="evidence-check-all"><input type="checkbox" checked={allChecked} disabled={!bulk.length} onChange={(event) => toggleAll(event.target.checked)} />{t("applications.batchSelectAll")}</label>
      </div>
      <div className="batch-list">{visible.length ? visible.map(({ application, effect, browser }) => {
        const allowed = effect === "allow";
        return <label className={`batch-app${allowed || browser ? " is-disabled" : ""}`} key={application.executable} title={browser ? t("applications.batchBrowserHint") : undefined}>
          <input type="checkbox" checked={allowed || (!browser && checked.has(application.executable))} disabled={allowed || browser} onChange={(event) => toggle(application.executable, event.target.checked)} />
          <div className="app-main"><strong>{application.name}</strong><small>{application.publisher ? `${application.publisher} · ${application.executable}` : application.executable}</small></div>
          {allowed && <span className="tag allow">{t("applications.batchAlreadyAllowed")}</span>}
          {effect === "block" && <span className="tag block">{t("applications.batchCurrentlyBlocked")}</span>}
          {browser && <span className="tag sensitive">{t("applications.browser")}</span>}
        </label>;
      }) : <p className="learned-empty">{t("applications.batchEmpty")}</p>}</div>
      {overLimit && <div className="form-error"><CircleAlert size={16} />{t("applications.batchTooMany", { max: maxApplicationRules })}</div>}
      {error && <div className="form-error"><CircleAlert size={16} />{error}</div>}
    </>}
    <div className="modal-actions"><button className="secondary-button" onClick={onClose}>{t("common.cancel")}</button><button className="primary-button" disabled={!chosen.length || overLimit} onClick={confirm}><Check size={16} />{t("applications.batchConfirm", { count: chosen.length })}</button></div>
  </Modal>;
}

const builtInCategories: Array<{ id: BuiltInApplicationCategory; label: MessageKey }> = [
  { id: "browser", label: "applications.browser" },
  { id: "player", label: "applications.player" },
  { id: "game", label: "applications.game" },
  { id: "custom", label: "applications.custom" },
];

/** Mirror the Service's application category limits. */
const maxApplicationCategories = 64;
const maxCategoryNameChars = 64;

type CategoryOption = { id: string; name: string; builtIn: boolean };

function categoryOptions(state: ConsoleState, t: Translate): CategoryOption[] {
  return [
    ...builtInCategories.map((category) => ({ id: category.id, name: t(category.label), builtIn: true })),
    ...state.applicationCategories.map((category) => ({ ...category, builtIn: false })),
  ];
}

function categoryName(state: ConsoleState, id: string, t: Translate): string {
  return categoryOptions(state, t).find((category) => category.id === id)?.name ?? id;
}

function CategorySelect({ state, value, onChange, disabled = false, label }: { state: ConsoleState; value: string; onChange: (value: string) => void; disabled?: boolean; label?: string }) {
  const { t } = useI18n();
  const options = categoryOptions(state, t);
  return <select value={value} disabled={disabled} aria-label={label} onChange={(event) => onChange(event.target.value)}>
    {options.map((category) => <option key={category.id} value={category.id}>{category.name}</option>)}
    {!options.some((category) => category.id === value) && <option value={value}>{value}</option>}
  </select>;
}

/** Lists and edits the programs in one category; changes join the unsaved draft. */
function CategoryMembersModal({ state, update, sessionToken, categoryId, onClose }: { state: ConsoleState; update: (state: ConsoleState) => void; sessionToken: string; categoryId: string; onClose: () => void }) {
  const { t } = useI18n();
  const [source, setSource] = useState<"installed" | "manual">("installed");
  const [installed, setInstalled] = useState<InstalledApplication[]>();
  const [installedError, setInstalledError] = useState("");
  const [draft, setDraft] = useState({ name: "", executable: "" });
  const [error, setError] = useState("");
  useEffect(() => {
    let current = true;
    listInstalledApplications(sessionToken)
      .then((applications) => { if (current) setInstalled(applications); })
      .catch((reason) => { if (current) { setInstalled([]); setInstalledError(localizeError(reason, t)); } });
    return () => { current = false; };
  }, [sessionToken]);
  const name = categoryName(state, categoryId, t);
  const members = state.applications.filter((rule) => rule.category === categoryId);
  const memberExecutables = new Set(members.map((rule) => normalizeExecutable(rule.executable)));
  const candidates = installed?.filter((application) => !memberExecutables.has(normalizeExecutable(application.executable)));
  function add(applications: Array<{ name: string; executable: string }>) {
    const next = assignApplicationsToCategory(state.applications, applications, categoryId);
    if (next.length > maxApplicationRules) { setError(t("applications.batchTooMany", { max: maxApplicationRules })); return; }
    update({ ...state, applications: next });
    setError("");
  }
  function addManual() {
    if (!draft.executable.trim()) { setError(t("categories.errorExecutable")); return; }
    add([{ name: draft.name.trim() || executableFileName(draft.executable.trim()), executable: draft.executable.trim() }]);
    setDraft({ name: "", executable: "" });
  }
  return <Modal title={t("categories.membersTitle", { name })} onClose={onClose} wide>
    <p className="modal-description">{t(categoryId === "browser" ? "categories.membersBrowserDescription" : "categories.membersDescription")}</p>
    <div className="learned-list">
      <span className="learned-heading">{t("categories.members", { count: members.filter((rule) => !isPlaceholderApplication(rule)).length })}</span>
      {members.length ? members.map((rule) => {
        const ruleName = localizedApplicationName(rule, t);
        return <div className="app-rule" key={rule.id}>
          <div className="app-avatar"><AppWindow /></div>
          <div className="app-main"><strong>{ruleName}</strong><small>{localizedExecutable(rule, t)}</small></div>
          {!rule.enabled && <StatusPill tone="muted">{t("categories.disabledRule")}</StatusPill>}
          {!isPlaceholderApplication(rule) && <button className="icon-button danger" aria-label={t("categories.removeMember", { name: ruleName })} title={t("categories.removeMember", { name: ruleName })} onClick={() => update({ ...state, applications: removeFromCategory(state.applications, rule.id) })}><X size={16} /></button>}
        </div>;
      }) : <p className="learned-empty">{t("categories.noMembers")}</p>}
    </div>
    <div className="learned-tabs source-toggle" role="group" aria-label={t("categories.addMembers")}>
      <button type="button" className={source === "installed" ? "active" : ""} aria-pressed={source === "installed"} onClick={() => setSource("installed")}><Search size={14} />{t("applications.sourceInstalled")}</button>
      <button type="button" className={source === "manual" ? "active" : ""} aria-pressed={source === "manual"} onClick={() => setSource("manual")}><Pencil size={14} />{t("applications.sourceManual")}</button>
    </div>
    {source === "installed" ? <InstalledApplicationPicker applications={candidates} error={installedError} onPick={(application) => add([application])} /> : <>
      <label className="field"><span>{t("applications.displayName")}</span><input value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} placeholder={t("applications.namePlaceholder")} /></label>
      <label className="field"><span>{t("applications.executable")}</span><input value={draft.executable} onChange={(event) => { setDraft({ ...draft, executable: event.target.value }); setError(""); }} onKeyDown={(event) => event.key === "Enter" && addManual()} placeholder={t("applications.executablePlaceholder")} /></label>
      <button className="secondary-button" onClick={addManual}><Plus size={16} />{t("categories.addMember")}</button>
    </>}
    {error && <div className="form-error"><CircleAlert size={16} />{error}</div>}
    <small className="field-hint">{t("categories.pendingSave")}</small>
    <div className="modal-actions"><button className="primary-button" onClick={onClose}><Check size={16} />{t("categories.done")}</button></div>
  </Modal>;
}

function ApplicationCategoriesCard({ state, update, sessionToken }: { state: ConsoleState; update: (state: ConsoleState) => void; sessionToken: string }) {
  const { t } = useI18n();
  const [name, setName] = useState("");
  const [error, setError] = useState("");
  const [managing, setManaging] = useState<string>();
  const [renaming, setRenaming] = useState<{ id: string; name: string }>();
  const [renameError, setRenameError] = useState("");
  const categories = categoryOptions(state, t);
  const memberCount = (id: string) => state.applications.filter((rule) => rule.category === id && !isPlaceholderApplication(rule)).length;
  const usedBy = (id: string) => state.schedules.filter((rule) => rule.target === "categories" && rule.categories.includes(id));
  function nameError(value: string, ignoreId?: string): string | undefined {
    const trimmed = value.trim();
    if (!trimmed) return t("categories.errorName");
    if (trimmed.length > maxCategoryNameChars) return t("categories.errorNameLength", { max: maxCategoryNameChars });
    if (categories.some((category) => category.id !== ignoreId && category.name.toLowerCase() === trimmed.toLowerCase())) return t("categories.errorDuplicate");
    return undefined;
  }
  function add() {
    const problem = nameError(name) ?? (state.applicationCategories.length >= maxApplicationCategories ? t("categories.errorTooMany", { max: maxApplicationCategories }) : undefined);
    if (problem) { setError(problem); return; }
    update({ ...state, applicationCategories: [...state.applicationCategories, { id: `cat-${crypto.randomUUID()}`, name: name.trim() }] });
    setName("");
    setError("");
  }
  function saveRename() {
    if (!renaming) return;
    const problem = nameError(renaming.name, renaming.id);
    if (problem) { setRenameError(problem); return; }
    update({ ...state, applicationCategories: state.applicationCategories.map((category) => category.id === renaming.id ? { ...category, name: renaming.name.trim() } : category) });
    setRenaming(undefined);
  }
  // Members of a deleted category stay as rules and move to "Other".
  function remove(id: string) {
    update({ ...state, applicationCategories: state.applicationCategories.filter((category) => category.id !== id), applications: state.applications.map((rule) => rule.category === id ? { ...rule, category: "custom" } : rule) });
  }
  return <Card>
    <div className="card-heading"><div><h2>{t("categories.title")}</h2><p>{t("categories.description")}</p></div></div>
    <div className="toolbar">
      <div className="input-with-icon"><Tags size={17} /><input value={name} placeholder={t("categories.namePlaceholder")} onChange={(event) => { setName(event.target.value); setError(""); }} onKeyDown={(event) => event.key === "Enter" && add()} /></div>
      <button className="primary-button" onClick={add}><Plus size={17} />{t("categories.add")}</button>
    </div>
    {error && <div className="form-error"><CircleAlert size={16} />{error}</div>}
    {categories.map((category) => {
      const used = usedBy(category.id);
      const details = [t(category.builtIn ? "categories.builtIn" : "categories.userDefined"), t("categories.memberCount", { count: memberCount(category.id) })];
      if (used.length) details.push(t("categories.usedBy", { names: used.map((rule) => rule.name).join("、") }));
      return <div className="app-rule" key={category.id}>
        <div className="app-avatar"><Tags /></div>
        <div className="app-main"><strong>{category.name}</strong><small>{details.join(" · ")}</small></div>
        <button className="secondary-button" onClick={() => setManaging(category.id)}><ListChecks size={16} />{t("categories.manage")}</button>
        {!category.builtIn && <>
          <button className="icon-button" aria-label={t("categories.rename", { name: category.name })} title={t("categories.rename", { name: category.name })} onClick={() => { setRenaming({ id: category.id, name: category.name }); setRenameError(""); }}><Pencil size={16} /></button>
          <button className="icon-button danger" disabled={used.length > 0} aria-label={t("categories.delete", { name: category.name })} title={t(used.length ? "categories.deleteInUse" : "categories.delete", { name: category.name })} onClick={() => remove(category.id)}><Trash2 size={16} /></button>
        </>}
      </div>;
    })}
    {managing && <CategoryMembersModal state={state} update={update} sessionToken={sessionToken} categoryId={managing} onClose={() => setManaging(undefined)} />}
    {renaming && <Modal title={t("categories.renameTitle")} onClose={() => setRenaming(undefined)}>
      <label className="field"><span>{t("categories.name")}</span><input value={renaming.name} autoFocus onChange={(event) => { setRenaming({ ...renaming, name: event.target.value }); setRenameError(""); }} onKeyDown={(event) => event.key === "Enter" && saveRename()} /></label>
      {renameError && <div className="form-error"><CircleAlert size={16} />{renameError}</div>}
      <button className="primary-button modal-submit" onClick={saveRename}><Check size={17} />{t("categories.renameSave")}</button>
    </Modal>}
  </Card>;
}

function Applications({ state, update, sessionToken, onBannedAllowed }: { state: ConsoleState; update: (state: ConsoleState) => void; sessionToken: string; onBannedAllowed: ApplicationAllowedHandler }) {
  const { t } = useI18n();
  const [showAdd, setShowAdd] = useState(false);
  const [showBatch, setShowBatch] = useState(false);
  const [source, setSource] = useState<"manual" | "installed">("manual");
  const [installed, setInstalled] = useState<InstalledApplication[]>();
  const [installedError, setInstalledError] = useState("");
  const loadingInstalled = useRef(false);
  const [draft, setDraft] = useState({ name: "", executable: "", category: "custom" });
  function closeAdd() { setShowAdd(false); setSource("manual"); }
  function add() { if (!draft.name.trim() || !draft.executable.trim()) return; update({ ...state, applications: [...state.applications, { id: crypto.randomUUID(), ...draft, action: "content_only", enabled: true }] }); closeAdd(); setDraft({ name: "", executable: "", category: "custom" }); }
  function chooseInstalled() {
    setSource("installed");
    if (installed || loadingInstalled.current) return;
    loadingInstalled.current = true;
    setInstalledError("");
    listInstalledApplications(sessionToken)
      .then(setInstalled, (reason) => setInstalledError(localizeError(reason, t)))
      .finally(() => { loadingInstalled.current = false; });
  }
  // Picking fills the form; the administrator still reviews the category and saves.
  function pick(application: InstalledApplication) { setDraft({ ...draft, name: application.name, executable: application.executable }); setSource("manual"); }
  function patchRule(id: string, next: Partial<ApplicationRule>) { update({ ...state, applications: state.applications.map((item) => item.id === id ? { ...item, ...next } : item) }); }
  const sourceToggle = <div className="learned-tabs source-toggle" role="group" aria-label={t("applications.executable")}>
    <button type="button" className={source === "manual" ? "active" : ""} aria-pressed={source === "manual"} onClick={() => setSource("manual")}><Pencil size={14} />{t("applications.sourceManual")}</button>
    <button type="button" className={source === "installed" ? "active" : ""} aria-pressed={source === "installed"} onClick={chooseInstalled}><Search size={14} />{t("applications.sourceInstalled")}</button>
  </div>;
  return <div className="page-stack"><div className="privacy-banner"><AppWindow size={18} /><span><strong>{t("applications.bannerTitle")}</strong> {t("applications.bannerDescription")}</span></div><RepeatOffenderCard state={state} update={update} sessionToken={sessionToken} onBannedAllowed={onBannedAllowed} /><ApplicationCategoriesCard state={state} update={update} sessionToken={sessionToken} /><div className="page-actions"><button className="secondary-button" onClick={() => setShowBatch(true)}><ListChecks size={17} />{t("applications.batchAdd")}</button><button className="primary-button" onClick={() => setShowAdd(true)}><Plus size={17} />{t("applications.add")}</button></div><Card>{state.applications.map((rule) => { const name = localizedApplicationName(rule, t); return <div className="app-rule" key={rule.id}><div className="app-avatar"><AppWindow /></div><div className="app-main"><strong>{name}</strong><small>{localizedExecutable(rule, t)}</small></div><CategorySelect state={state} value={rule.category} disabled={isPlaceholderApplication(rule)} label={t("applications.categoryOf", { name })} onChange={(category) => patchRule(rule.id, { category })} /><select value={rule.action} aria-label={t("applications.actionOf", { name })} onChange={(event) => patchRule(rule.id, { action: event.target.value as ApplicationRule["action"] })}><option value="content_only">{t("applications.contentOnly")}</option><option value="block">{t("applications.block")}</option><option value="allow">{t("applications.allow")}</option></select><Toggle checked={rule.enabled} onChange={(enabled) => patchRule(rule.id, { enabled })} label={t("applications.enable", { name })} /></div>; })}</Card>{showAdd && <Modal title={t("applications.modalTitle")} onClose={closeAdd}>{sourceToggle}{source === "installed" ? <InstalledApplicationPicker applications={installed} error={installedError} onPick={pick} /> : <><label className="field"><span>{t("applications.displayName")}</span><input value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} placeholder={t("applications.namePlaceholder")} /></label><label className="field"><span>{t("applications.executable")}</span><input value={draft.executable} onChange={(event) => setDraft({ ...draft, executable: event.target.value })} placeholder={t("applications.executablePlaceholder")} /></label><label className="field"><span>{t("applications.category")}</span><CategorySelect state={state} value={draft.category} onChange={(category) => setDraft({ ...draft, category })} /></label><button className="primary-button modal-submit" onClick={add}>{t("applications.save")}</button></>}</Modal>}{showBatch && <BatchAllowApplications state={state} update={update} sessionToken={sessionToken} onClose={() => setShowBatch(false)} />}</div>;
}

const dayKeys: MessageKey[] = ["schedule.dayMon", "schedule.dayTue", "schedule.dayWed", "schedule.dayThu", "schedule.dayFri", "schedule.daySat", "schedule.daySun"];
const dayNameKeys: MessageKey[] = ["schedule.dayNameMon", "schedule.dayNameTue", "schedule.dayNameWed", "schedule.dayNameThu", "schedule.dayNameFri", "schedule.dayNameSat", "schedule.dayNameSun"];
const networkTargets: Array<{ value: NetworkTarget; label: MessageKey; hint: MessageKey }> = [
  { value: "categories", label: "schedule.targetCategories", hint: "schedule.targetCategoriesHint" },
  { value: "applications", label: "schedule.targetApplications", hint: "schedule.targetApplicationsHint" },
  { value: "all_network", label: "schedule.targetAllNetwork", hint: "schedule.targetAllNetworkHint" },
];
const scheduleActions: Array<{ value: ScheduleAction; label: MessageKey; hint: MessageKey }> = [
  { value: "block_network", label: "schedule.actionBlockNetwork", hint: "schedule.actionBlockNetworkHint" },
  { value: "block_launch", label: "schedule.actionBlockLaunch", hint: "schedule.actionBlockLaunchHint" },
];

type ScheduleDraft = Omit<ScheduleRule, "id">;

function newScheduleDraft(name: string): ScheduleDraft {
  return { name, action: "block_network", target: "categories", categories: ["browser"], applications: [], blocks: [0, 1, 2, 3, 4].map((day) => ({ day, start: "21:00", end: "07:00" })), enabled: true };
}

function sortBlocks(blocks: ScheduleBlock[]): ScheduleBlock[] {
  return [...blocks].sort((left, right) => left.day - right.day || left.start.localeCompare(right.start));
}

function scheduleDraftError(draft: ScheduleDraft): MessageKey | undefined {
  if (!draft.name.trim()) return "schedule.errorName";
  if (!draft.blocks.length) return "schedule.errorNoBlocks";
  if (draft.blocks.some((block) => !block.start || !block.end || block.start === block.end)) return "schedule.errorEmptyBlock";
  if (draft.target === "applications" && !draft.applications.length) return "schedule.errorNoApplications";
  if (draft.target === "categories" && !draft.categories.length) return "schedule.errorNoCategories";
  if (draft.target === "all_network" && draft.action === "block_launch") return "schedule.errorLaunchAllNetwork";
  return undefined;
}

function formatNetworkTime(ms: number, locale: Locale): string {
  const date = new Date(ms);
  const now = new Date();
  const sameDay = date.toDateString() === now.toDateString();
  return new Intl.DateTimeFormat(locale, sameDay ? { hour: "2-digit", minute: "2-digit", hour12: false } : { weekday: "short", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false }).format(date);
}

function NetworkStatusCard({ state, sessionToken, onNetworkChange }: { state: ConsoleState; sessionToken: string; onNetworkChange: (network: NetworkStatus) => void }) {
  const { locale, t } = useI18n();
  const [open, setOpen] = useState(false);
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const network = state.network;
  const ruleNames = network.activeRuleIds.map((id) => state.schedules.find((rule) => rule.id === id)?.name ?? id).join("、");
  async function submit() {
    if (!password || busy) return;
    setBusy(true);
    setError("");
    try {
      onNetworkChange(await restoreNetwork(sessionToken, password));
      setOpen(false);
      setPassword("");
    } catch (reason) {
      setError(localizeError(reason, t));
    } finally {
      setBusy(false);
    }
  }
  let icon = <Wifi />;
  let tone = "green";
  let title = t("network.idleTitle");
  let detail = t(state.serviceConnected ? state.superPasswordSet ? "network.idleSuperDescription" : "network.idleDescription" : "network.serviceRequired");
  if (network.restricted) {
    icon = <WifiOff />;
    tone = "amber";
    title = t("network.restrictedTitle");
    detail = network.restrictedUntilMs ? t("network.restrictedUntil", { time: formatNetworkTime(network.restrictedUntilMs, locale) }) : t("network.restrictedOngoing");
  } else if (network.restoredUntilMs) {
    icon = <Wifi />;
    tone = "blue";
    title = t("network.restoredTitle");
    detail = t("network.restoredUntil", { time: formatNetworkTime(network.restoredUntilMs, locale) });
  }
  return (
    <Card>
      <div className="setting-row">
        <div className={`setting-icon ${tone}`}>{icon}</div>
        <div><strong>{title}</strong><p>{detail}{ruleNames && <><br />{t("network.activeRules", { names: ruleNames })}</>}</p></div>
        {network.restricted && <button className="secondary-button" onClick={() => { setOpen(true); setError(""); setPassword(""); }}><KeyRound size={16} />{t("network.restoreButton")}</button>}
      </div>
      {open && (
        <Modal title={t("network.restoreTitle")} onClose={() => setOpen(false)}>
          <p className="modal-description">{t("network.restoreDescription")}</p>
          <label className="field"><span>{t(state.superPasswordSet ? "super.password" : "network.password")}</span><input type="password" value={password} autoFocus autoComplete="current-password" onChange={(event) => setPassword(event.target.value)} onKeyDown={(event) => event.key === "Enter" && void submit()} /></label>
          {error && <div className="form-error"><CircleAlert size={16} />{error}</div>}
          <button className="primary-button modal-submit" disabled={!password || busy} onClick={() => void submit()}><Wifi size={17} />{t(busy ? "network.restoring" : "network.restoreConfirm")}</button>
        </Modal>
      )}
    </Card>
  );
}

function Schedule({ state, update, sessionToken, onNetworkChange }: { state: ConsoleState; update: (state: ConsoleState) => void; sessionToken: string; onNetworkChange: (network: NetworkStatus) => void }) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<ScheduleDraft>(() => newScheduleDraft(t("schedule.defaultName")));
  const [editingId, setEditingId] = useState<string>();
  const [application, setApplication] = useState("");
  const [error, setError] = useState("");
  const knownExecutables = useMemo(() => Array.from(new Set(state.applications.map((rule) => rule.executable).filter((value) => /\.exe$/i.test(value)))), [state.applications]);
  const targetHint = networkTargets.find((target) => target.value === draft.target)?.hint ?? "schedule.targetCategoriesHint";
  const actionHint = scheduleActions.find((action) => action.value === draft.action)?.hint ?? "schedule.actionBlockNetworkHint";
  // "Other" groups programs without a category, so it is not offered as a target.
  const schedulableCategories = categoryOptions(state, t).filter((category) => category.id !== "custom");
  const memberCount = (id: string) => state.applications.filter((rule) => rule.enabled && rule.category === id && !isPlaceholderApplication(rule)).length;
  function patch(next: Partial<ScheduleDraft>) { setDraft({ ...draft, ...next }); setError(""); }
  // Terminating every non-system program is not supported.
  function chooseAction(action: ScheduleAction) { patch(action === "block_launch" && draft.target === "all_network" ? { action, target: "categories" } : { action }); }
  function toggleCategory(id: string, checked: boolean) { patch({ categories: checked ? [...draft.categories, id] : draft.categories.filter((value) => value !== id) }); }
  function patchBlock(index: number, next: Partial<ScheduleBlock>) { patch({ blocks: draft.blocks.map((block, position) => position === index ? { ...block, ...next } : block) }); }
  function addBlock(day: number) {
    // Start a new block where the day's latest same-day block ends; an
    // overnight block leaves the daytime free, so suggest the afternoon.
    const last = sortBlocks(draft.blocks.filter((block) => block.day === day)).at(-1);
    const start = last && last.end > last.start && last.end < "22:00" ? last.end : last ? "14:00" : "19:00";
    const end = `${String(Math.min(Number(start.slice(0, 2)) + 2, 23)).padStart(2, "0")}:${start.slice(3)}`;
    patch({ blocks: [...draft.blocks, { day, start, end }] });
  }
  function copyDay(day: number) {
    const source = draft.blocks.filter((block) => block.day === day);
    patch({ blocks: sortBlocks(dayKeys.flatMap((_, target) => source.map((block) => ({ ...block, day: target })))) });
  }
  function addApplication() {
    const value = application.trim();
    if (!value) return;
    if (!draft.applications.some((item) => item.toLowerCase() === value.toLowerCase())) patch({ applications: [...draft.applications, value] });
    setApplication("");
  }
  function resetDraft() { setDraft(newScheduleDraft(t("schedule.defaultName"))); setEditingId(undefined); setApplication(""); setError(""); }
  function submit() {
    const problem = scheduleDraftError(draft);
    if (problem) { setError(t(problem)); return; }
    const rule: ScheduleRule = { ...draft, name: draft.name.trim(), categories: draft.target === "categories" ? draft.categories : [], applications: draft.target === "applications" ? draft.applications : [], blocks: sortBlocks(draft.blocks), id: editingId ?? crypto.randomUUID() };
    update({ ...state, schedules: editingId ? state.schedules.map((item) => item.id === editingId ? rule : item) : [...state.schedules, rule] });
    resetDraft();
  }
  function edit(rule: ScheduleRule) { const { id, ...rest } = rule; setDraft({ ...rest, blocks: sortBlocks(rest.blocks) }); setEditingId(id); setApplication(""); setError(""); }
  function targetLabel(rule: ScheduleRule) {
    if (rule.target === "categories") return rule.categories.map((id) => categoryName(state, id, t)).join("、");
    return t(networkTargets.find((item) => item.value === rule.target)?.label ?? "schedule.targetApplications");
  }
  function actionLabel(action: ScheduleAction) { return t(scheduleActions.find((item) => item.value === action)?.label ?? "schedule.actionBlockNetwork"); }
  function blockLabel(block: ScheduleBlock) { return `${block.start}–${block.end}${block.end <= block.start ? ` (${t("schedule.nextDay")})` : ""}`; }
  return (
    <div className="page-stack">
      <NetworkStatusCard state={state} sessionToken={sessionToken} onNetworkChange={onNetworkChange} />
      <div className="two-column schedule-layout">
        <Card>
          <div className="card-heading"><div><h2>{t(editingId ? "schedule.editTitle" : "schedule.newTitle")}</h2><p>{t("schedule.description")}</p></div></div>
          <label className="field"><span>{t("schedule.name")}</span><input value={draft.name} onChange={(event) => patch({ name: event.target.value })} /></label>
          <label className="field"><span>{t("schedule.action")}</span><select value={draft.action} onChange={(event) => chooseAction(event.target.value as ScheduleAction)}>{scheduleActions.map((action) => <option key={action.value} value={action.value}>{t(action.label)}</option>)}</select><small className="field-hint">{t(actionHint)}</small></label>
          <label className="field"><span>{t("schedule.target")}</span><select value={draft.target} onChange={(event) => patch({ target: event.target.value as NetworkTarget })}>{networkTargets.filter((target) => draft.action === "block_network" || target.value !== "all_network").map((target) => <option key={target.value} value={target.value}>{t(target.label)}</option>)}</select><small className="field-hint">{t(targetHint)}</small></label>
          {draft.target === "categories" && (
            <div className="field">
              <span>{t("schedule.categories")}</span>
              <div className="schedule-categories">
                {schedulableCategories.map((category) => <label key={category.id} className={`schedule-category${draft.categories.includes(category.id) ? " checked" : ""}`}><input type="checkbox" checked={draft.categories.includes(category.id)} onChange={(event) => toggleCategory(category.id, event.target.checked)} /><span>{category.name}</span><small>{category.id === "browser" ? t("schedule.categoryBrowserCount", { count: memberCount(category.id) }) : t("categories.memberCount", { count: memberCount(category.id) })}</small></label>)}
              </div>
            </div>
          )}
          {draft.target === "applications" && (
            <div className="field">
              <span>{t("schedule.applications")}</span>
              <div className="schedule-app-input"><input list="schedule-known-executables" value={application} placeholder={t("schedule.applicationPlaceholder")} onChange={(event) => { setApplication(event.target.value); setError(""); }} onKeyDown={(event) => event.key === "Enter" && addApplication()} /><button type="button" className="secondary-button" onClick={addApplication}><Plus size={16} />{t("schedule.addApplication")}</button></div>
              <datalist id="schedule-known-executables">{knownExecutables.map((value) => <option key={value} value={value} />)}</datalist>
              {draft.applications.length > 0 && <div className="schedule-apps">{draft.applications.map((item) => <span className="schedule-app" key={item}>{item}<button type="button" aria-label={t("schedule.removeApplication", { name: item })} onClick={() => patch({ applications: draft.applications.filter((value) => value !== item) })}><X size={12} /></button></span>)}</div>}
            </div>
          )}
          <div className="field">
            <span>{t("schedule.blocks")}</span>
            <div className="week-blocks">
              {dayKeys.map((key, day) => {
                const entries = draft.blocks.map((block, index) => ({ block, index })).filter(({ block }) => block.day === day);
                return (
                  <div className="week-day" key={key}>
                    <strong>{t(dayNameKeys[day])}</strong>
                    <div className="week-day-blocks">
                      {entries.length ? entries.map(({ block, index }) => (
                        <div className="time-block" key={index}>
                          <input type="time" value={block.start} aria-label={t("schedule.start")} onChange={(event) => patchBlock(index, { start: event.target.value })} />
                          <span>{t("schedule.to")}</span>
                          <input type="time" value={block.end} aria-label={t("schedule.end")} onChange={(event) => patchBlock(index, { end: event.target.value })} />
                          {block.end && block.start && block.end <= block.start && <small>{t("schedule.nextDay")}</small>}
                          <button type="button" className="icon-button danger" aria-label={t("schedule.removeBlock")} onClick={() => patch({ blocks: draft.blocks.filter((_, position) => position !== index) })}><Trash2 size={14} /></button>
                        </div>
                      )) : <small className="week-day-empty">{t("schedule.noBlocks")}</small>}
                    </div>
                    <div className="week-day-actions">
                      <button type="button" className="icon-button" aria-label={t("schedule.addBlock", { day: t(dayNameKeys[day]) })} title={t("schedule.addBlock", { day: t(dayNameKeys[day]) })} onClick={() => addBlock(day)}><Plus size={15} /></button>
                      <button type="button" className="icon-button" disabled={!entries.length} aria-label={t("schedule.copyDay", { day: t(dayNameKeys[day]) })} title={t("schedule.copyDay", { day: t(dayNameKeys[day]) })} onClick={() => copyDay(day)}><Copy size={14} /></button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
          {error && <div className="form-error"><CircleAlert size={16} />{error}</div>}
          <div className="schedule-actions">
            {editingId && <button className="secondary-button" onClick={resetDraft}>{t("schedule.cancelEdit")}</button>}
            <button className="primary-button" onClick={submit}>{editingId ? <Check size={17} /> : <Plus size={17} />}{t(editingId ? "schedule.save" : "schedule.add")}</button>
          </div>
          <small className="field-hint">{t("schedule.pendingSave")}</small>
        </Card>
        <Card>
          <div className="card-heading"><div><h2>{t("schedule.configured")}</h2><p>{t(state.schedules.length === 1 ? "schedule.policyCountOne" : "schedule.policyCount", { count: state.schedules.length })}</p></div></div>
          {state.schedules.length ? (
            <div className="schedule-list">
              {state.schedules.map((rule) => (
                <div key={rule.id} className={editingId === rule.id ? "editing" : ""}>
                  <div className="schedule-info">
                    <strong>{rule.name}</strong>
                    <span className={`tag ${rule.action === "block_launch" || rule.target === "all_network" ? "block" : "sensitive"}`}>{actionLabel(rule.action)}</span>
                    <span className="tag">{targetLabel(rule)}</span>
                    {rule.target === "applications" && <small>{rule.applications.join(" · ")}</small>}
                    {dayKeys.map((key, day) => {
                      const blocks = rule.blocks.filter((block) => block.day === day);
                      return blocks.length ? <small key={key}>{t(dayNameKeys[day])}：{blocks.map(blockLabel).join("，")}</small> : null;
                    })}
                  </div>
                  <Toggle checked={rule.enabled} onChange={(enabled) => update({ ...state, schedules: state.schedules.map((item) => item.id === rule.id ? { ...item, enabled } : item) })} label={t("schedule.enable", { name: rule.name })} />
                  <button className="icon-button" aria-label={t("schedule.edit", { name: rule.name })} onClick={() => edit(rule)}><Pencil size={16} /></button>
                  <button className="icon-button danger" aria-label={t("schedule.delete", { name: rule.name })} onClick={() => { update({ ...state, schedules: state.schedules.filter((item) => item.id !== rule.id) }); if (editingId === rule.id) resetDraft(); }}><Trash2 size={16} /></button>
                </div>
              ))}
            </div>
          ) : <EmptyState icon={<CalendarClock />} title={t("schedule.emptyTitle")} text={t("schedule.emptyDescription")} />}
        </Card>
      </div>
    </div>
  );
}

function evidenceTimestamp(value: string): number | undefined {
  const parsed = /^\d+$/.test(value) ? Number(value) : Date.parse(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : undefined;
}

function parseLocalDateTime(value: string): number | undefined {
  if (!value) return undefined;
  const parsed = new Date(value).getTime();
  return Number.isFinite(parsed) ? parsed : undefined;
}

/** A readable explanation of why an evidence image was saved. */
function evidenceReason(item: EvidenceItem, t: Translate): string {
  const cause = item.reason === "image_immediate" ? t("evidence.reasonImageImmediate") : t("evidence.reasonOther", { code: item.reason });
  return item.riskMillis === undefined ? cause : t("evidence.reasonWithScore", { reason: cause, score: Math.round(item.riskMillis / 10) });
}

function Evidence({ state, update, sessionToken, onDeleted, onApplicationAllowed }: { state: ConsoleState; update: (state: ConsoleState) => void; sessionToken: string; onDeleted: (ids: string[]) => void; onApplicationAllowed: ApplicationAllowedHandler }) {
  const { locale, t } = useI18n();
  const authorize = useSuperAuthorization();
  const [pendingAllow, setPendingAllow] = useState<EvidenceItem | null>(null);
  const [allowing, setAllowing] = useState(false);
  const [allowError, setAllowError] = useState("");
  const [allowNotice, setAllowNotice] = useState("");
  const allowedExecutables = useMemo(() => new Set(state.applications.filter((rule) => rule.enabled && rule.action === "allow").map((rule) => normalizeExecutable(rule.executable))), [state.applications]);
  const [selected, setSelected] = useState<EvidenceItem | null>(null);
  const [checked, setChecked] = useState<ReadonlySet<string>>(new Set());
  const [pendingDelete, setPendingDelete] = useState<string[] | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState("");
  const [password, setPassword] = useState("");
  const [imageUrl, setImageUrl] = useState<string>();
  const [error, setError] = useState("");
  const [rangeFrom, setRangeFrom] = useState("");
  const [rangeTo, setRangeTo] = useState("");
  const dateTimeFormat = useMemo(() => new Intl.DateTimeFormat(locale, { year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false }), [locale]);
  const fromMs = parseLocalDateTime(rangeFrom);
  const toMs = parseLocalDateTime(rangeTo);
  const invalidRange = fromMs !== undefined && toMs !== undefined && fromMs > toMs;
  const filterActive = fromMs !== undefined || toMs !== undefined;
  const items = useMemo(() => state.evidence
    .map((item) => ({ item, at: evidenceTimestamp(item.capturedAt) }))
    .filter(({ at }) => {
      if (fromMs === undefined && toMs === undefined) return true;
      if (at === undefined) return false;
      return (fromMs === undefined || at >= fromMs) && (toMs === undefined || at < toMs + 60_000);
    })
    .sort((left, right) => (right.at ?? 0) - (left.at ?? 0)), [state.evidence, fromMs, toMs]);
  const formatCapturedAt = (at: number | undefined) => at === undefined ? t("evidence.unknownTime") : dateTimeFormat.format(at);
  async function reveal() { if (!selected) return; setError(""); try { setImageUrl(await revealEvidence(sessionToken, selected.id, password)); } catch (reason) { setError(localizeError(reason, t)); } }
  const filterBar = <Card><div className="toolbar evidence-filter"><label className="field"><span>{t("evidence.filterFrom")}</span><input type="datetime-local" value={rangeFrom} max={rangeTo || undefined} onChange={(event) => setRangeFrom(event.target.value)} /></label><label className="field"><span>{t("evidence.filterTo")}</span><input type="datetime-local" value={rangeTo} min={rangeFrom || undefined} onChange={(event) => setRangeTo(event.target.value)} /></label><button className="secondary-button" disabled={!filterActive} onClick={() => { setRangeFrom(""); setRangeTo(""); }}><X size={16} />{t("evidence.filterClear")}</button><span className="evidence-filter-count">{t("evidence.filterCount", { shown: invalidRange ? 0 : items.length, total: state.evidence.length })}</span></div>{invalidRange && <div className="form-error"><CircleAlert size={16} />{t("evidence.filterInvalidRange")}</div>}</Card>;
  const visibleIds = invalidRange ? [] : items.map(({ item }) => item.id);
  const checkedVisible = visibleIds.filter((id) => checked.has(id));
  const allChecked = visibleIds.length > 0 && checkedVisible.length === visibleIds.length;
  function toggleChecked(id: string, value: boolean) { setChecked((current) => { const next = new Set(current); if (value) next.add(id); else next.delete(id); return next; }); }
  function toggleAll(value: boolean) { setChecked(value ? new Set(visibleIds) : new Set()); }
  function askDelete(ids: string[]) { if (ids.length) { setDeleteError(""); setPendingDelete(ids); } }
  function askAllow(item: EvidenceItem) { setAllowError(""); setAllowNotice(""); setPendingAllow(item); }
  async function confirmAllow() {
    if (!pendingAllow) return;
    const item = pendingAllow;
    setAllowing(true);
    setAllowError("");
    try {
      await authorize(async (superPassword) => onApplicationAllowed(item.applicationName, await allowEvidenceApplication(sessionToken, item.id, superPassword)));
      setPendingAllow(null);
      setAllowNotice(t("evidence.allowedNotice", { name: executableFileName(item.applicationName) }));
    } catch (reason) {
      if (!isAuthorizationCancelled(reason)) setAllowError(localizeError(reason, t));
    } finally {
      setAllowing(false);
    }
  }
  function allowButton(item: EvidenceItem) {
    const attributed = isAttributedExecutable(item.applicationName);
    const allowed = attributed && allowedExecutables.has(normalizeExecutable(item.applicationName));
    // Allowing a browser would exempt every page it shows; the Service refuses it too.
    const browser = attributed && isBrowserExecutable(state.applications, item.applicationName);
    return <button type="button" className="secondary-button compact" disabled={!attributed || allowed || browser || !state.serviceConnected} aria-label={t("evidence.allowApplicationOne", { name: item.applicationName })} title={t(!attributed ? "evidence.allowApplicationUnavailable" : browser ? "evidence.allowApplicationBrowser" : "evidence.allowApplicationHint")} onClick={() => askAllow(item)}><ShieldCheck size={14} />{t(allowed ? "evidence.alreadyAllowed" : "evidence.allowApplication")}</button>;
  }
  async function confirmDelete() {
    if (!pendingDelete) return;
    // Critical items go first, so a request that needs the super password is
    // refused before any item of a later batch is deleted.
    const critical = new Set(state.evidence.filter((item) => item.risk === "critical").map((item) => item.id));
    const ids = [...pendingDelete].sort((left, right) => Number(critical.has(right)) - Number(critical.has(left)));
    setDeleting(true);
    setDeleteError("");
    try {
      await authorize((superPassword) => deleteEvidence(sessionToken, ids, superPassword));
      onDeleted(pendingDelete);
      setChecked((current) => new Set([...current].filter((id) => !pendingDelete.includes(id))));
      if (selected && pendingDelete.includes(selected.id)) setSelected(null);
      setPendingDelete(null);
    } catch (reason) {
      if (!isAuthorizationCancelled(reason)) setDeleteError(localizeError(reason, t));
    } finally {
      setDeleting(false);
    }
  }
  const selectionBar = <div className="evidence-selection">
    <label className="evidence-check-all"><input type="checkbox" checked={allChecked} disabled={!visibleIds.length} onChange={(event) => toggleAll(event.target.checked)} />{t("evidence.selectAll")}</label>
    <span>{t("evidence.selectedCount", { count: checkedVisible.length })}</span>
    <button className="secondary-button danger-text" disabled={!checkedVisible.length} onClick={() => askDelete(checkedVisible)}><Trash2 size={16} />{t("evidence.deleteSelected")}</button>
  </div>;
  const grid = invalidRange || !items.length ? <Card><EmptyState icon={<CalendarClock />} title={t("evidence.filterEmptyTitle")} text={t("evidence.filterEmptyDescription")} /></Card> : <div className="evidence-grid">{items.map(({ item, at }) => <div className={`evidence-card${checked.has(item.id) ? " is-checked" : ""}`} key={item.id}>
    <button type="button" className="evidence-open" onClick={() => { setSelected(item); setImageUrl(undefined); setPassword(""); setError(""); }}>
      <div className="evidence-image">{item.thumbnailUrl ? <img src={item.thumbnailUrl} alt={t("evidence.blurredAlt")} /> : <Image />}<span><Eye size={16} />{t("evidence.viewAfterVerification")}</span></div>
      <div className="evidence-body">
        <p className="evidence-reason"><CircleAlert size={13} /><span><b>{t("evidence.reasonLabel")}</b>{evidenceReason(item, t)}</span></p>
        <StatusPill tone={item.risk === "critical" ? "danger" : "warn"}>{t(item.risk === "critical" ? "evidence.critical" : "evidence.highRisk")}</StatusPill>
        <strong>{item.applicationName}</strong>
        <time className="evidence-time" dateTime={at === undefined ? undefined : new Date(at).toISOString()}><CalendarClock size={13} />{formatCapturedAt(at)}</time>
        <small>{item.monitorName}</small>
      </div>
    </button>
    <div className="evidence-actions">
      <label><input type="checkbox" checked={checked.has(item.id)} onChange={(event) => toggleChecked(item.id, event.target.checked)} />{t("evidence.select")}</label>
      <div className="evidence-action-buttons">
        {allowButton(item)}
        <button type="button" className="icon-button danger" aria-label={t("evidence.deleteOne", { name: item.applicationName })} title={t("evidence.delete")} onClick={() => askDelete([item.id])}><Trash2 size={16} /></button>
      </div>
    </div>
  </div>)}</div>;
  const pendingCritical = pendingDelete ? state.evidence.filter((item) => item.risk === "critical" && pendingDelete.includes(item.id)).length : 0;
  const deleteModal = pendingDelete && <Modal title={t("evidence.deleteTitle")} onClose={() => !deleting && setPendingDelete(null)}>
    <p className="modal-description">{t("evidence.deleteConfirm", { count: pendingDelete.length })}</p>
    {pendingCritical > 0 && state.superPasswordSet && <p className="modal-description">{t("evidence.deleteCriticalNeedsSuper", { count: pendingCritical })}</p>}
    {deleteError && <div className="form-error"><CircleAlert size={16} />{deleteError}</div>}
    <div className="modal-actions">
      <button className="secondary-button" disabled={deleting} onClick={() => setPendingDelete(null)}>{t("evidence.deleteCancel")}</button>
      <button className="primary-button danger-fill" disabled={deleting} onClick={() => void confirmDelete()}><Trash2 size={16} />{t(deleting ? "evidence.deleting" : "evidence.deleteConfirmButton", { count: pendingDelete.length })}</button>
    </div>
  </Modal>;
  const allowModal = pendingAllow && <Modal title={t("evidence.allowTitle")} onClose={() => !allowing && setPendingAllow(null)}>
    <p className="modal-description">{t("evidence.allowConfirm", { name: executableFileName(pendingAllow.applicationName) })}</p>
    <p className="modal-description allow-path">{pendingAllow.applicationName}</p>
    {allowError && <div className="form-error"><CircleAlert size={16} />{allowError}</div>}
    <div className="modal-actions">
      <button className="secondary-button" disabled={allowing} onClick={() => setPendingAllow(null)}>{t("evidence.deleteCancel")}</button>
      <button className="primary-button" disabled={allowing} onClick={() => void confirmAllow()}><ShieldCheck size={16} />{t(allowing ? "evidence.allowing" : "evidence.allowConfirmButton")}</button>
    </div>
  </Modal>;
  return <div className="page-stack"><div className="privacy-banner"><Lock size={18} /><span><strong>{t("evidence.bannerTitle")}</strong> {t("evidence.bannerDescription")}</span></div><Card><div className="setting-row"><div className="setting-icon violet"><AppWindow /></div><div><strong>{t("evidence.skipAllowedTitle")}</strong><p>{t("evidence.skipAllowedDescription")}</p></div><Toggle checked={state.recognition.evidenceSkipAllowedApplications} onChange={(value) => update({ ...state, recognition: { ...state.recognition, evidenceSkipAllowedApplications: value } })} label={t("evidence.skipAllowedTitle")} /></div></Card>{allowNotice && <div className="form-success"><Check size={16} />{allowNotice}</div>}{state.evidence.length ? <>{filterBar}{selectionBar}{grid}</> : <Card><EmptyState icon={<Lock />} title={t("evidence.emptyTitle")} text={t(state.recognition.evidenceEnabled ? "evidence.emptyEnabled" : "evidence.emptyDisabled")} /></Card>}{selected && <Modal title={t("evidence.modalTitle")} onClose={() => setSelected(null)} wide>{imageUrl ? <img className="revealed-image" src={imageUrl} alt={t("evidence.originalAlt")} /> : <div className="reveal-panel"><div className="locked-preview"><LockKeyhole /></div><strong>{evidenceReason(selected, t)}</strong><p>{formatCapturedAt(evidenceTimestamp(selected.capturedAt))} · {selected.applicationName} · {selected.monitorName}</p><label className="field"><span>{t("evidence.passwordAgain")}</span><input type="password" value={password} onChange={(event) => setPassword(event.target.value)} onKeyDown={(event) => event.key === "Enter" && reveal()} autoFocus /></label>{error && <div className="form-error"><CircleAlert size={16} />{error}</div>}<button className="primary-button modal-submit" onClick={reveal}><Eye size={17} />{t("evidence.decrypt")}</button></div>}</Modal>}{deleteModal}{allowModal}</div>;
}

type AuditLevel = "success" | "warning" | "denied";

const deniedAuditOutcomes = new Set(["denied", "rate_limited", "failure", "failed", "access_denied"]);
const warningAuditOutcomes = new Set(["busy", "authorized", "identity_changed", "source_uncertain", "update_in_progress"]);
/** Closure outcomes that actually closed the window and count toward a restriction. */
const completedClosureOutcomes = new Set(["closed_gracefully", "terminated"]);

function auditLevel(item: AuditItem): AuditLevel {
  if (deniedAuditOutcomes.has(item.outcome)) return "denied";
  if (warningAuditOutcomes.has(item.outcome)) return "warning";
  return "success";
}

function isClosure(item: AuditItem): boolean {
  return item.kind === "application_disposition";
}

/** The executable an audit row names. Bans from v0.3.9 and earlier kept it as the outcome. */
function auditExecutable(item: AuditItem): string | undefined {
  if (item.detail?.executable) return item.detail.executable;
  if (item.kind === "repeat_offender_banned" && item.outcome !== "success") return item.outcome;
  return undefined;
}

/** Without `detailed`, closures with the same reason share one label, such as in ban summaries. */
function closureReasonText(detail: AuditDetail | null, t: Translate, detailed = true): string {
  const reason = detail?.reason;
  if (!reason) return t("audit.reason.unrecorded");
  if (reason === "image") {
    if (!detailed || detail.riskMillis === undefined || detail.thresholdMillis === undefined) return t("audit.reason.imageUnknown");
    return t("audit.reason.image", { risk: Math.round(detail.riskMillis / 10), threshold: Math.round(detail.thresholdMillis / 10) });
  }
  if (reason === "title_keyword" && detailed && detail.keyword) return t("audit.reason.titleKeywordMatched", { keyword: detail.keyword });
  const key = `audit.reason.${reason}`;
  return isMessageKey(key) ? t(key) : reason;
}

function auditText(item: AuditItem, t: Translate): { title: string; detail: string } {
  const executable = auditExecutable(item);
  const app = executable ? executableFileName(executable) : t("audit.unknownApp");
  if (isClosure(item)) {
    const outcomeKey = `audit.outcome.${item.outcome}`;
    return {
      title: t("audit.closure", { app }),
      detail: t("audit.closureDetail", { reason: closureReasonText(item.detail, t), outcome: isMessageKey(outcomeKey) ? t(outcomeKey) : item.outcome }),
    };
  }
  if (item.kind === "repeat_offender_banned") return { title: t("audit.banned", { app }), detail: executable ?? "" };
  if (item.kind === "schedule_launch_blocked") return { title: t(item.outcome === "terminated" ? "audit.launchBlocked" : "audit.launchBlockFailed", { app }), detail: executable ?? "" };
  const kindKey = `auditKind.${item.kind}`;
  return { title: isMessageKey(kindKey) ? t(kindKey) : item.kind, detail: item.kind };
}

/** Counts the reasons of the completed closures that established `ban`. */
function banStrikeSummary(audit: AuditItem[], ban: ActiveBan, windowMinutes: number, t: Translate): string {
  const executable = normalizeExecutable(ban.executable);
  const bannedAt = audit.find((item) => item.kind === "repeat_offender_banned" && item.occurredAtMs <= ban.bannedUntilMs && normalizeExecutable(auditExecutable(item) ?? "") === executable)?.occurredAtMs;
  if (bannedAt === undefined) return t("applications.banStrikesUnknown");
  const from = bannedAt - windowMinutes * 60_000;
  const counts = new Map<string, number>();
  for (const item of audit) {
    if (!isClosure(item) || item.occurredAtMs < from || item.occurredAtMs > bannedAt) continue;
    if (!completedClosureOutcomes.has(item.outcome) || item.detail?.reason === "repeat_offender_ban") continue;
    if (normalizeExecutable(item.detail?.executable ?? "") !== executable) continue;
    const label = closureReasonText(item.detail, t, false);
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }
  if (!counts.size) return t("applications.banStrikesUnknown");
  return t("applications.banStrikes", { reasons: [...counts].map(([label, count]) => `${label} ×${count}`).join(" · ") });
}

function useAuditDateTime(): Intl.DateTimeFormat {
  const { locale } = useI18n();
  return useMemo(() => new Intl.DateTimeFormat(locale, { year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false }), [locale]);
}

function Audit({ state, update }: { state: ConsoleState; update: (state: ConsoleState) => void }) {
  const { t } = useI18n();
  const dateTime = useAuditDateTime();
  const [closuresOnly, setClosuresOnly] = useState(false);
  const [categoryNotice, setCategoryNotice] = useState<{ error: boolean; text: string }>();
  // Puts a closed application in a category so a Blocking Schedule can block it with the rest.
  function assignCategory(executable: string, category: string) {
    const next = assignApplicationsToCategory(state.applications, [{ name: executableFileName(executable), executable }], category);
    if (next.length > maxApplicationRules) { setCategoryNotice({ error: true, text: t("applications.batchTooMany", { max: maxApplicationRules }) }); return; }
    update({ ...state, applications: next });
    setCategoryNotice({ error: false, text: t("categories.pendingSave") });
  }
  function categoryControl(item: AuditItem) {
    const executable = isClosure(item) ? auditExecutable(item) : undefined;
    if (!executable || !executableFileName(executable).toLowerCase().endsWith(".exe")) return null;
    const app = executableFileName(executable);
    const knownBrowser = knownBrowserExecutables.includes(app.toLowerCase());
    return <CategorySelect state={state} value={applicationCategoryOf(state.applications, executable)} disabled={knownBrowser} label={t("audit.categoryOf", { app })} onChange={(category) => assignCategory(executable, category)} />;
  }
  const items = closuresOnly ? state.audit.filter(isClosure) : state.audit;
  const filter = <div className="toolbar audit-filter">
    <button className={closuresOnly ? "secondary-button" : "primary-button"} onClick={() => setClosuresOnly(false)}>{t("audit.filterAll")}</button>
    <button className={closuresOnly ? "primary-button" : "secondary-button"} onClick={() => setClosuresOnly(true)}><Ban size={16} />{t("audit.filterClosures")}</button>
  </div>;
  const notice = categoryNotice && (categoryNotice.error ? <div className="form-error"><CircleAlert size={16} />{categoryNotice.text}</div> : <small className="field-hint">{categoryNotice.text}</small>);
  if (!state.audit.length) return <Card><EmptyState icon={<FileClock />} title={t("audit.emptyTitle")} text={t("audit.emptyDescription")} /></Card>;
  return <Card>{filter}{notice}{items.length ? <div className="audit-list">{items.map((item) => {
    const level = auditLevel(item);
    const text = auditText(item, t);
    return <div key={item.id}><span className={`audit-icon ${level}`}>{isClosure(item) ? <Ban /> : level === "success" ? <Check /> : <CircleAlert />}</span><div><strong>{text.title}</strong><small>{text.detail} · {dateTime.format(item.occurredAtMs)}</small></div>{categoryControl(item)}<StatusPill tone={level === "success" ? "good" : level === "warning" ? "warn" : "danger"}>{t(level === "success" ? "audit.success" : level === "warning" ? "audit.warning" : "audit.denied")}</StatusPill></div>;
  })}</div> : <p className="learned-empty">{t("audit.closuresEmpty")}</p>}</Card>;
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

function BackupCard({
  sessionToken,
  state,
  update,
}: {
  sessionToken: string;
  state: ConsoleState;
  update: (state: ConsoleState) => void;
}) {
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const fileInput = useRef<HTMLInputElement | null>(null);

  function applyImported(imported: Partial<ConsoleState>) {
    update(mergeImportedPolicy(state, imported));
    setNotice(t("settings.importApplied"));
  }

  async function doExport() {
    setError("");
    setNotice("");
    setBusy(true);
    try {
      if (isTauriConsole()) {
        const path = await chooseExportPath();
        if (!path) return;
        await exportSettings(sessionToken, path);
      } else {
        await exportSettings(sessionToken);
      }
      setNotice(t("settings.exportSuccess"));
    } catch (reason) {
      setError(localizeError(reason, t));
    } finally {
      setBusy(false);
    }
  }

  async function doImport() {
    setError("");
    setNotice("");
    if (!isTauriConsole()) {
      fileInput.current?.click();
      return;
    }
    setBusy(true);
    try {
      const path = await chooseImportPath();
      if (!path) return;
      applyImported(await importSettings(sessionToken, path));
    } catch (reason) {
      setError(localizeError(reason, t));
    } finally {
      setBusy(false);
    }
  }

  async function onBrowserFile(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) return;
    setError("");
    setNotice("");
    setBusy(true);
    try {
      applyImported(await parseImportedBackupFile(file));
    } catch (reason) {
      setError(localizeError(reason, t));
    } finally {
      setBusy(false);
    }
  }

  return <Card className="full-span">
    <div className="card-heading"><div><h2>{t("settings.backupTitle")}</h2><p>{t("settings.backupDescription")}</p></div></div>
    <div className="backup-actions">
      <button className="primary-button" disabled={busy} onClick={doExport}><Download size={17} />{t("settings.exportSettings")}</button>
      <button className="secondary-button" disabled={busy} onClick={doImport}><Upload size={17} />{t("settings.importSettings")}</button>
      <input ref={fileInput} type="file" accept="application/json,.json" hidden onChange={onBrowserFile} />
    </div>
    {error && <div className="form-error backup-feedback"><CircleAlert size={16} />{error}</div>}
    {notice && !error && <div className="form-success backup-feedback"><Check size={16} />{notice}</div>}
  </Card>;
}

function SuperPasswordCard({ state, sessionToken, onRefresh }: { state: ConsoleState; sessionToken: string; onRefresh: () => Promise<void> }) {
  const { t } = useI18n();
  const [current, setCurrent] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    setSaved(false);
    if (Array.from(password).length < 10) return setError(t("super.tooShort"));
    if (password !== confirm) return setError(t("auth.passwordMismatch"));
    if (!state.superPasswordSet && current === password) return setError(t("super.mustDiffer"));
    setBusy(true);
    try {
      if (state.superPasswordSet) await changeSuperPassword(sessionToken, current, password);
      else await setSuperPassword(sessionToken, current, password);
      setCurrent(""); setPassword(""); setConfirm("");
      await onRefresh();
      setSaved(true);
    } catch (reason) { setError(localizeError(reason, t)); }
    finally { setBusy(false); }
  }
  return <Card className="full-span">
    <div className="card-heading"><div><h2>{t("super.title")}</h2><p>{t("super.description")}</p></div><StatusPill tone={state.superPasswordSet ? "good" : "muted"}>{t(state.superPasswordSet ? "super.configured" : "super.notConfigured")}</StatusPill></div>
    {!state.serviceConnected && <p className="field-hint">{t("authorization.serviceRequired")}</p>}
    <form className="password-form" onSubmit={submit}>
      <label className="field"><span>{t(state.superPasswordSet ? "super.current" : "auth.password")}</span><input type="password" autoComplete="current-password" value={current} onChange={(event) => { setCurrent(event.target.value); setSaved(false); }} /></label>
      <label className="field"><span>{t("super.new")}</span><input type="password" autoComplete="new-password" value={password} onChange={(event) => { setPassword(event.target.value); setSaved(false); }} /></label>
      <label className="field"><span>{t("super.confirm")}</span><input type="password" autoComplete="new-password" value={confirm} onChange={(event) => { setConfirm(event.target.value); setSaved(false); }} /></label>
      <div className="password-form-footer">
        {error && <div className="form-error"><CircleAlert size={16} />{error}</div>}
        {saved && !error && <div className="form-success"><Check size={16} />{t("super.saved")}</div>}
        <button className="primary-button" type="submit" disabled={busy || !state.serviceConnected || !current || !password || !confirm}><KeyRound size={17} />{t(busy ? "common.saving" : state.superPasswordSet ? "super.change" : "super.set")}</button>
      </div>
    </form>
  </Card>;
}

function countdown(until: number, now: number): string {
  const seconds = Math.max(0, Math.ceil((until - now) / 1000));
  return `${Math.floor(seconds / 3600)}:${String(Math.floor(seconds / 60) % 60).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

function UninstallCard({ state, sessionToken, onRefresh }: { state: ConsoleState; sessionToken: string; onRefresh: () => Promise<void> }) {
  const { locale, t } = useI18n();
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [now, setNow] = useState(Date.now);
  useEffect(() => { const interval = window.setInterval(() => setNow(Date.now()), 1000); return () => window.clearInterval(interval); }, []);
  const pending = state.uninstall;
  const waiting = pending !== null && now < pending.effectiveAtMs;
  const ready = pending !== null && now >= pending.effectiveAtMs && now < pending.expiresAtMs;
  async function perform(cancel: boolean) {
    setBusy(true); setError("");
    try {
      if (cancel) await cancelUninstall(sessionToken);
      else await requestUninstall(sessionToken, password);
      setPassword("");
      await onRefresh();
    } catch (reason) { setError(localizeError(reason, t)); }
    finally { setBusy(false); }
  }
  return <Card className="full-span">
    <div className="card-heading"><div><h2>{t("uninstall.title")}</h2><p>{t("uninstall.description")}</p></div><div className="setting-icon amber"><CalendarClock /></div></div>
    {!state.serviceConnected && <p className="field-hint">{t("authorization.serviceRequired")}</p>}
    {pending && <div className="uninstall-status" role="status"><StatusPill tone={ready ? "good" : waiting ? "warn" : "muted"}>{t(waiting ? "uninstall.waiting" : ready ? "uninstall.ready" : "uninstall.expired")}</StatusPill><strong className="countdown">{waiting ? t("uninstall.waitRemaining", { time: countdown(pending.effectiveAtMs, now) }) : ready ? t("uninstall.windowRemaining", { time: countdown(pending.expiresAtMs, now) }) : t("uninstall.requestAgain")}</strong><small>{t("uninstall.window", { start: new Date(pending.effectiveAtMs).toLocaleString(locale), end: new Date(pending.expiresAtMs).toLocaleString(locale) })}</small>{ready && <p>{t("uninstall.instructions")}</p>}</div>}
    <form className="uninstall-form" onSubmit={(event) => { event.preventDefault(); void perform(false); }}>
      {(!pending || (!waiting && !ready)) && <><label className="field"><span>{t("auth.password")}</span><input type="password" autoComplete="current-password" value={password} onChange={(event) => setPassword(event.target.value)} /></label><button className="primary-button" type="submit" disabled={busy || !state.serviceConnected || !password}>{t(busy ? "auth.verifying" : "uninstall.request")}</button></>}
      {pending && <button className="secondary-button" type="button" disabled={busy || !state.serviceConnected} onClick={() => void perform(true)}>{t("uninstall.cancel")}</button>}
    </form>
    {error && <div className="form-error backup-feedback"><CircleAlert size={16} />{error}</div>}
  </Card>;
}

const updatePhaseKeys: Record<ConsoleState["update"]["phase"], MessageKey> = {
  idle: "updates.idle", checking: "updates.checking", downloading: "updates.downloading", installing: "updates.installing", up_to_date: "updates.upToDate", failed: "updates.failed",
};

const updateErrorKeys: Record<string, MessageKey> = {
  storage: "updates.errorStorage",
  invalid_manifest: "updates.errorManifest",
  bad_signature: "updates.errorSignature",
  not_newer: "updates.errorNotNewer",
  hash_mismatch: "updates.errorIntegrity",
  missing_file: "updates.errorIntegrity",
  too_large: "updates.errorTooLarge",
  invalid_archive: "updates.errorArchive",
  network: "updates.errorNetwork",
  unsafe_path: "updates.errorPath",
};

function SoftwareUpdatesCard({ state, update, sessionToken, onRefresh }: { state: ConsoleState; update: (state: ConsoleState) => void; sessionToken: string; onRefresh: () => Promise<void> }) {
  const { locale, t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const active = ["checking", "downloading", "installing"].includes(state.update.phase);
  async function check() {
    setBusy(true); setError("");
    try { await checkForUpdates(sessionToken); await onRefresh(); }
    catch (reason) { setError(localizeError(reason, t)); }
    finally { setBusy(false); }
  }
  return <Card className="full-span">
    <div className="card-heading"><div><h2>{t("updates.title")}</h2><p>{t("updates.version", { version: state.productVersion || t("updates.unknownVersion") })}</p></div><div className="setting-icon blue"><Download /></div></div>
    <div className="setting-row update-preference"><div><strong>{t("updates.autoTitle")}</strong><p>{t("updates.autoDescription")}</p></div><Toggle checked={state.updates.autoInstall} onChange={(autoInstall) => update({ ...state, updates: { ...state.updates, autoInstall } })} label={t("updates.autoTitle")} /></div>
    <div className="update-status"><div role="status"><strong>{t(updatePhaseKeys[state.update.phase])}</strong>{state.update.availableVersion && <small>{t("updates.available", { version: state.update.availableVersion })}</small>}{state.update.lastCheckedAtMs && <small>{t("updates.lastChecked", { time: new Date(state.update.lastCheckedAtMs).toLocaleString(locale) })}</small>}</div><button className="secondary-button" disabled={busy || active || !state.serviceConnected} onClick={() => void check()}><Download size={16} />{t(busy || active ? "updates.checking" : "updates.checkNow")}</button></div>
    {state.update.lastError && <div className="form-error backup-feedback"><CircleAlert size={16} />{t(updateErrorKeys[state.update.lastError] ?? "updates.failed")}</div>}
    {!state.serviceConnected && <p className="field-hint">{t("authorization.serviceRequired")}</p>}
    <small className="field-hint">{t("updates.saveHint")}</small>
    {error && <div className="form-error backup-feedback"><CircleAlert size={16} />{error}</div>}
  </Card>;
}

function SettingsPage({ state, update, sessionToken, onRefresh }: { state: ConsoleState; update: (state: ConsoleState) => void; sessionToken: string; onRefresh: () => Promise<void> }) {
  const { t } = useI18n();
  return <div className="settings-grid"><Card className="full-span"><div className="setting-row"><div className="setting-icon green"><KarmaShieldIcon /></div><div><strong>{t("settings.protectionTitle")}</strong><p>{t("settings.protectionDescription")}</p></div><Toggle checked={state.protectionEnabled} onChange={(value) => update({ ...state, protectionEnabled: value })} label={t("settings.protectionToggle")} /></div></Card><Card className="full-span"><div className="setting-row"><div className="setting-icon violet"><Power /></div><div><strong>{t("settings.autostartTitle")}</strong><p>{t("settings.autostartDescription")}</p></div><Toggle checked={state.launchAtStartup} onChange={(value) => update({ ...state, launchAtStartup: value })} label={t("settings.autostartToggle")} /></div></Card><Card className="full-span language-card"><div className="setting-row"><div className="setting-icon blue"><Globe2 /></div><div><strong>{t("settings.languageTitle")}</strong><p>{t("language.description")}</p></div><LanguageSelect /></div></Card><PasswordChangeCard sessionToken={sessionToken} /><SuperPasswordCard state={state} sessionToken={sessionToken} onRefresh={onRefresh} /><UninstallCard state={state} sessionToken={sessionToken} onRefresh={onRefresh} /><SoftwareUpdatesCard state={state} update={update} sessionToken={sessionToken} onRefresh={onRefresh} /><BackupCard sessionToken={sessionToken} state={state} update={update} /><Card className="full-span"><div className="card-heading"><div><h2>{t("settings.capabilities")}</h2><p>{t("settings.progress")}</p></div></div><div className="capability-list"><div><Check />{t("settings.capability1")}</div><div><Check />{t("settings.capability2")}</div><div><Check />{t("settings.capability3")}</div><div><Check />{t("settings.capability4")}</div><div><Check />{t("settings.capability5")}</div><div className="pending"><CircleAlert />{t("settings.capabilityPending")}</div></div></Card></div>;
}

function Modal({ title, children, onClose, wide = false }: { title: string; children: ReactNode; onClose: () => void; wide?: boolean }) {
  const { t } = useI18n();
  return <div className="modal-backdrop" onMouseDown={onClose}><section role="dialog" aria-modal="true" aria-label={title} className={`modal ${wide ? "wide" : ""}`} onMouseDown={(event) => event.stopPropagation()}><div className="modal-head"><h2>{title}</h2><button className="icon-button" aria-label={t("common.close")} onClick={onClose}><X /></button></div>{children}</section></div>;
}

type SuperProtectedAction = (superPassword?: string) => Promise<void>;
const SuperAuthorizationContext = createContext<((action: SuperProtectedAction) => Promise<void>) | null>(null);

function useSuperAuthorization() {
  const authorize = useContext(SuperAuthorizationContext);
  if (!authorize) throw new Error("Super authorization is unavailable");
  return authorize;
}

function SuperAuthorizationProvider({ children }: { children: ReactNode }) {
  const { t } = useI18n();
  const [pending, setPending] = useState<{ action: SuperProtectedAction; resolve: () => void; reject: (reason: unknown) => void } | null>(null);
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const queue = useRef<Promise<void>>(Promise.resolve());
  function authorize(action: SuperProtectedAction): Promise<void> {
    const result = queue.current.then(() => attempt(action));
    queue.current = result.catch(() => undefined);
    return result;
  }
  async function attempt(action: SuperProtectedAction) {
    try { await action(); }
    catch (reason) {
      // Only the Service may decide that this action needs the super password.
      if (!isSuperPasswordRequired(reason)) throw reason;
      setPassword(""); setError("");
      await new Promise<void>((resolve, reject) => setPending({ action, resolve, reject }));
    }
  }
  function cancel() {
    if (busy || !pending) return;
    pending.reject(new Error("super_password_cancelled"));
    setPending(null); setPassword(""); setError("");
  }
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!pending || !password || busy) return;
    setBusy(true); setError("");
    try {
      await pending.action(password);
      pending.resolve(); setPending(null); setPassword("");
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : String(reason);
      if (isSuperPasswordRequired(reason) || message === "密码不正确" || message === "验证失败次数过多，请稍后重试") setError(localizeError(reason, t));
      else { pending.reject(reason); setPending(null); setPassword(""); }
    } finally { setBusy(false); }
  }
  return <SuperAuthorizationContext.Provider value={authorize}>{children}{pending && <Modal title={t("super.authorizeTitle")} onClose={cancel}><p className="modal-description">{t("super.authorizeDescription")}</p><form onSubmit={submit}><label className="field"><span>{t("super.password")}</span><input type="password" autoFocus autoComplete="current-password" value={password} onChange={(event) => setPassword(event.target.value)} /></label>{error && <div className="form-error backup-feedback"><CircleAlert size={16} />{error}</div>}<button className="primary-button modal-submit" type="submit" disabled={busy || !password}>{t(busy ? "auth.verifying" : "super.authorize")}</button><button className="secondary-button modal-submit" type="button" disabled={busy} onClick={cancel}>{t("common.cancel")}</button></form></Modal>}</SuperAuthorizationContext.Provider>;
}

/** Applies the rules the Service changed between `before` and `after` to an unsaved draft. */
function mergeSavedRules(draft: ApplicationRule[], before: ApplicationRule[], after: ApplicationRule[]): ApplicationRule[] {
  const previous = new Map(before.map((rule) => [rule.id, JSON.stringify(rule)]));
  const changed = after.filter((rule) => previous.get(rule.id) !== JSON.stringify(rule));
  const draftIds = new Set(draft.map((rule) => rule.id));
  return [...draft.map((rule) => changed.find((item) => item.id === rule.id) ?? rule), ...changed.filter((rule) => !draftIds.has(rule.id))];
}

/** Restores the policy fields the Service last accepted, keeping runtime status current. */
function withSavedPolicy(current: ConsoleState, saved: ConsoleState): ConsoleState {
  return { ...current, protectionEnabled: saved.protectionEnabled, launchAtStartup: saved.launchAtStartup, updates: saved.updates, recognition: saved.recognition, repeatOffender: saved.repeatOffender, keywords: saved.keywords, titleKeywordAdditions: saved.titleKeywordAdditions, websites: saved.websites, applications: saved.applications, schedules: saved.schedules };
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
  const [appVersion, setAppVersion] = useState("");
  const editRevision = useRef(0);
  // The policy the Service last accepted; a refused super authorization reverts the draft to it.
  const savedState = useRef<ConsoleState>(defaultConsoleState);
  const authorize = useSuperAuthorization();

  async function refreshAuthStatus() { setLoading(true); setStartupError(""); try { setAuthMode(await authStatus()); } catch (reason) { setStartupError(reason instanceof Error ? reason.message : String(reason)); } finally { setLoading(false); } }
  useEffect(() => { document.documentElement.lang = locale; document.title = t("document.title"); }, [locale, t]);
  useEffect(() => { void refreshAuthStatus(); }, []);
  useEffect(() => { void getAppVersion().then(setAppVersion).catch(() => undefined); }, []);
  useEffect(() => {
    if (authMode !== "unlocked" || !sessionToken) return;
    let timeout = window.setTimeout(() => void signOut(), 15 * 60 * 1000);
    const reset = () => { window.clearTimeout(timeout); timeout = window.setTimeout(() => void signOut(), 15 * 60 * 1000); };
    const events: Array<keyof WindowEventMap> = ["pointerdown", "keydown", "wheel"];
    events.forEach((eventName) => window.addEventListener(eventName, reset, { passive: true }));
    return () => { window.clearTimeout(timeout); events.forEach((eventName) => window.removeEventListener(eventName, reset)); };
  }, [authMode, sessionToken]);
  useEffect(() => {
    if (authMode !== "unlocked" || !sessionToken) return;
    let current = true;
    const refresh = () => {
      void loadConsole(sessionToken, true).then((next) => {
        if (!current) return;
        // Runtime refreshes never replace an administrator's policy draft or
        // change the revision against which that draft will be saved.
        setState((previous) => ({ ...previous, serviceConnected: next.serviceConnected, agentConnected: next.agentConnected, monitors: next.monitors, network: next.network, evidence: next.evidence, audit: next.audit, rememberedTitleTotal: next.rememberedTitleTotal, misjudgedTitleTotal: next.misjudgedTitleTotal, activeBans: next.activeBans, productVersion: next.productVersion, superPasswordSet: next.superPasswordSet, uninstall: next.uninstall, update: next.update }));
      }).catch((reason) => {
        if (!current) return;
        // A restarted service invalidates every in-memory session; return to the
        // unlock screen so a fresh token is issued instead of reporting offline.
        if (isSessionExpiredError(reason)) { void signOut(); return; }
        setState((current) => ({ ...current, serviceConnected: false, agentConnected: false, monitors: [] }));
      });
    };
    const interval = window.setInterval(refresh, 5000);
    return () => { current = false; window.clearInterval(interval); };
  }, [authMode, sessionToken]);
  async function authenticated(token: string) { setSessionToken(token); setLoading(true); try { const next = await loadConsole(token); savedState.current = next; setState(next); setAuthMode("unlocked"); void configureLaunchAtStartup(next.launchAtStartup).catch((reason) => console.error("Failed to synchronize autostart", reason)); } finally { setLoading(false); } }
  function update(next: ConsoleState) { editRevision.current += 1; setState(next); setDirty(true); setSaveMessage(""); setSaveError(""); }
  // Evidence is runtime state, so deleting it never marks the policy draft dirty.
  function evidenceDeleted(ids: string[]) { setState((previous) => ({ ...previous, evidence: previous.evidence.filter((item) => !ids.includes(item.id)) })); }
  // The Service saved these rules directly; keep the administrator's other unsaved edits.
  function applicationAllowed(executable: string, applications: ApplicationRule[]) {
    const before = savedState.current.applications;
    const allowed = normalizeExecutable(executable);
    savedState.current = { ...savedState.current, applications };
    setState((current) => ({ ...current, applications: mergeSavedRules(current.applications, before, applications), activeBans: current.activeBans.filter((ban) => normalizeExecutable(ban.executable) !== allowed) }));
  }
  // Network status is runtime state reported by the Service, not an unsaved policy edit.
  function updateNetwork(network: NetworkStatus) { setState((current) => ({ ...current, network })); }
  async function refreshAuthorization() {
    const status = await loadAuthorizationStatus(sessionToken);
    setState((current) => ({ ...current, ...status }));
  }
  async function save() {
    const revision = editRevision.current;
    setSaving(true);
    setSaveError("");
    try {
      const next = { ...state, recognition: { ...state.recognition, immediateThreshold: state.recognition.sensitivity } };
      await authorize((superPassword) => saveConsole(sessionToken, next, superPassword));
      savedState.current = next;
      await configureLaunchAtStartup(next.launchAtStartup);
      if (editRevision.current === revision) setDirty(false);
      setSaveMessage(t("common.settingsSaved"));
      window.setTimeout(() => setSaveMessage(""), 2200);
    } catch (reason) {
      if (isSessionExpiredError(reason)) { await signOut(); return; }
      if (isAuthorizationCancelled(reason)) {
        // The Service refused this draft without the super password, so leaving it
        // on screen would show protection as changed when it was not.
        editRevision.current += 1;
        setState((current) => withSavedPolicy(current, savedState.current));
        setDirty(false);
        setSaveError(t("super.reverted"));
      } else setSaveError(localizeError(reason, t));
    } finally {
      setSaving(false);
    }
  }
  async function signOut() {
    // The service may be down or the session already invalid; always reset locally.
    await lock(sessionToken).catch(() => undefined);
    setSessionToken("");
    editRevision.current += 1;
    setDirty(false);
    savedState.current = defaultConsoleState;
    setState(defaultConsoleState);
    setAuthMode("locked");
    setPage("overview");
    setSaveMessage("");
    setSaveError("");
  }

  const content = useMemo(() => {
    if (page === "overview") return <Overview state={state} update={update} sessionToken={sessionToken} onNetworkChange={updateNetwork} />;
    if (page === "monitors") return <Monitors state={state} />;
    if (page === "recognition") return <Recognition state={state} update={update} sessionToken={sessionToken} />;
    if (page === "keywords") return <Keywords state={state} update={update} />;
    if (page === "websites") return <Websites state={state} update={update} />;
    if (page === "applications") return <Applications state={state} update={update} sessionToken={sessionToken} onBannedAllowed={applicationAllowed} />;
    if (page === "schedule") return <Schedule state={state} update={update} sessionToken={sessionToken} onNetworkChange={updateNetwork} />;
    if (page === "evidence") return <Evidence state={state} update={update} sessionToken={sessionToken} onDeleted={evidenceDeleted} onApplicationAllowed={applicationAllowed} />;
    if (page === "audit") return <Audit state={state} update={update} />;
    return <SettingsPage state={state} update={update} sessionToken={sessionToken} onRefresh={refreshAuthorization} />;
  }, [page, state, sessionToken, locale]);

  if (loading) return <div className="loading-screen"><div className="brand-mark"><KarmaShieldIcon /></div><span>{t("common.loading")}</span></div>;
  if (startupError) return <ServiceConnectionError detail={localizeError(startupError, t)} onRetry={() => void refreshAuthStatus()} />;
  if (authMode !== "unlocked") return <PasswordGate mode={authMode} onAuthenticated={authenticated} />;
  const meta = pageMeta[page];
  const saveStatus = saveError ? <span className="sidebar-status error"><CircleAlert size={14} /><span>{saveError}</span></span>
    : dirty && !saving ? <span className="sidebar-status unsaved"><CircleAlert size={14} /><span>{t("common.unsavedChanges")}</span></span>
    : saveMessage ? <span className="sidebar-status saved"><Check size={14} /><span>{saveMessage}</span></span> : null;
  return <div className="app-shell"><aside className="sidebar"><div className="brand-lockup sidebar-brand"><div className="brand-mark"><KarmaShieldIcon /></div><div><strong>KARMA</strong><span>{t("brand.short")}</span></div></div><nav>{navItems.map((item) => { const Icon = item.icon; return <button className={page === item.key ? "active" : ""} key={item.key} onClick={() => setPage(item.key)}><Icon size={19} /><span>{t(item.label)}</span>{item.key === "evidence" && state.evidence.length > 0 && <b>{state.evidence.length}</b>}</button>; })}<div className="sidebar-actions">{saveStatus}<button className={`primary-button sidebar-save${dirty ? " unsaved" : ""}`} disabled={!dirty || saving} title={dirty ? t("common.unsavedChanges") : undefined} onClick={save}><Save size={17} /><span>{t(saving ? "common.saving" : "common.saveSettings")}</span></button><button className="sidebar-lock" onClick={signOut}><Lock size={18} /><span>{t("common.lockConsole")}</span></button></div></nav><div className="sidebar-foot"><div className="mini-health"><span className={state.serviceConnected ? "online" : "offline"} /><div><strong>{t(state.serviceConnected ? "sidebar.serviceOnline" : "sidebar.serviceOffline")}</strong><small>{t(state.agentConnected ? "sidebar.agentOnline" : "sidebar.consoleOnly")}</small></div></div>{appVersion && <div className="sidebar-version">{t("sidebar.version", { version: appVersion })}</div>}</div></aside><main className="main"><header><div><span className="eyebrow">KARMA CONTROL</span><h1>{t(meta.title)}</h1><p>{t(meta.subtitle)}</p></div></header><div className="page-content">{content}</div></main></div>;
}

export function App() {
  const [locale, setLocaleState] = useState<Locale>(getInitialLocale);
  const t = useMemo(() => createTranslator(locale), [locale]);
  const setLocale = (nextLocale: Locale) => { persistLocale(nextLocale); setLocaleState(nextLocale); };
  const value = useMemo(() => ({ locale, setLocale, t }), [locale, t]);
  return <I18nContext.Provider value={value}><SuperAuthorizationProvider><AppContent /></SuperAuthorizationProvider></I18nContext.Provider>;
}
