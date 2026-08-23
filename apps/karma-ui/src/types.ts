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

export interface ScheduleRule {
  id: string;
  name: string;
  days: number[];
  start: string;
  end: string;
  target: "internet" | "browser_player" | "all_restricted";
  enabled: boolean;
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
  keywords: KeywordRule[];
  websites: WebsiteRule[];
  applications: ApplicationRule[];
  schedules: ScheduleRule[];
  evidence: EvidenceItem[];
  audit: AuditItem[];
}
