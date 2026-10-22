import { invoke } from "@tauri-apps/api/core";
import { disable, enable } from "@tauri-apps/plugin-autostart";
import type { ApplicationRule, AuthorizationStatus, InstalledApplication, ConsoleState, NetworkStatus, NetworkTarget, RememberedTitle, RememberedTitlePage, ScheduleBlock, ScheduleRule } from "./types";

const browserFallbackKey = "karma-ui-browser-state";
const browserPasswordKey = "karma-ui-browser-password";
const browserTitlesKey = "karma-ui-browser-titles";

export const defaultConsoleState: ConsoleState = {
  protectionEnabled: true,
  launchAtStartup: true,
  updates: { autoInstall: true },
  productVersion: "0.3.5",
  superPasswordSet: false,
  uninstall: null,
  update: { phase: "idle", lastCheckedAtMs: null, availableVersion: null, lastError: null },
  serviceConnected: false,
  agentConnected: false,
  monitors: [],
  recognition: {
    imageEnabled: true,
    ocrEnabled: true,
    titleMatchingEnabled: true,
    sensitivity: 82,
    immediateThreshold: 82,
    evidenceEnabled: false,
    evidenceRetentionDays: 7,
    evidenceSkipAllowedApplications: false,
    rememberTitlesEnabled: true,
    ambiguousTitleKeywordsEnabled: false,
  },
  repeatOffender: { windowMinutes: 20, strikeLimit: 5 },
  keywords: [],
  titleKeywordAdditions: [],
  websites: [],
  applications: [
    { id: "browser", name: "浏览器", executable: "受支持浏览器", category: "browser", action: "content_only", enabled: true },
    { id: "player", name: "播放器", executable: "受支持播放器", category: "player", action: "content_only", enabled: true },
  ],
  schedules: [],
  evidence: [],
  audit: [],
  network: { restricted: false, activeRuleIds: [], restrictedUntilMs: null, restoredUntilMs: null },
  rememberedTitleTotal: 0,
  misjudgedTitleTotal: 0,
  activeBans: [],
};

export const repeatOffenderLimits = {
  windowMinutes: { min: 1, max: 1440 },
  strikeLimit: { min: 2, max: 20 },
} as const;

function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export function isTauriConsole(): boolean {
  return isTauri();
}

const runtimeStateKeys = ["serviceConnected", "agentConnected", "monitors", "evidence", "audit", "network", "rememberedTitleTotal", "misjudgedTitleTotal", "activeBans", "productVersion", "superPasswordSet", "uninstall", "update"];

function stripRuntimeState(value: Record<string, unknown>): Record<string, unknown> {
  const stripped = { ...value };
  for (const key of runtimeStateKeys) delete stripped[key];
  return stripped;
}

function exportFileName(): string {
  return `karma-policy-${new Date().toISOString().slice(0, 10)}.json`;
}

export async function chooseExportPath(): Promise<string | null> {
  const { save } = await import("@tauri-apps/plugin-dialog");
  return save({
    defaultPath: exportFileName(),
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
}

export async function chooseImportPath(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selection = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  return typeof selection === "string" ? selection : null;
}

export async function exportSettings(sessionToken: string, path?: string): Promise<void> {
  if (isTauri()) {
    if (!path) throw new Error("无法写入导出文件");
    await invoke("export_settings", { sessionToken, path });
    return;
  }
  const stored = localStorage.getItem(browserFallbackKey);
  const policy: Record<string, unknown> = stored
    ? (JSON.parse(stored) as Record<string, unknown>)
    : { ...defaultConsoleState };
  const backup = {
    schema: "karma-policy-export",
    version: 1,
    exported_at_ms: Date.now(),
    policy: stripRuntimeState(policy),
  };
  const url = URL.createObjectURL(new Blob([JSON.stringify(backup, null, 2)], { type: "application/json" }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = exportFileName();
  anchor.click();
  URL.revokeObjectURL(url);
}

export async function importSettings(sessionToken: string, path: string): Promise<Partial<ConsoleState>> {
  if (isTauri()) return hydrateConsoleState(await invoke("import_settings", { sessionToken, path }));
  throw new Error("备份文件无法读取或格式不正确");
}

export async function parseImportedBackupFile(file: File): Promise<Partial<ConsoleState>> {
  let parsed: { schema?: unknown; version?: unknown; policy?: unknown };
  try {
    parsed = JSON.parse(await file.text());
  } catch {
    throw new Error("备份文件无法读取或格式不正确");
  }
  if (parsed.schema !== "karma-policy-export" || parsed.version !== 1 || typeof parsed.policy !== "object" || !parsed.policy) {
    throw new Error("备份文件无法读取或格式不正确");
  }
  return hydrateConsoleState(parsed.policy as Partial<ConsoleState>);
}

export function mergeImportedPolicy(current: ConsoleState, imported: Partial<ConsoleState>): ConsoleState {
  return hydrateConsoleState({
    ...current,
    ...stripRuntimeState(imported as Record<string, unknown>),
    recognition: { ...current.recognition, ...(imported.recognition ?? {}) },
    repeatOffender: { ...current.repeatOffender, ...(imported.repeatOffender ?? {}) },
    updates: { ...current.updates, ...(imported.updates ?? {}) },
    productVersion: current.productVersion,
    superPasswordSet: current.superPasswordSet,
    uninstall: current.uninstall,
    update: current.update,
  });
}

async function browserPasswordDigest(password: string): Promise<string> {
  const bytes = new TextEncoder().encode(`karma-ui-development-only:${password}`);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export async function authStatus(): Promise<"setup" | "locked"> {
  if (isTauri()) return invoke("auth_status");
  return localStorage.getItem(browserPasswordKey) ? "locked" : "setup";
}

export async function enroll(password: string): Promise<string> {
  if (isTauri()) return invoke("enroll", { password });
  localStorage.setItem(browserPasswordKey, await browserPasswordDigest(password));
  return "browser-development-session";
}

export async function unlock(password: string): Promise<string> {
  if (isTauri()) return invoke("unlock", { password });
  if (localStorage.getItem(browserPasswordKey) !== await browserPasswordDigest(password)) throw new Error("管理员密码不正确");
  return "browser-development-session";
}

export async function changePassword(sessionToken: string, currentPassword: string, newPassword: string): Promise<void> {
  if (isTauri()) return invoke("change_password", { sessionToken, currentPassword, newPassword });
  if (localStorage.getItem(browserPasswordKey) !== await browserPasswordDigest(currentPassword)) throw new Error("管理员密码不正确");
  if (newPassword.length < 10) throw new Error("管理员密码至少需要 10 个字符");
  localStorage.setItem(browserPasswordKey, await browserPasswordDigest(newPassword));
}

export async function lock(sessionToken: string): Promise<void> {
  if (isTauri()) await invoke("lock", { sessionToken });
}

export async function loadConsole(sessionToken: string, runtimeOnly = false): Promise<ConsoleState> {
  if (isTauri()) return hydrateConsoleState(await invoke("load_console", { sessionToken, runtimeOnly }));
  const stored = localStorage.getItem(browserFallbackKey);
  return stored ? hydrateConsoleState(JSON.parse(stored)) : defaultConsoleState;
}

function hydrateConsoleState(value: Partial<ConsoleState>): ConsoleState {
  return {
    ...defaultConsoleState,
    ...value,
    recognition: {
      ...defaultConsoleState.recognition,
      ...(value.recognition ?? {}),
    },
    repeatOffender: { ...defaultConsoleState.repeatOffender, ...(value.repeatOffender ?? {}) },
    updates: { ...defaultConsoleState.updates, ...(value.updates ?? {}) },
    update: { ...defaultConsoleState.update, ...(value.update ?? {}) },
    titleKeywordAdditions: value.titleKeywordAdditions ?? [],
    websites: value.websites ?? [],
    schedules: (value.schedules ?? []).map(normalizeSchedule),
    network: { ...defaultConsoleState.network, ...(value.network ?? {}) },
    rememberedTitleTotal: isTauri() ? value.rememberedTitleTotal ?? 0 : browserTitles().length,
    misjudgedTitleTotal: isTauri() ? value.misjudgedTitleTotal ?? 0 : 0,
    activeBans: value.activeBans ?? [],
  };
}

const legacyTargets: Record<string, NetworkTarget> = { browser_player: "browsers", internet: "all_network", all_restricted: "all_network" };

/** Converts single-range rules saved by earlier consoles into weekly blocks. */
export function normalizeSchedule(rule: ScheduleRule | Record<string, unknown>): ScheduleRule {
  const value = rule as Record<string, unknown>;
  if (Array.isArray(value.blocks)) {
    return { ...(rule as ScheduleRule), applications: Array.isArray(value.applications) ? (value.applications as string[]) : [] };
  }
  const days = Array.isArray(value.days) ? (value.days as number[]) : [];
  const blocks: ScheduleBlock[] = days.map((day) => ({ day, start: String(value.start ?? "21:00"), end: String(value.end ?? "07:00") }));
  return {
    id: String(value.id ?? crypto.randomUUID()),
    name: String(value.name ?? ""),
    target: legacyTargets[String(value.target)] ?? "browsers",
    applications: [],
    blocks,
    enabled: value.enabled !== false,
  };
}

export async function saveConsole(sessionToken: string, state: ConsoleState, superPassword?: string): Promise<void> {
  if (isTauri()) return invoke("save_console", { sessionToken, state, superPassword });
  localStorage.setItem(browserFallbackKey, JSON.stringify(state));
}

export async function configureLaunchAtStartup(enabled: boolean): Promise<void> {
  if (!isTauri()) return;
  if (enabled) {
    await enable();
  } else {
    await disable();
  }
}

export async function getAppVersion(): Promise<string> {
  if (!isTauri()) return "dev";
  try {
    const { getVersion } = await import("@tauri-apps/api/app");
    return await getVersion();
  } catch {
    return "dev";
  }
}

export async function restoreNetwork(sessionToken: string, password: string): Promise<NetworkStatus> {
  if (isTauri()) return invoke("restore_network", { sessionToken, password });
  if (localStorage.getItem(browserPasswordKey) !== await browserPasswordDigest(password)) throw new Error("管理员密码不正确");
  return defaultConsoleState.network;
}

/** Development-mode titles seeded into localStorage; the Service owns real ones. */
function browserTitles(): RememberedTitle[] {
  try {
    const stored = JSON.parse(localStorage.getItem(browserTitlesKey) ?? "[]");
    return Array.isArray(stored) ? (stored as RememberedTitle[]) : [];
  } catch {
    return [];
  }
}

export const rememberedTitlePageSize = 20;

export async function listRememberedTitles(sessionToken: string, offset: number, query: string, misjudged = false): Promise<RememberedTitlePage> {
  const trimmed = query.trim();
  if (isTauri()) return invoke("list_remembered_titles", { sessionToken, offset, limit: rememberedTitlePageSize, query: trimmed || null, misjudged });
  if (misjudged) return { items: [], total: 0 };
  const needle = trimmed.toLowerCase();
  const matching = browserTitles()
    .filter((item) => !needle || [item.title, item.browserHost ?? "", item.executable].some((value) => value.toLowerCase().includes(needle)))
    .sort((left, right) => right.rememberedAtMs - left.rememberedAtMs);
  return { items: matching.slice(offset, offset + rememberedTitlePageSize), total: matching.length };
}

export async function forgetRememberedTitle(sessionToken: string, titleId: string, superPassword?: string): Promise<void> {
  if (isTauri()) return invoke("forget_remembered_title", { sessionToken, titleId, superPassword });
  localStorage.setItem(browserTitlesKey, JSON.stringify(browserTitles().filter((item) => item.id !== titleId)));
}

/** Always allows a banned executable and lifts its ban; returns the saved application rules. */
export async function allowBannedApplication(sessionToken: string, executable: string, superPassword?: string): Promise<ApplicationRule[]> {
  if (isTauri()) return invoke("allow_banned_application", { sessionToken, executable, superPassword });
  throw new Error("KarmaService 尚未运行或命名管道不可用");
}

/** Always allows the executable the Service attributed an evidence item to; returns the saved application rules. */
export async function allowEvidenceApplication(sessionToken: string, evidenceId: string, superPassword?: string): Promise<ApplicationRule[]> {
  if (isTauri()) return invoke("allow_evidence_application", { sessionToken, evidenceId, superPassword });
  throw new Error("KarmaService 尚未运行或命名管道不可用");
}

/** Programs registered as installed on this computer; none outside the desktop console. */
export async function listInstalledApplications(sessionToken: string): Promise<InstalledApplication[]> {
  if (isTauri()) return invoke("list_installed_applications", { sessionToken });
  return [];
}

/** Mirrors the Service's application rule limit. */
export const maxApplicationRules = 256;

function pathComponents(value: string): string[] {
  return value.split(/[\\/]/).filter(Boolean).map((part) => part.toLowerCase());
}

function trailingMatch(observed: string[], rule: string[]): boolean {
  return rule.length > 0 && observed.length >= rule.length && rule.every((part, index) => observed[observed.length - rule.length + index] === part);
}

/** Resolves an executable like the Service does: an enabled block rule outranks allow. */
export function applicationEffect(rules: ApplicationRule[], executable: string): ApplicationRule["action"] {
  const observed = pathComponents(executable);
  let effect: ApplicationRule["action"] = "content_only";
  for (const rule of rules) {
    if (!rule.enabled || !trailingMatch(observed, pathComponents(rule.executable))) continue;
    if (rule.action === "block") return "block";
    if (rule.action === "allow") effect = "allow";
  }
  return effect;
}

/**
 * Always allows each application, as the Service's one-click allow does: a rule
 * for exactly that executable becomes an enabled "Allow" rule, an enabled
 * "Always block" rule that would outrank it becomes "Allow", and otherwise a
 * new "Allow" rule is appended.
 */
export function allowApplications(rules: ApplicationRule[], applications: InstalledApplication[]): ApplicationRule[] {
  let next = rules;
  for (const application of applications) {
    const target = pathComponents(application.executable);
    let exact = false;
    next = next.map((rule) => {
      const components = pathComponents(rule.executable);
      if (components.length === target.length && trailingMatch(target, components)) {
        exact = true;
        return { ...rule, action: "allow", enabled: true };
      }
      return rule.enabled && rule.action === "block" && trailingMatch(target, components) ? { ...rule, action: "allow" } : rule;
    });
    if (!exact) next = [...next, { id: crypto.randomUUID(), name: application.name, executable: application.executable, category: application.browser ? "browser" : "custom", action: "allow", enabled: true }];
  }
  return next;
}

/** Forgets a remembered title and keeps it from being remembered again. */
export async function markTitleMisjudged(sessionToken: string, titleId: string, superPassword?: string): Promise<void> {
  if (isTauri()) return invoke("mark_title_misjudged", { sessionToken, titleId, superPassword });
  localStorage.setItem(browserTitlesKey, JSON.stringify(browserTitles().filter((item) => item.id !== titleId)));
}

export async function restoreMisjudgedTitle(sessionToken: string, titleId: string): Promise<void> {
  if (isTauri()) return invoke("restore_misjudged_title", { sessionToken, titleId });
}

export async function revealEvidence(sessionToken: string, evidenceId: string, password: string): Promise<string> {
  if (isTauri()) return invoke("reveal_evidence", { sessionToken, evidenceId, password });
  if (localStorage.getItem(browserPasswordKey) !== await browserPasswordDigest(password)) throw new Error("管理员密码不正确");
  throw new Error("开发模式中没有已加密的证据原图");
}

/** Deletes evidence items; an ordinary administrator session is enough. */
export async function deleteEvidence(sessionToken: string, evidenceIds: string[]): Promise<void> {
  if (!evidenceIds.length) return;
  if (isTauri()) return invoke("delete_evidence", { sessionToken, evidenceIds });
  throw new Error("开发模式中没有已加密的证据原图");
}

export function isSuperPasswordRequired(reason: unknown): boolean {
  return (reason instanceof Error ? reason.message : String(reason)) === "super_password_required";
}

export function isAuthorizationCancelled(reason: unknown): boolean {
  return (reason instanceof Error ? reason.message : String(reason)) === "super_password_cancelled";
}

export async function loadAuthorizationStatus(sessionToken: string): Promise<AuthorizationStatus> {
  if (isTauri()) return invoke("authorization_status", { sessionToken });
  return { productVersion: defaultConsoleState.productVersion, superPasswordSet: false, uninstall: null, update: defaultConsoleState.update };
}

export async function setSuperPassword(sessionToken: string, adminPassword: string, superPassword: string): Promise<void> {
  if (isTauri()) return invoke("set_super_password", { sessionToken, adminPassword, superPassword });
  throw new Error("KarmaService 尚未运行或命名管道不可用");
}

export async function changeSuperPassword(sessionToken: string, currentSuperPassword: string, newSuperPassword: string): Promise<void> {
  if (isTauri()) return invoke("change_super_password", { sessionToken, currentSuperPassword, newSuperPassword });
  throw new Error("KarmaService 尚未运行或命名管道不可用");
}

export async function requestUninstall(sessionToken: string, password: string): Promise<void> {
  if (isTauri()) return invoke("request_uninstall", { sessionToken, password });
  throw new Error("KarmaService 尚未运行或命名管道不可用");
}

export async function cancelUninstall(sessionToken: string): Promise<void> {
  if (isTauri()) return invoke("cancel_uninstall", { sessionToken });
  throw new Error("KarmaService 尚未运行或命名管道不可用");
}

export async function checkForUpdates(sessionToken: string): Promise<void> {
  if (isTauri()) return invoke("check_for_updates", { sessionToken });
  throw new Error("KarmaService 尚未运行或命名管道不可用");
}
