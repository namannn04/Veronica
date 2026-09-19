/** Mirrors of the Rust types crossing the IPC boundary. */

export type CapabilityState =
  | { state: "available" }
  | { state: "permissionRequired"; reason: string }
  | { state: "integrationRequired"; reason: string }
  | { state: "unsupported"; reason: string };

export type Availability =
  | { availability: "available" }
  | { availability: "degraded"; missing: string[] }
  | { availability: "unavailable"; missing: string[] };

export type ExtensionGroup = "agent" | "system" | "media" | "utilities";

export interface ExtensionReport extends Record<string, unknown> {
  id: string;
  title: string;
  subtitle: string;
  icon: string;
  /** The settings key holding the on/off state, from the Rust catalogue. */
  defaultsKey: string;
  group: ExtensionGroup;
  featured: boolean;
  enabled: boolean;
  availability: Availability["availability"];
  missing?: string[];
}

export type ToolReadiness =
  | { state: "installed"; path: string; version: string }
  | { state: "uninstalled" }
  | { state: "error"; detail: string };

export type ToolReport = {
  id: string;
  displayName: string;
  why: string;
  instruction: string;
  /** The extensions that named this tool. */
  wantedBy: string[];
} & ToolReadiness;

export interface ToolSurvey {
  tools: ToolReport[];
  /**
   * Extension id to the tool ids it needs and does not have. Rust decides,
   * because Agent Usage is satisfied by either provider and nothing else is.
   * A satisfied extension is absent rather than present and empty.
   */
  unmet: Record<string, string[] | undefined>;
}

export interface DesktopSession {
  kind: "wayland" | "x11" | "headless";
  desktop: string;
  isGnome: boolean;
  shellVersion: string | null;
  hasGlobalShortcutsPortal: boolean;
  hasContainerRuntime: boolean;
  hasPipewire: boolean;
  /** The GDK backend this process connected to, which can differ from `kind`. */
  toolkitBackend: string;
}

export interface Diagnostics {
  version: string;
  session: DesktopSession;
  directories: {
    configuration: string;
    data: string;
    cache: string;
    state: string;
    runtime: string;
  };
  capabilities: { states: Record<string, CapabilityState> };
  extensions: ExtensionReport[];
}

export interface Totals {
  cost: number;
  tokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheCreationTokens: number;
  cacheReadTokens: number;
}

export interface DayPoint {
  period: string;
  cost: number;
  tokens: number;
}

export interface NamedAmount {
  name: string;
  label: string;
  cost: number;
  tokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheCreationTokens: number;
  cacheReadTokens: number;
}

export interface HourPoint {
  hour: number;
  cost: number;
  tokens: number;
}

export interface HeatmapCell {
  period: string;
  cost: number;
  tokens: number;
  /** 0 for an idle day, then 1-4 by rank among active days. */
  level: number;
}

export interface ChatEntry {
  id: string;
  path: string;
  title: string;
  tokens: number;
  cost: number;
  firstTs: number | null;
  lastTs: number | null;
  source: string;
}

export interface ProjectRollup {
  projectName: string;
  repositoryID: string | null;
  repositoryURL: string | null;
  path: string;
  cost: number;
  tokens: number;
  chats: ChatEntry[];
}

export interface Dashboard {
  totals: Totals;
  days: DayPoint[];
  byModel: NamedAmount[];
  bySource: NamedAmount[];
  byHour: HourPoint[];
  heatmap: HeatmapCell[];
  projects: ProjectRollup[];
  activeDays: number;
  sessionCount: number;
}

export interface UsageView {
  dashboard: Dashboard | null;
  generatedAt: string | null;
  sources: { id: string; label: string }[];
  /** Stable, document-wide model order, for fixed colour slots. */
  models: string[];
  hasData: boolean;
}

export type CollectorEvent =
  | { kind: "phase"; name: string; detail: string; seconds: number }
  | { kind: "note"; message: string }
  | { kind: "summary"; name: string; detail: string }
  | { kind: "error"; message: string }
  | { kind: "done"; seconds: number }
  | { kind: "unknown"; line: string };

export interface SystemSnapshot {
  hostName: string | null;
  kernel: string | null;
  distribution: string;
  uptimeSecs: number;
  cpu: {
    usagePercent: number;
    perCore: number[];
    physicalCores: number | null;
    logicalCores: number;
    brand: string;
    frequencyMhz: number;
  };
  memory: {
    totalBytes: number;
    usedBytes: number;
    availableBytes: number;
    swapTotalBytes: number;
    swapUsedBytes: number;
  };
  disks: {
    name: string;
    mountPoint: string;
    fileSystem: string;
    totalBytes: number;
    availableBytes: number;
    removable: boolean;
  }[];
  loadAverage: [number, number, number];
  temperatures: { label: string; celsius: number; criticalCelsius: number | null }[];
  battery: { percent: number; charging: boolean; timeToEmptySecs: number | null } | null;
}

export interface RunningProcess {
  pid: number;
  name: string;
  executable: string | null;
  cpuPercent: number;
  memoryBytes: number;
}

export interface HerdrSession {
  name: string;
  running: boolean;
  default: boolean;
  session_dir: string;
  socket_path: string;
  error?: string | null;
}

export interface HerdrAgent {
  id: string;
  session: string;
  kind: string;
  status: "blocked" | "working" | "unknown" | "done" | "idle";
  title: string;
  workspace: string;
  cwd: string;
  paneId: string;
  focused: boolean;
  /** The exact line that attaches to this agent, built by the backend. */
  attachCommand: string;
}

export interface HerdrBoard {
  installed: boolean;
  executable: string | null;
  sessions: HerdrSession[];
  agents: HerdrAgent[];
  /** Why the board is emptier than expected, when Herdr answered oddly. */
  error: string | null;
}

export interface VolumeState {
  volume: number;
  muted: boolean;
}

/** One application's PipeWire stream, as the per-app mixer shows it. */
export interface AudioStream {
  /** PipeWire node id — the handle every mixer call takes. */
  id: number;
  application: string;
  mediaName: string | null;
  direction: "playback" | "capture";
  /** 0-1, on the scale every mixer displays, not PipeWire's cubed storage. */
  volume: number;
  muted: boolean;
}

export interface BluetoothAdapter {
  id: string;
  name: string;
  address: string;
  powered: boolean;
  discovering: boolean;
  discoverable: boolean;
}

export interface BluetoothDevice {
  address: string;
  name: string;
  paired: boolean;
  trusted: boolean;
  connected: boolean;
  /** null when the device publishes no battery, which is not the same as 0%. */
  batteryPercent: number | null;
  rssi: number | null;
  icon: string | null;
}

export interface BluetoothState {
  adapters: BluetoothAdapter[];
  devices: BluetoothDevice[];
  /** Set when BlueZ could not be reached: "cannot tell", not "nothing paired". */
  unavailable: string | null;
}

export type PlaybackStatus = "playing" | "paused" | "stopped";

export interface NowPlaying {
  /** The player's bus name suffix, e.g. "spotify". */
  player: string;
  identity: string;
  status: PlaybackStatus | null;
  title: string;
  artist: string;
  album: string;
  artUrl: string | null;
  /** Microseconds, as MPRIS reports them; null when the player reports unknown. */
  lengthUs: number | null;
  positionUs: number | null;
  canGoNext: boolean;
  canGoPrevious: boolean;
}

export type MediaAction = "play" | "pause" | "toggle" | "next" | "previous" | "stop";

export interface CalendarEvent {
  sourceUid: string;
  eventUid: string;
  summary: string;
  /** RFC 3339, local offset. */
  start: string;
  end: string;
  allDay: boolean;
  joinUrl: string | null;
}

export interface AgendaDay {
  /** `YYYY-MM-DD`, local. */
  date: string;
  /** "Today", "Tomorrow", a weekday, or a date. */
  label: string;
  isToday: boolean;
  events: CalendarEvent[];
}

export interface AgendaView {
  hasCalendars: boolean;
  days: AgendaDay[];
  nextUp: CalendarEvent | null;
  happeningNow: CalendarEvent | null;
}

export type NotificationUrgency = "low" | "normal" | "critical";

export interface DesktopNotification {
  id: number;
  appName: string;
  appIcon: string;
  summary: string;
  body: string;
  urgency: NotificationUrgency;
  /** Unix milliseconds. */
  receivedAt: number;
  desktopEntry: string | null;
}

export type MachineReach =
  | { kind: "local" }
  | { kind: "ssh"; target: string; port?: number };

export interface Machine {
  id: string;
  name: string;
  reach: MachineReach;
  /** Set only where the user gave one, with `vr machines add --mac`. */
  mac?: string | null;
}
export interface MachineFile { name: string; path: string; kind: "directory" | "file" | "link" | "other"; sizeBytes: number; modifiedUnix: number; }
export interface MachineDirectory { path: string; parent: string | null; entries: MachineFile[]; }
export interface ContainerInfo { id: string; name: string; image: string; status: string; state: string; engine: "docker" | "podman"; }

export interface MachineDisk {
  mountPoint: string;
  totalBytes: number;
  availableBytes: number;
}

export interface MachineStats {
  hostName: string;
  kernel: string;
  os: string;
  uptimeSecs: number;
  loadAverage: [number, number, number];
  cpuPercent: number;
  memoryTotalBytes: number;
  memoryAvailableBytes: number;
  swapTotalBytes: number;
  swapFreeBytes: number;
  disks: MachineDisk[];
  temperatures: { label: string; celsius: number }[];
  fans: { label: string; rpm: number }[];
  gpus: MachineGpu[];
  /** The busiest processes on that machine, hottest first. */
  processes: { pid: number; name: string; cpuPercent: number; memoryBytes: number }[];
}

export interface MachineGpu {
  name: string;
  utilizationPercent: number;
  memoryUsedBytes: number;
  memoryTotalBytes: number;
  /** null where the driver reports none, which is not a GPU at 0 °C. */
  temperatureCelsius: number | null;
}

export interface MachineReport {
  machine: Machine;
  stats: MachineStats | null;
  /** Why the probe failed, when it did. */
  error: string | null;
}

export interface ClipRow {
  id: number;
  preview: string;
  /** The full text, so copying back needs no second call. */
  text: string;
  lines: number;
  bytes: number;
  count: number;
  lastSeen: string;
}

export type BlurCategory = "money" | "usage" | "agents" | "calendar" | "music";

export interface ScreenShareState {
  sharing: boolean;
  screencastSessions: number;
  remoteSessions: number;
  reason: string | null;
  /** Set when detection could not run at all, e.g. on a non-GNOME desktop. */
  unavailable: string | null;
}

export interface PresenterView {
  enabled: boolean;
  manual: boolean;
  autoEnabled: boolean;
  autoActive: boolean;
  autoPaused: boolean;
  autoReason: string | null;
  categories: BlurCategory[];
  /** The resolved gate: enabled && (manual || an undismissed detected share). */
  active: boolean;
  /** One CSS class per category blurred right now. */
  blurredClasses: string[];
  share: ScreenShareState;
}

export interface NotifySettings {
  master: boolean;
  trackSession: boolean;
  trackWeekly: boolean;
  recovery: boolean;
  pacingWarning: boolean;
  pacingHot: boolean;
  reminderSession: boolean;
  reminderSessionOffsetMin: number;
  reminderWeekly: boolean;
  reminderWeeklyOffsetMin: number;
  tokenExpired: boolean;
  /** Blend time remaining into the level rather than using the raw percentage. */
  smartColor: boolean;
  pacingMargin: number;
  thresholds: { warningPercent: number; criticalPercent: number };
}

export interface WatchedWindow {
  percent: number;
  resetsAt: string | null;
  /** "2 h 14 min", absent when the provider gave no reset time. */
  resetsIn: string | null;
  level: UsageLevel;
  zone: PacingZone;
}

export interface AlertsView {
  settings: NotifySettings;
  /** Seconds between polls, after clamping. */
  pollSeconds: number;
  session: WatchedWindow | null;
  week: WatchedWindow | null;
  /** Why there is nothing to watch, when that is the case. */
  note: string | null;
  sessionReminderAt: string | null;
  weekReminderAt: string | null;
}

export interface SwatchRow {
  id: number;
  hex: string;
  /** Components in 0..1, in `profile`'s space. */
  red: number;
  green: number;
  blue: number;
  profile: "srgb" | "displayP3";
  profileLabel: string;
  pickedAt: string;
  /** Every representation, keyed by format id, so copying needs no round trip. */
  formats: Record<string, string>;
  /** Whether a dark label is legible on this colour. */
  prefersDarkText: boolean;
}

export interface CopyFormatOption {
  id: string;
  label: string;
}

export interface PickResult {
  swatch: SwatchRow;
  /** What was put on the clipboard, in the configured format. */
  value: string;
  format: string;
  /** Which backend opened the eyedropper. */
  source: string;
  copiedVia: string | null;
  /** Set when the colour was recorded but the clipboard refused it. */
  copyError: string | null;
}

export interface CopyResult {
  value: string;
  format: string;
  copiedVia: string;
}

export interface BackupSummary {
  path: string;
  appVersion: string;
  createdAt: string;
  files: number;
  decodedBytes: number;
}

export interface ImportReport {
  filesRestored: number;
  bytesRestored: number;
  sourceVersion: string;
  createdAt: string;
}

export interface AttentionFocusSession {
  id: string;
  name: string;
  startedAt: string;
  plannedDurationSeconds: number;
  endedAt: string | null;
}

export interface AttentionStatus {
  active: AttentionFocusSession | null;
  completedSessions: number;
  totalFocusSeconds: number;
}
export interface AttentionCategory { id: string; name: string; color: string; applications: string[]; }
export interface AttentionSettings { enabled: boolean; idleThresholdSeconds: number; privacy: "applications" | "detailed"; categories: AttentionCategory[]; }
export interface AttentionEvent { id: string; startedAt: string; durationSeconds: number; application: string; title: string | null; idle: boolean; }
export interface AttentionOverview { from: string; to: string; activeSeconds: number; idleSeconds: number; focusedSeconds: number; contextSwitches: number; applications: [string, number][]; categories: [string, number][]; events: AttentionEvent[]; }
export interface CompanionItem { id: number; kind: "note" | "voice"; title: string; body: string; pinned: boolean; createdAt: string; updatedAt: string; audioPath: string | null; durationSeconds: number | null; }
export interface CompanionRecordingStatus { recording: boolean; elapsedSeconds: number; }
export interface LocalTrack { id: string; path: string; title: string; artist: string; album: string; artPath: string | null; }
export interface PowerStatus {
  hasLid: boolean;
  lidAwakeActive: boolean;
  preventSleepActive: boolean;
  /** Seconds left on a timed session; null when open-ended or off. */
  lidAwakeRemainingSecs: number | null;
  preventSleepRemainingSecs: number | null;
}
export interface UpdateInfo { currentVersion: string; latestVersion: string; updateAvailable: boolean; releaseUrl: string; packageUrl: string | null; publishedAt: string | null; notes: string; }

export type UsageLevel = "green" | "orange" | "red";
export type PacingZone = "chill" | "onTrack" | "warning" | "hot";

export interface Gauge {
  /** "Claude" or "Codex". */
  provider: string;
  /** "Session", "Week", or a model-scoped label. */
  window: string;
  percent: number;
  resetsInSecs: number | null;
  /** 0-1, blending absolute use, projected overrun and pace. */
  risk: number;
  level: UsageLevel;
  zone: PacingZone;
  /** Percentage points ahead of a linear burn; negative means behind. */
  paceDelta: number | null;
}

export interface GaugeReport {
  gauges: Gauge[];
  /** Why a provider contributed nothing. */
  notes: string[];
}

export interface EmojiGroup {
  id: string;
  name: string;
  /** Edith stores an SF Symbol name here; the web interface uses its own glyphs. */
  symbol: string;
}

export interface EmojiRow {
  /** Already in the configured skin tone, which is what gets copied. */
  character: string;
  name: string;
  groupIndex: number;
  supportsSkinTones: boolean;
}

export interface EmojiCopyResult {
  character: string;
  copiedVia: string;
  /** Whether it also went into the app you were typing in. */
  inserted: boolean;
}

export interface CleanerCategory {
  id: string;
  title: string;
  /** What removing it costs — the only thing that makes an informed choice possible. */
  cost: string;
  family: "cache" | "project";
  onByDefault: boolean;
  paths: string[];
  directoryNames: string[];
}

export interface CleanerItem { category: string; path: string; bytes: number }

export interface CleanerScan {
  items: CleanerItem[];
  /** Total per category id, so a summary needs no second pass. */
  totals: Record<string, number>;
  totalBytes: number;
}

export interface CleanReport {
  trashed: CleanerItem[];
  /** [path, why]. One failure never stops the rest. */
  failed: [string, string][];
  bytesReclaimed: number;
}

export interface PackageSource {
  source: "apt" | "snap" | "flatpak";
  available: boolean;
  /** Whether changing anything here raises an authentication dialog. */
  needsRoot: boolean;
}

export interface PackageUpdate {
  source: "apt" | "snap" | "flatpak";
  name: string;
  /** null where the source does not report what is installed. */
  installedVersion: string | null;
  availableVersion: string;
  origin: string | null;
}

export interface PackageInstalled {
  source: "apt" | "snap" | "flatpak";
  name: string;
  version: string;
  sizeBytes: number | null;
}

export interface RemovalPlan {
  source: "apt" | "snap" | "flatpak";
  package: string;
  /** Everything that would go, including the package itself. */
  removed: string[];
  nowUnused: string[];
  command: string;
  /** Whether this takes more than the package that was named. */
  removesMoreThanAsked: boolean;
}

export interface PackageUpdateResult {
  source: string;
  names: string[];
  command: string;
  succeeded: boolean;
  output: string;
}

export type AuditSeverity = "error" | "warning" | "notice";

export interface AuditIssue {
  code: string;
  severity: AuditSeverity;
  title: string;
  detail: string;
}

export interface AuditMetadata {
  title: string | null;
  description: string | null;
  canonicalUrl: string | null;
  robots: string | null;
  language: string | null;
  heading: string | null;
  openGraphTitle: string | null;
  openGraphDescription: string | null;
  openGraphImageUrl: string | null;
  openGraphType: string | null;
  twitterCard: string | null;
  twitterTitle: string | null;
  twitterDescription: string | null;
  twitterImageUrl: string | null;
  wordCount: number;
}

export interface AuditPage {
  url: string;
  statusCode: number | null;
  responseMillis: number | null;
  bytes: number;
  metadata: AuditMetadata;
  issues: AuditIssue[];
  /** Set when the page could not be fetched at all — not the same as a 500. */
  error: string | null;
}

export interface AuditReport {
  site: string;
  startedAt: string;
  pages: AuditPage[];
  errors: number;
  warnings: number;
  notices: number;
  /** [code, severity, how many pages have it], most common first. */
  byCode: [string, AuditSeverity, number][];
}

export interface QuinjetWorktree {
  path: string;
  head: string;
  branch: string | null;
  current: boolean;
  bare: boolean;
  detached: boolean;
  locked: string | null;
  prunable: string | null;
}

export interface QuinjetProject {
  name: string;
  commonDir: string;
  worktrees: QuinjetWorktree[];
}

export interface QuinjetBoard {
  /** False rather than an error: not having Quinjet is a normal state. */
  installed: boolean;
  projects: QuinjetProject[];
}

/** A saved connection. Never carries a credential — that is the whole point. */
export interface DatabaseConnection {
  id: string;
  displayName: string;
  productHint: string;
  environment: { kind: string; label: string; protection: string };
  readOnlyPolicy: "disabled" | "preferred" | "required";
  productionPolicy: "standard" | "requireMutationPreview" | "prohibitMutations";
  location:
    | { kind: "network"; endpoints: { host: string; port: number; role: string }[] }
    | { kind: "sqlite"; sqlite: { path: string; accessMode: string } }
    | { kind: "memory"; name: string | null };
}

export interface DatabaseObject {
  kind: string;
  path: string[];
  nativeIdentifier: string | null;
}

export interface DatabaseValue { kind: string; value?: unknown }

export interface DatabasePage {
  columns: { name: string; typeName: string | null }[];
  rows: DatabaseValue[][];
  hasMore: boolean;
  nextOffset: number | null;
  elapsedMillis: number;
}

export interface DatabaseOperation {
  id: string;
  connectionId: string;
  connectionName: string;
  kind: string;
  /** Redacted: the statement, never a parameter value. */
  summary: string;
  outcome: "succeeded" | "failed" | "abandoned";
  affectedRecords: number | null;
  elapsedMillis: number;
  error: string | null;
  at: string;
}
