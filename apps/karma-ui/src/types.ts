export type AuthMode = "setup" | "locked" | "unlocked";
export type PageKey =
  | "overview"
  | "monitors"
  | "recognition"
  | "keywords"
  | "websites"
  | "applications"
  | "schedule"
  | "evidence"
  | "audit"
  | "settings";

export interface MonitorStatus {
  id: string;
  name: string;
  resolution: string;
  state: "healthy" | "degraded" | "offline";
  fps: number;
  latencyMs: number;
}

export interface RecognitionSettings {
  imageEnabled: boolean;
  ocrEnabled: boolean;
  titleMatchingEnabled: boolean;
  sensitivity: number;
  immediateThreshold: number;
  evidenceEnabled: boolean;
  evidenceRetentionDays: number;
  rememberTitlesEnabled: boolean;
}

export interface RepeatOffenderSettings {
  /** Rolling window in which closures accumulate toward a ban. */
  windowMinutes: number;
  /** Closures within the window that ban an application for one hour. */
  strikeLimit: number;
}

export interface RememberedTitle {
  id: string;
  executable: string;
  title: string;
  browserHost: string | null;
  rememberedAtMs: number;
}

export interface ActiveBan {
  executable: string;
  bannedUntilMs: number;
}

export interface RememberedTitlePage {
  items: RememberedTitle[];
  /** Titles matching the query across all pages. */
  total: number;
}

export interface KeywordRule {
  id: string;
  phrase: string;
  category: "high_risk" | "sensitive" | "exemption";
  enabled: boolean;
}

export interface ApplicationRule {
  id: string;
  name: string;
  executable: string;
  category: "browser" | "player" | "game" | "custom";
  action: "allow" | "block" | "content_only";
  enabled: boolean;
}

export interface WebsiteRule {
  id: string;
  pattern: string;
  action: "allow" | "block";
  enabled: boolean;
}

export type NetworkTarget = "browsers" | "applications" | "all_network";

export interface ScheduleBlock {
  /** Monday = 0 through Sunday = 6. */
  day: number;
  start: string;
  /** An end that is not after the start continues into the next day. */
  end: string;
}

export interface ScheduleRule {
  id: string;
  name: string;
  target: NetworkTarget;
  applications: string[];
  blocks: ScheduleBlock[];
  enabled: boolean;
}

export interface NetworkStatus {
  restricted: boolean;
  activeRuleIds: string[];
  restrictedUntilMs: number | null;
  restoredUntilMs: number | null;
}

export interface EvidenceItem {
  id: string;
  capturedAt: string;
  monitorName: string;
  applicationName: string;
  reason: string;
  risk: "high" | "critical";
  thumbnailUrl?: string;
  originalAvailable: boolean;
}

export interface AuditItem {
  id: string;
  occurredAt: string;
  kind: string;
  detail: string;
  outcome: "success" | "warning" | "denied";
}

export interface ConsoleState {
  protectionEnabled: boolean;
  launchAtStartup: boolean;
  serviceConnected: boolean;
  agentConnected: boolean;
  monitors: MonitorStatus[];
  recognition: RecognitionSettings;
  repeatOffender: RepeatOffenderSettings;
  keywords: KeywordRule[];
  websites: WebsiteRule[];
  applications: ApplicationRule[];
  schedules: ScheduleRule[];
  evidence: EvidenceItem[];
  audit: AuditItem[];
  network: NetworkStatus;
  /** Remembered titles in the Service; the console reads them page by page. */
  rememberedTitleTotal: number;
  activeBans: ActiveBan[];
}
