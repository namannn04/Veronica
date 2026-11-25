import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  type EventCallback,
  type EventName,
  listen as tauriListen,
  type UnlistenFn,
} from "@tauri-apps/api/event";

export function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(command, args).catch((reason: unknown) => {
    throw describe(reason);
  });
}

export function isDesktopApp(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function listenEvent<T>(event: EventName, handler: EventCallback<T>): Promise<UnlistenFn> {
  if (!isDesktopApp()) {
    return Promise.resolve(() => {});
  }
  return tauriListen<T>(event, handler);
}

function describe(reason: unknown): string {
  if (!isDesktopApp()) {
    return "Veronica is open outside the desktop app, so nothing on this page can load. Launch Veronica from the Ubuntu app grid, or run `vr` for the same data on the command line.";
  }
  if (typeof reason === "string") {
    return reason;
  }
  if (reason instanceof Error) {
    return reason.message;
  }
  return String(reason);
}

import type {
  AudioStream,
  BackupSummary,
  BluetoothState,
  CleanerCategory,
  CleanerItem,
  CleanerScan,
  CleanReport,
  ContainerInfo,
  DatabaseConnection,
  DatabaseObject,
  DatabaseOperation,
  DatabasePage,
  DesktopNotification,
  Diagnostics,
  EmojiCopyResult,
  EmojiGroup,
  EmojiRow,
  ImportReport,
  InstalledExtension,
  LaunchAtLoginStatus,
  Machine,
  MachineDirectory,
  MachineReport,
  PackageInstalled,
  PackageSource,
  PackageUpdate,
  PackageUpdateResult,
  PlannedShortcut,
  PowerStatus,
  PresenterView,
  RemovalPlan,
  RunningProcess,
  ShellExtensionStatus,
  SystemSnapshot,
  ToolInstallOutcome,
  ToolSurvey,
  UpdateInfo,
  VolumeState,
} from "./types";

export const ipc = {
  agentTracking: (enabled: boolean, confirmed: boolean) =>
    invoke<void>("agent_tracking", { enabled, confirmed }),
  agentProfiles: () => invoke<import("./agents").AgentProfile[]>("agent_profiles"),
  agentProfilesSave: (profiles: import("./agents").AgentProfile[]) =>
    invoke<void>("agent_profiles_save", { profiles }),
  agentList: () => invoke<import("./agents").AgentView[]>("agent_list"),
  agentExternal: () => invoke<import("./agents").AgentView[]>("agent_external"),
  agentStart: (profile: import("./agents").AgentProfile, cwd: string) =>
    invoke<import("./agents").AgentView>("agent_start", { profile, cwd }),
  agentOutput: (id: string, cursor: number) =>
    invoke<import("./agents").TerminalOutput>("agent_output", { id, cursor }),
  agentInput: (id: string, bytes: string) => invoke<void>("agent_input", { id, bytes }),
  agentResize: (id: string, cols: number, rows: number) =>
    invoke<void>("agent_resize", { id, cols, rows }),
  agentStop: (id: string) => invoke<void>("agent_stop", { id }),
  diagnostics: () => invoke<Diagnostics>("diagnostics"),

  settingsAll: () => invoke<Record<string, unknown>>("settings_all"),
  settingsSet: (key: string, value: unknown) => invoke<void>("settings_set", { key, value }),
  shortcutsList: () => invoke<PlannedShortcut[]>("shortcuts_list"),
  launchAtLoginStatus: () => invoke<LaunchAtLoginStatus>("launch_at_login_status"),
  launchAtLoginSet: (enabled: boolean) =>
    invoke<LaunchAtLoginStatus>("launch_at_login_set", { enabled }),
  shellExtensionStatus: () => invoke<ShellExtensionStatus>("shell_extension_status"),
  shellExtensionEnable: () => invoke<ShellExtensionStatus>("shell_extension_enable"),
  backupExport: (path: string | null = null) => invoke<BackupSummary>("backup_export", { path }),
  backupInspect: (path: string) => invoke<BackupSummary>("backup_inspect", { path }),
  backupImport: (path: string, confirm: boolean) =>
    invoke<ImportReport>("backup_import", { path, confirm }),
  shellAction: (action: "cleanKeys" | "pickColor" | "showClipboard") =>
    invoke<void>("shell_action", { action }),

  presenterState: () => invoke<PresenterView>("presenter_state"),
  extList: () => invoke<InstalledExtension[]>("ext_list"),
  extPageOpened: (extension: string, page: string) =>
    invoke<void>("ext_page_opened", { extension, page }),
  extPageClosed: (extension: string, page: string) =>
    invoke<void>("ext_page_closed", { extension, page }),
  extOnboardingDetect: () => invoke<import("./onboarding").Detection>("ext_onboarding_detect"),
  extConfirmAnswer: (request: number, accepted: boolean) =>
    invoke<boolean>("ext_confirm_answer", { request, accepted }),
  presenterSet: (action: string) => invoke<void>("presenter_set", { action }),

  machinesProbe: () => invoke<MachineReport[]>("machines_probe"),
  machinesAdd: (target: string, name: string | null, port: number | null) =>
    invoke<Machine>("machines_add", { target, name, port }),
  machinesRemove: (id: string) => invoke<void>("machines_remove", { id }),
  machinesDiscover: () => invoke<string[]>("machines_discover"),
  machinesTerminal: (id: string) => invoke<void>("machines_terminal", { id }),
  machinesFiles: (id: string, path: string | null = null) =>
    invoke<MachineDirectory>("machines_files", { id, path }),
  machinesFileDownload: (id: string, path: string) =>
    invoke<string>("machines_file_download", { id, path }),
  machinesContainers: (id: string) => invoke<ContainerInfo[]>("machines_containers", { id }),
  machinesContainerAction: (
    id: string,
    engine: string,
    container: string,
    action: "start" | "stop" | "restart",
  ) => invoke<void>("machines_container_action", { id, engine, container, action }),
  machinesContainerLogs: (id: string, engine: string, container: string, lines = 200) =>
    invoke<string>("machines_container_logs", { id, engine, container, lines }),
  machinesPower: (id: string, action: "restart" | "shutdown") =>
    invoke<void>("machines_power", { id, action }),
  machinesWake: (id: string) => invoke<void>("machines_wake", { id }),

  portsSnapshot: () => invoke<import("./types").PortsSnapshot>("ports_snapshot"),
  portsStop: (request: { pid: number; startedAt: string; inode: string }) =>
    invoke<string>("ports_stop", { request }),
  dockerSnapshot: () => invoke<import("./types").DockerSnapshot>("docker_snapshot"),
  dockerAction: (endpoint: string, id: string, action: "start" | "stop" | "restart") =>
    invoke<void>("docker_action", { endpoint, id, action }),
  systemSnapshot: () => invoke<SystemSnapshot>("system_snapshot"),
  powerStatus: () => invoke<PowerStatus>("power_status"),
  updateCheck: () => invoke<UpdateInfo>("update_check"),
  updateInstall: (version: string) => invoke<string>("update_install", { version }),
  appRestart: () => invoke<void>("app_restart"),
  systemProcesses: () => invoke<RunningProcess[]>("system_processes"),
  systemQuitProcess: (pid: number) => invoke<void>("system_quit_process", { pid }),

  databaseConnections: () => invoke<DatabaseConnection[]>("database_connections"),
  databaseTest: (connection: string) => invoke<unknown>("database_test", { connection }),
  databaseBrowse: (connection: string, path: string[]) =>
    invoke<DatabaseObject[]>("database_browse", { connection, path }),
  databaseQuery: (connection: string, statement: string, limit: number, offset: number) =>
    invoke<DatabasePage>("database_query", { connection, statement, limit, offset }),
  databaseRead: (connection: string, path: string[], limit: number, offset: number) =>
    invoke<DatabasePage>("database_read", { connection, path, limit, offset }),
  databaseCapabilities: (connection: string) =>
    invoke<unknown>("database_capabilities", { connection }),
  databaseOperations: (limit: number) =>
    invoke<DatabaseOperation[]>("database_operations", { limit }),

  packagesSources: () => invoke<PackageSource[]>("packages_sources"),
  packagesUpdates: () => invoke<PackageUpdate[]>("packages_updates"),
  packagesInventory: () => invoke<PackageInstalled[]>("packages_inventory"),
  packagesUpdate: (source: string, names: string[]) =>
    invoke<PackageUpdateResult>("packages_update", { source, names }),
  packagesRemovalPlan: (packageName: string) =>
    invoke<RemovalPlan>("packages_removal_plan", { package: packageName }),

  cleanerCategories: () => invoke<CleanerCategory[]>("cleaner_categories"),
  cleanerScan: (categories: string[]) => invoke<CleanerScan>("cleaner_scan", { categories }),
  cleanerClean: (items: CleanerItem[]) => invoke<CleanReport>("cleaner_clean", { items }),
  toolsReadiness: () => invoke<ToolSurvey>("tools_readiness"),
  toolsInstall: (tool: string) => invoke<ToolInstallOutcome>("tools_install", { tool }),
  microphoneState: () => invoke<VolumeState>("microphone_state"),
  microphoneToggle: () => invoke<VolumeState>("microphone_toggle"),
  audioStreams: () => invoke<AudioStream[]>("audio_streams"),
  audioStreamVolume: (id: number, volume: number) =>
    invoke<void>("audio_stream_volume", { id, volume }),
  audioStreamToggleMute: (id: number) => invoke<boolean>("audio_stream_toggle_mute", { id }),
  bluetoothState: () => invoke<BluetoothState>("bluetooth_state"),

  emojiGroups: () => invoke<EmojiGroup[]>("emoji_groups"),
  emojiSearch: (query: string, group: number | null, limit: number) =>
    invoke<EmojiRow[]>("emoji_search", { query, group, limit }),
  emojiRecents: (limit: number) => invoke<EmojiRow[]>("emoji_recents", { limit }),
  emojiCopy: (character: string, insert: boolean) =>
    invoke<EmojiCopyResult>("emoji_copy", { character, insert }),

  notificationsList: () => invoke<DesktopNotification[]>("notifications_list"),
  notificationsDismiss: (id: number) => invoke<void>("notifications_dismiss", { id }),
  notificationsClear: () => invoke<void>("notifications_clear"),

  showMainWindow: () => invoke<void>("show_main_window"),
  openExternal: (target: string) => invoke<void>("open_external", { target }),
};

export type { Diagnostics, SystemSnapshot, VolumeState };
