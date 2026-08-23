import { invoke } from "@tauri-apps/api/core";
import { disable, enable } from "@tauri-apps/plugin-autostart";
import type { ConsoleState } from "./types";

const browserFallbackKey = "karma-ui-browser-state";
const browserPasswordKey = "karma-ui-browser-password";

export const defaultConsoleState: ConsoleState = {
  protectionEnabled: true,
  launchAtStartup: true,
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
  },
  keywords: [],
  websites: [],
  applications: [
    { id: "browser", name: "浏览器", executable: "受支持浏览器", category: "browser", action: "content_only", enabled: true },
    { id: "player", name: "播放器", executable: "受支持播放器", category: "player", action: "content_only", enabled: true },
  ],
  schedules: [],
  evidence: [],
  audit: [],
};

function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export function isTauriConsole(): boolean {
  return isTauri();
}

const runtimeStateKeys = ["serviceConnected", "agentConnected", "monitors", "evidence", "audit"];

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
    ...imported,
    recognition: { ...current.recognition, ...(imported.recognition ?? {}) },
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

export async function loadConsole(sessionToken: string): Promise<ConsoleState> {
  if (isTauri()) return hydrateConsoleState(await invoke("load_console", { sessionToken }));
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
    websites: value.websites ?? [],
  };
}

export async function saveConsole(sessionToken: string, state: ConsoleState): Promise<void> {
  if (isTauri()) return invoke("save_console", { sessionToken, state });
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

export async function revealEvidence(sessionToken: string, evidenceId: string, password: string): Promise<string> {
  if (isTauri()) return invoke("reveal_evidence", { sessionToken, evidenceId, password });
  if (localStorage.getItem(browserPasswordKey) !== await browserPasswordDigest(password)) throw new Error("管理员密码不正确");
  throw new Error("开发模式中没有已加密的证据原图");
}
