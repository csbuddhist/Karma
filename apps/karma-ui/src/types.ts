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
  /** Discard evidence attributed to an application with an enabled "Allow" rule. */
  evidenceSkipAllowedApplications: boolean;
  rememberTitlesEnabled: boolean;
  /** Also enforce built-in keywords that are likely to cause false positives. */
  ambiguousTitleKeywordsEnabled: boolean;
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
  /** When it was remembered, or marked as misjudged. */
  rememberedAtMs: number;
  /** The image recognition result that remembered it; null for older titles and misjudged ones. */
  reason?: TitleReason | null;
}

export interface TitleReason {
  reasonCode: string;
  riskMillis: number;
  thresholdMillis: number;
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

/** Categories every policy has; `custom` holds applications without a more specific one. */
export type BuiltInApplicationCategory = "browser" | "player" | "game" | "custom";

/** An administrator-defined application category. */
export interface ApplicationCategory {
  id: string;
  name: string;
}

export interface ApplicationRule {
  id: string;
  name: string;
  executable: string;
  /** A built-in category or the id of an `ApplicationCategory`. */
  category: string;
  action: "allow" | "block" | "content_only";
  enabled: boolean;
}

/** A program the console found in the Windows installed-program registrations. */
export interface InstalledApplication {
  name: string;
  executable: string;
  publisher?: string | null;
  /** A known browser: allowing it exempts every page it shows. */
  browser?: boolean;
}

export interface WebsiteRule {
  id: string;
  pattern: string;
  action: "allow" | "block";
  enabled: boolean;
}

/** Rules saved before categories existed used `browsers`, which the console reads as the browser category. */
export type NetworkTarget = "categories" | "applications" | "all_network";

/** `block_launch` terminates the covered applications; `all_network` supports only `block_network`. */
export type ScheduleAction = "block_network" | "block_launch";

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
  action: ScheduleAction;
  target: NetworkTarget;
  /** Category ids covered when `target` is `categories`. */
  categories: string[];
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

/** "code" holds product-code prefixes such as "SSIS", which always use "code" matching. */
export type TitleKeywordCategory = "organ" | "sexual" | "code";
export type TitleKeywordMatch = "word" | "substring" | "prefix" | "code";

export interface TitleKeywordAddition {
  id: string;
  term: string;
  /** Lowercase ISO 639 language code, such as "zh". */
  language: string;
  category: TitleKeywordCategory;
  match: TitleKeywordMatch;
}

export interface EvidenceItem {
  id: string;
  capturedAt: string;
  monitorName: string;
  applicationName: string;
  reason: string;
  risk: "high" | "critical";
  /** Image risk score that saved it, 0–1000; absent in development builds. */
  riskMillis?: number;
  thumbnailUrl?: string;
  originalAvailable: boolean;
}

/** Why the Service required a window to be closed. */
export type ClosureReason = "image" | "remembered_title" | "blocked_website" | "title_keyword" | "blocked_application" | "repeat_offender_ban";

/** Title-free context the Service records with some audit rows. */
export interface AuditDetail {
  executable?: string;
  reason?: ClosureReason;
  riskMillis?: number;
  thresholdMillis?: number;
  /** Matched title keyword, masked by the Service, for `title_keyword` closures. */
  keyword?: string;
}

export interface AuditItem {
  id: string;
  occurredAtMs: number;
  kind: string;
  /** Raw Service outcome, such as `success`, `denied`, or `terminated`. */
  outcome: string;
  detail: AuditDetail | null;
}

export interface UninstallStatus {
  requestedAtMs: number;
  effectiveAtMs: number;
  expiresAtMs: number;
}

export interface UpdateStatus {
  phase: "idle" | "checking" | "downloading" | "installing" | "up_to_date" | "failed";
  lastCheckedAtMs: number | null;
  availableVersion: string | null;
  lastError: string | null;
}

export interface AuthorizationStatus {
  productVersion: string;
  superPasswordSet: boolean;
  uninstall: UninstallStatus | null;
  update: UpdateStatus;
}

export interface ConsoleState extends AuthorizationStatus {
  protectionEnabled: boolean;
  launchAtStartup: boolean;
  updates: { autoInstall: boolean };
  serviceConnected: boolean;
  agentConnected: boolean;
  monitors: MonitorStatus[];
  recognition: RecognitionSettings;
  repeatOffender: RepeatOffenderSettings;
  keywords: KeywordRule[];
  /** Keywords administrators added to the built-in list. */
  titleKeywordAdditions: TitleKeywordAddition[];
  websites: WebsiteRule[];
  applicationCategories: ApplicationCategory[];
  applications: ApplicationRule[];
  schedules: ScheduleRule[];
  evidence: EvidenceItem[];
  audit: AuditItem[];
  network: NetworkStatus;
  /** Remembered titles in the Service; the console reads them page by page. */
  rememberedTitleTotal: number;
  /** Titles marked as misjudged, which are never remembered again. */
  misjudgedTitleTotal: number;
  activeBans: ActiveBan[];
}
