import type { BackendState, Granted } from "@veronica/extension-api";

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
  wantedBy: string[];
} & ToolReadiness;

export interface ToolSurvey {
  tools: ToolReport[];
  unmet: Record<string, string[] | undefined>;
}

export type ToolInstallOutcome =
  | { outcome: "alreadyInstalled"; path: string; version: string }
  | { outcome: "installed"; path: string; version: string }
  | {
      outcome: "notRun";
      command: string | null;
      instruction: string;
      reason: string;
    };

export interface DesktopSession {
  kind: "wayland" | "x11" | "headless";
  desktop: string;
  isGnome: boolean;
  shellVersion: string | null;
  hasGlobalShortcutsPortal: boolean;
  hasContainerRuntime: boolean;
  hasPipewire: boolean;
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
  legacyToggles: string[];
}

export interface LaunchAtLoginStatus {
  enabled: boolean;
  entryPath: string;
}

export interface ShellExtensionStatus {
  state: "active" | "disabled" | "restartRequired" | "notInstalled" | "error" | "unsupported";
  detail: string;
  canEnable: boolean;
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
  repositoryId: string | null;
  repositoryUrl: string | null;
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
  attachCommand: string;
}

export interface HerdrBoard {
  installed: boolean;
  executable: string | null;
  sessions: HerdrSession[];
  agents: HerdrAgent[];
  error: string | null;
}

export interface VolumeState {
  volume: number;
  muted: boolean;
}

export interface AudioStream {
  id: number;
  application: string;
  mediaName: string | null;
  direction: "playback" | "capture";
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
  batteryPercent: number | null;
  rssi: number | null;
  icon: string | null;
}

export interface BluetoothState {
  adapters: BluetoothAdapter[];
  devices: BluetoothDevice[];
  unavailable: string | null;
}

export type PlaybackStatus = "playing" | "paused" | "stopped";

export interface NowPlaying {
  player: string;
  identity: string;
  status: PlaybackStatus | null;
  title: string;
  artist: string;
  album: string;
  artUrl: string | null;
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
  start: string;
  end: string;
  allDay: boolean;
  joinUrl: string | null;
}

export interface AgendaDay {
  date: string;
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
  receivedAt: number;
  desktopEntry: string | null;
}

export type MachineReach = { kind: "local" } | { kind: "ssh"; target: string; port?: number };

export interface Machine {
  id: string;
  name: string;
  reach: MachineReach;
  mac?: string | null;
}
export interface MachineFile {
  name: string;
  path: string;
  kind: "directory" | "file" | "link" | "other";
  sizeBytes: number;
  modifiedUnix: number;
}
export interface MachineDirectory {
  path: string;
  parent: string | null;
  entries: MachineFile[];
}
export interface ContainerInfo {
  id: string;
  name: string;
  image: string;
  status: string;
  state: string;
  engine: "docker" | "podman";
  ports: string;
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
  temperatures: { label: string; celsius: number }[];
  fans: { label: string; rpm: number }[];
  gpus: MachineGpu[];
  processes: { pid: number; name: string; cpuPercent: number; memoryBytes: number }[];
}

export interface MachineGpu {
  name: string;
  utilizationPercent: number;
  memoryUsedBytes: number;
  memoryTotalBytes: number;
  temperatureCelsius: number | null;
}

export interface MachineReport {
  machine: Machine;
  stats: MachineStats | null;
  error: string | null;
}

export interface ClipRow {
  id: number;
  preview: string;
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
  active: boolean;
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
  smartColor: boolean;
  pacingMargin: number;
  thresholds: { warningPercent: number; criticalPercent: number };
}

export interface WatchedWindow {
  percent: number;
  resetsAt: string | null;
  resetsIn: string | null;
  level: UsageLevel;
  zone: PacingZone;
}

export interface AlertsView {
  settings: NotifySettings;
  pollSeconds: number;
  session: WatchedWindow | null;
  week: WatchedWindow | null;
  note: string | null;
  sessionReminderAt: string | null;
  weekReminderAt: string | null;
}

export interface SwatchRow {
  id: number;
  hex: string;
  red: number;
  green: number;
  blue: number;
  profile: "srgb" | "displayP3";
  profileLabel: string;
  pickedAt: string;
  formats: Record<string, string>;
  prefersDarkText: boolean;
}

export interface CopyFormatOption {
  id: string;
  label: string;
}

export interface PickResult {
  swatch: SwatchRow;
  value: string;
  format: string;
  source: string;
  copiedVia: string | null;
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
export interface AttentionCategory {
  id: string;
  name: string;
  color: string;
  applications: string[];
}
export interface AttentionSettings {
  enabled: boolean;
  idleThresholdSeconds: number;
  privacy: "applications" | "detailed";
  categories: AttentionCategory[];
}
export interface AttentionEvent {
  id: string;
  startedAt: string;
  durationSeconds: number;
  application: string;
  title: string | null;
  idle: boolean;
}
export interface AttentionOverview {
  from: string;
  to: string;
  activeSeconds: number;
  idleSeconds: number;
  focusedSeconds: number;
  contextSwitches: number;
  applications: [string, number][];
  categories: [string, number][];
  events: AttentionEvent[];
}
export interface CompanionItem {
  id: number;
  kind: "note" | "voice";
  title: string;
  body: string;
  pinned: boolean;
  createdAt: string;
  updatedAt: string;
  audioPath: string | null;
  durationSeconds: number | null;
}
export interface CompanionRecordingStatus {
  recording: boolean;
  elapsedSeconds: number;
}
export interface LocalTrack {
  id: string;
  path: string;
  title: string;
  artist: string;
  album: string;
  artPath: string | null;
}
export interface PowerStatus {
  hasLid: boolean;
  lidAwakeActive: boolean;
  preventSleepActive: boolean;
  lidAwakeRemainingSecs: number | null;
  preventSleepRemainingSecs: number | null;
}
export interface UpdateInfo {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  releaseUrl: string;
  packageUrl: string | null;
  publishedAt: string | null;
  notes: string;
  installKind: "deb" | "flatpak" | "appimage" | "snap";
  canInstall: boolean;
}

export type UsageLevel = "green" | "orange" | "red";
export type PacingZone = "chill" | "onTrack" | "warning" | "hot";

export interface Gauge {
  provider: string;
  window: string;
  percent: number;
  resetsInSecs: number | null;
  risk: number;
  level: UsageLevel;
  zone: PacingZone;
  paceDelta: number | null;
}

export interface GaugeReport {
  gauges: Gauge[];
  notes: string[];
}

export interface EmojiGroup {
  id: string;
  name: string;
  symbol: string;
}

export interface EmojiRow {
  character: string;
  name: string;
  groupIndex: number;
  supportsSkinTones: boolean;
}

export interface EmojiCopyResult {
  character: string;
  copiedVia: string;
  inserted: boolean;
}

export interface CleanerCategory {
  id: string;
  title: string;
  cost: string;
  family: "cache" | "project";
  onByDefault: boolean;
  paths: string[];
  directoryNames: string[];
}

export interface CleanerItem {
  category: string;
  path: string;
  bytes: number;
}

export interface CleanerScan {
  items: CleanerItem[];
  totals: Record<string, number>;
  totalBytes: number;
}

export interface CleanReport {
  trashed: CleanerItem[];
  failed: [string, string][];
  bytesReclaimed: number;
}

export interface PackageSource {
  source: "apt" | "snap" | "flatpak";
  available: boolean;
  needsRoot: boolean;
}

export interface PackageUpdate {
  source: "apt" | "snap" | "flatpak";
  name: string;
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
  removed: string[];
  nowUnused: string[];
  command: string;
  removesMoreThanAsked: boolean;
}

export interface PackageUpdateResult {
  source: string;
  names: string[];
  command: string;
  succeeded: boolean;
  output: string;
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
  installed: boolean;
  projects: QuinjetProject[];
}

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

export interface DatabaseValue {
  kind: string;
  value?: unknown;
}

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
  summary: string;
  outcome: "succeeded" | "failed" | "abandoned";
  affectedRecords: number | null;
  elapsedMillis: number;
  error: string | null;
  at: string;
}

export type PortOwner = {
  pid: number;
  startedAt: string;
  uid: number;
  name: string;
  canStop: boolean;
};
export type ListeningPort = {
  address: string;
  port: number;
  protocol: "TCP" | "UDP";
  inode: string;
  owners: PortOwner[];
};
export type PortsSnapshot = { ports: ListeningPort[]; warnings: string[] };
export type DockerContainer = {
  id: string;
  name: string;
  image: string;
  state: string;
  status: string;
  ports: string;
};
export type DockerSnapshot = {
  status: "ready" | "missing" | "unavailable" | "remote";
  detail: string;
  endpoint: string | null;
  version: string | null;
  containers: DockerContainer[];
};

export interface ExtensionUiPage {
  id: string;
  title: string;
  entry: string;
  icon?: string;
  nav?: { group: string; order: number };
  hint?: string;
  keywords?: string[];
}

export interface ExtensionPermissions {
  network?: string[];
  filesystem?: { path: string; access: "read" | "readWrite" }[];
  dbus?: { session?: string[]; system?: string[] };
  spawn?: string[];
  shell?: string[];
  host?: Granted["host"];
}

export interface ExtensionRequirements {
  tools?: string[];
  capabilities?: string[];
}

export interface ExtensionManifest {
  id: string;
  title: string;
  summary: string;
  icon?: string;
  category: string;
  labs?: boolean;
  ui?: { root: string; pages: ExtensionUiPage[] };
  permissions: Granted & ExtensionPermissions;
  requires?: ExtensionRequirements;
}

export type ExtensionOrigin = "builtin" | "bundled" | "catalog" | "sideload";

export type ShortcutState = "active" | "unbound" | "invalid" | "conflict" | "unavailable";

export interface PlannedShortcut {
  owner: string;
  origin: ExtensionOrigin | null;
  id: string;
  title: string;
  default: string | null;
  run: string[];
  binding: string | null;
  overridden: boolean;
  state: ShortcutState;
  conflictsWith?: string;
}

export interface InstalledExtension {
  id: string;
  version: string;
  origin: ExtensionOrigin;
  enabled: boolean;
  installedAt: string;
  previous: string | null;
  state: BackendState;
  update: string | null;
  manifest: ExtensionManifest | null;
}

export interface CatalogCategory {
  id: string;
  title: string;
  hidden?: boolean;
}

export interface CatalogOffer {
  id: string;
  title: string;
  summary: string;
  category: string;
  icon?: string;
  labs?: boolean;
  permissions: ExtensionPermissions;
  requires: ExtensionRequirements;
  latest: string;
  installed: string | null;
  available: string | null;
}

export interface CatalogShelf {
  revision: number;
  generatedAt: string;
  stale: boolean;
  provenance: "seed" | "cache" | "network";
  categories: CatalogCategory[];
  profiles: CatalogProfile[];
  extensions: CatalogOffer[];
}

export interface CatalogProfile {
  id: string;
  title: string;
  description: string;
  extensions: string[];
}

export type ExtensionJobKind =
  | "install"
  | "sideload"
  | "update"
  | "enable"
  | "disable"
  | "remove"
  | "purge"
  | "rollback";

export interface ExtensionJob {
  id: number;
  kind: ExtensionJobKind;
  extension: string;
  phase: "queued" | "running" | "succeeded" | "failed";
  message: string | null;
  error: string | null;
  startedAt: string;
  finishedAt: string | null;
}

export interface AutoUpdatePreferences {
  enabled: boolean;
  intervalHours: number;
  disabled: string[];
}
