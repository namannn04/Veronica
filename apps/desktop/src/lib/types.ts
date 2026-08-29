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
}

export interface HerdrBoard {
  installed: boolean;
  executable: string | null;
  sessions: HerdrSession[];
  agents: HerdrAgent[];
}

export interface VolumeState {
  volume: number;
  muted: boolean;
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
export interface LocalTrack { id: string; path: string; title: string; artist: string; album: string; artPath: string | null; }
export interface PowerStatus { hasLid: boolean; lidAwakeActive: boolean; preventSleepActive: boolean; }
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
