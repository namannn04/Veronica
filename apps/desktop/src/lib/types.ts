import type { DayPoint, HeatmapCell, HourPoint, NamedAmount } from "@veronica/ui/charts";

export type { DayPoint, HeatmapCell, HourPoint, NamedAmount };

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
  bundled: string | null;
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
