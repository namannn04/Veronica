import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  type EventCallback,
  type EventName,
  listen as tauriListen,
  type UnlistenFn,
} from "@tauri-apps/api/event";

function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(command, args).catch((reason: unknown) => {
    throw describe(reason);
  });
}

export function isDesktopApp(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function listenEvent<T>(event: EventName, handler: EventCallback<T>): Promise<UnlistenFn> {
  if (!isDesktopApp()) return Promise.resolve(() => {});
  return tauriListen<T>(event, handler);
}

function describe(reason: unknown): string {
  if (!isDesktopApp()) {
    return "Veronica is open outside the desktop app, so nothing on this page can load. Launch Veronica from the Ubuntu app grid, or run `vr` for the same data on the command line.";
  }
  if (typeof reason === "string") return reason;
  if (reason instanceof Error) return reason.message;
  return String(reason);
}

import type {
  AgendaView,
  AlertsView,
  AttentionFocusSession,
  AttentionOverview,
  AttentionSettings,
  AttentionStatus,
  AudioStream,
  AuditReport,
  BackupSummary,
  BluetoothState,
  CleanerCategory,
  CleanerItem,
  CleanerScan,
  CleanReport,
  ClipRow,
  CompanionItem,
  CompanionRecordingStatus,
  ContainerInfo,
  CopyFormatOption,
  CopyResult,
  Dashboard,
  DatabaseConnection,
  DatabaseObject,
  DatabaseOperation,
  DatabasePage,
  DesktopNotification,
  Diagnostics,
  EmojiCopyResult,
  EmojiGroup,
  EmojiRow,
  GaugeReport,
  HerdrBoard,
  ImportReport,
  LaunchAtLoginStatus,
  LocalTrack,
  Machine,
  MachineDirectory,
  MachineReport,
  MediaAction,
  NowPlaying,
  PackageInstalled,
  PackageSource,
  PackageUpdate,
  PackageUpdateResult,
  PickResult,
  PowerStatus,
  PresenterView,
  QuinjetBoard,
  RemovalPlan,
  RunningProcess,
  ShellExtensionStatus,
  SwatchRow,
  SystemSnapshot,
  ToolInstallOutcome,
  ToolSurvey,
  UpdateInfo,
  UsageView,
  VolumeState,
} from "./types";

export const ipc = {
  chatJournal: (text: string) =>
    invoke<import("./companionChat").ChatMessage>("chat_journal", { text }),
  chatView: () => invoke<import("./companionChat").ChatView>("chat_view"),
  chatConfigure: (config: import("./companionChat").ModelConfig, memory: string) =>
    invoke<void>("chat_configure", { config, memory }),
  chatEnable: (enabled: boolean) =>
    invoke<import("./companionChat").ChatView>("chat_enable", { enabled }),
  chatSend: (text: string, key: string, includeHistory: boolean, includeMemory: boolean) =>
    invoke<import("./companionChat").ChatMessage>("chat_send", {
      text,
      key,
      includeHistory,
      includeMemory,
    }),
  chatPreview: (text: string, includeHistory: boolean, includeMemory: boolean) =>
    invoke<unknown>("chat_preview", { text, includeHistory, includeMemory }),
  chatModels: (config: import("./companionChat").ModelConfig) =>
    invoke<string[]>("chat_models", { config }),
  chatPull: (config: import("./companionChat").ModelConfig, confirmed: boolean) =>
    invoke<void>("chat_pull", { config, confirmed }),
  chatClear: (memory: boolean) => invoke<void>("chat_clear", { memory }),
  chatExport: () => invoke<string>("chat_export"),
  agentHerdrAttach: (session: string, pane: string, confirmed: boolean) =>
    invoke<import("./agents").AgentView>("agent_herdr_attach", { session, pane, confirmed }),
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
  graftDiscover: (connection: import("./workspace").GraftConnection) =>
    invoke<import("./workspace").GraftDiscovery>("graft_discover", { connection }),
  graftCall: (
    connection: import("./workspace").GraftConnection,
    name: string,
    argumentsValue: Record<string, unknown>,
    confirmed: boolean,
  ) => invoke<unknown>("graft_call", { connection, name, arguments: argumentsValue, confirmed }),
  workspaceDoctor: (request: { root: string; ports: number[]; containers: string[] }) =>
    invoke<import("./workspace").DoctorReport>("workspace_doctor", { request }),
  diagnostics: () => invoke<Diagnostics>("diagnostics"),

  usageView: (days: number | null, sources: string[]) =>
    invoke<UsageView>("usage_view", { days, sources }),

  usageLimits: () => invoke<GaugeReport>("usage_limits"),

  usageExport: (cards: string[], days: number | null, sources: string[] = []) =>
    invoke<string[]>("usage_export", { cards, days, sources }),
  usageRefresh: () => invoke<string>("usage_refresh"),

  settingsAll: () => invoke<Record<string, unknown>>("settings_all"),
  settingsSet: (key: string, value: unknown) => invoke<void>("settings_set", { key, value }),
  launchAtLoginStatus: () => invoke<LaunchAtLoginStatus>("launch_at_login_status"),
  launchAtLoginSet: (enabled: boolean) =>
    invoke<LaunchAtLoginStatus>("launch_at_login_set", { enabled }),
  shellExtensionStatus: () => invoke<ShellExtensionStatus>("shell_extension_status"),
  shellExtensionEnable: () => invoke<ShellExtensionStatus>("shell_extension_enable"),
  backupExport: (path: string | null = null) => invoke<BackupSummary>("backup_export", { path }),
  backupInspect: (path: string) => invoke<BackupSummary>("backup_inspect", { path }),
  backupImport: (path: string, confirm: boolean) =>
    invoke<ImportReport>("backup_import", { path, confirm }),
  companionList: (query = "") => invoke<CompanionItem[]>("companion_list", { query }),
  companionNoteCreate: (title: string, body: string) =>
    invoke<CompanionItem>("companion_note_create", { title, body }),
  companionUpdate: (id: number, title: string, body: string, pinned: boolean) =>
    invoke<CompanionItem>("companion_update", { id, title, body, pinned }),
  companionRemove: (id: number) => invoke<void>("companion_remove", { id }),
  companionRecordingStatus: () => invoke<CompanionRecordingStatus>("companion_recording_status"),
  companionRecordStart: () => invoke<void>("companion_record_start"),
  companionRecordStop: () => invoke<CompanionItem>("companion_record_stop"),
  companionRecordCancel: () => invoke<void>("companion_record_cancel"),
  companionAudio: (id: number) => invoke<string>("companion_audio", { id }),
  attentionStatus: () => invoke<AttentionStatus>("attention_status"),
  attentionStart: (name: string, durationSeconds: number) =>
    invoke<AttentionFocusSession>("attention_start", { name, durationSeconds }),
  attentionStop: () => invoke<AttentionFocusSession>("attention_stop"),
  attentionHistory: () => invoke<AttentionFocusSession[]>("attention_history"),
  attentionOverview: (days: number) => invoke<AttentionOverview>("attention_overview", { days }),
  attentionSettings: () => invoke<AttentionSettings>("attention_settings"),
  attentionSettingsSave: (settings: AttentionSettings) =>
    invoke<void>("attention_settings_save", { settings }),
  shellAction: (action: "cleanKeys" | "pickColor" | "showClipboard") =>
    invoke<void>("shell_action", { action }),

  clipboardList: (query: string) => invoke<ClipRow[]>("clipboard_list", { query }),
  clipboardRemove: (id: number) => invoke<void>("clipboard_remove", { id }),
  clipboardClear: () => invoke<void>("clipboard_clear"),

  presenterState: () => invoke<PresenterView>("presenter_state"),
  presenterSet: (action: string) => invoke<void>("presenter_set", { action }),

  alertsView: () => invoke<AlertsView>("alerts_view"),
  alertsTest: () => invoke<string>("alerts_test"),
  alertsReset: () => invoke<void>("alerts_reset"),

  colorFormats: () => invoke<CopyFormatOption[]>("color_formats"),
  colorSwatches: () => invoke<SwatchRow[]>("color_swatches"),
  colorPick: () => invoke<PickResult>("color_pick"),
  colorCopy: (id: number, format: string | null) =>
    invoke<CopyResult>("color_copy", { id, format }),
  colorForget: (id: number) => invoke<void>("color_forget", { id }),
  colorClear: () => invoke<void>("color_clear"),

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
  auditSite: (site: string, limit: number, concurrency: number) =>
    invoke<AuditReport>("audit_site", { site, limit, concurrency }),

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
  herdrBoard: () => invoke<HerdrBoard>("herdr_board"),
  herdrOpen: (session: string, paneId: string | null = null) =>
    invoke<void>("herdr_open", { session, paneId }),
  quinjetProjects: () => invoke<QuinjetBoard>("quinjet_projects"),
  quinjetOpen: (worktree: string) => invoke<void>("quinjet_open", { worktree }),
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

  calendarAgenda: (days: number, withLinks: boolean) =>
    invoke<AgendaView>("calendar_agenda", { days, withLinks }),
  calendarOpen: () => invoke<void>("calendar_open"),

  mediaNowPlaying: () => invoke<NowPlaying | null>("media_now_playing"),
  mediaControl: (action: MediaAction) => invoke<void>("media_control", { action }),
  musicLibrary: () => invoke<LocalTrack[]>("music_library"),

  notificationsList: () => invoke<DesktopNotification[]>("notifications_list"),
  notificationsDismiss: (id: number) => invoke<void>("notifications_dismiss", { id }),
  notificationsClear: () => invoke<void>("notifications_clear"),

  showMainWindow: () => invoke<void>("show_main_window"),
  openExternal: (target: string) => invoke<void>("open_external", { target }),
};

export type {
  AgendaView,
  Dashboard,
  Diagnostics,
  NowPlaying,
  SystemSnapshot,
  UsageView,
  VolumeState,
};
