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
