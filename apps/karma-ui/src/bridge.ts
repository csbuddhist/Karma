import { invoke } from "@tauri-apps/api/core";
import type { ConsoleState } from "./types";

const browserFallbackKey = "karma-ui-browser-state";
const browserPasswordKey = "karma-ui-browser-password";

export const defaultConsoleState: ConsoleState = {
  protectionEnabled: true,
  serviceConnected: false,
  agentConnected: false,
  monitors: [],
  recognition: {
    imageEnabled: true,
    ocrEnabled: true,
    sensitivity: 82,
    immediateThreshold: 95,
    evidenceEnabled: false,
    evidenceRetentionDays: 7,
  },
  keywords: [],
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

export async function lock(sessionToken: string): Promise<void> {
  if (isTauri()) await invoke("lock", { sessionToken });
}

export async function loadConsole(sessionToken: string): Promise<ConsoleState> {
  if (isTauri()) return invoke("load_console", { sessionToken });
  const stored = localStorage.getItem(browserFallbackKey);
  return stored ? JSON.parse(stored) : defaultConsoleState;
}

export async function saveConsole(sessionToken: string, state: ConsoleState): Promise<void> {
  if (isTauri()) return invoke("save_console", { sessionToken, state });
  localStorage.setItem(browserFallbackKey, JSON.stringify(state));
}

export async function revealEvidence(sessionToken: string, evidenceId: string, password: string): Promise<string> {
  if (isTauri()) return invoke("reveal_evidence", { sessionToken, evidenceId, password });
  if (localStorage.getItem(browserPasswordKey) !== await browserPasswordDigest(password)) throw new Error("管理员密码不正确");
  throw new Error("开发模式中没有已加密的证据原图");
}
