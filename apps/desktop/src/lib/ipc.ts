/** Typed wrappers over the Tauri command layer. */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  listen as tauriListen,
  type EventCallback,
  type EventName,
  type UnlistenFn,
} from "@tauri-apps/api/event";

/**
 * Call a command, and fail in words a person can act on.
 *
 * A Rust command already rejects with its own message, written for the
 * screen, and that is passed straight through. What is not fit to show is
 * everything else: with no Tauri bridge on the page - a browser preview, or
 * the dev server opened directly - `invoke` is undefined and every page on
 * screen would otherwise print `TypeError: Cannot read properties of
 * undefined (reading 'invoke')` in its error banner.
 *
 * The rejection stays a string rather than becoming an `Error`, because that
 * is what Tauri itself rejects with and what the pages already render.
 */
function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(command, args).catch((reason: unknown) => {
    throw describe(reason);
  });
}

/** Whether the page has a Tauri bridge at all. */
export function isDesktopApp(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Subscribe to a desktop event when the Tauri bridge exists.
 *
 * Browser previews deliberately render the same React application without a
 * native bridge. Returning a no-op unsubscriber keeps those previews useful
 * and prevents Tauri's event API from throwing before a page can render.
 */
export function listenEvent<T>(
  event: EventName,
  handler: EventCallback<T>,
): Promise<UnlistenFn> {
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
  BackupSummary,
  ImportReport,
  AttentionFocusSession,
  AttentionStatus,
  AttentionOverview,
  AttentionSettings,
  CompanionItem,
  CompanionRecordingStatus,
  PresenterView,
  ClipRow,
  CopyFormatOption,
  CopyResult,
  PickResult,
  SwatchRow,
  GaugeReport,
  DesktopNotification,
  Machine,
  MachineReport,
  MachineDirectory,
  ContainerInfo,
  Dashboard,
  Diagnostics,
  MediaAction,
  NowPlaying,
  SystemSnapshot,
  RunningProcess,
  HerdrBoard,
  ToolSurvey,
  UsageView,
  VolumeState,
  AudioStream,
  BluetoothState,
  CleanerCategory,
  CleanerItem,
  AuditReport,
  QuinjetBoard,
  DatabaseConnection,
  DatabaseObject,
  DatabasePage,
  DatabaseOperation,
  PackageSource,
  PackageUpdate,
  PackageInstalled,
  PackageUpdateResult,
  RemovalPlan,
  CleanerScan,
  CleanReport,
  EmojiGroup,
  EmojiRow,
  EmojiCopyResult,
  LocalTrack,
  PowerStatus,
  UpdateInfo,
} from "./types";

export const ipc = {
  diagnostics: () => invoke<Diagnostics>("diagnostics"),

  /** `days` of null means the whole history; empty `sources` means all. */
  usageView: (days: number | null, sources: string[]) =>
    invoke<UsageView>("usage_view", { days, sources }),

  usageLimits: () => invoke<GaugeReport>("usage_limits"),

  /** Writes branded PNG cards to ~/Pictures and returns their paths. */
  usageExport: (cards: string[], days: number | null) =>
    invoke<string[]>("usage_export", { cards, days }),
  usageRefresh: () => invoke<string>("usage_refresh"),

  settingsAll: () => invoke<Record<string, unknown>>("settings_all"),
  settingsSet: (key: string, value: unknown) =>
    invoke<void>("settings_set", { key, value }),
  backupExport: (path: string | null = null) =>
    invoke<BackupSummary>("backup_export", { path }),
  backupInspect: (path: string) => invoke<BackupSummary>("backup_inspect", { path }),
  backupImport: (path: string, confirm: boolean) =>
    invoke<ImportReport>("backup_import", { path, confirm }),
  companionList: (query = "") => invoke<CompanionItem[]>("companion_list", { query }),
  companionNoteCreate: (title: string, body: string) => invoke<CompanionItem>("companion_note_create", { title, body }),
  companionUpdate: (id: number, title: string, body: string, pinned: boolean) => invoke<CompanionItem>("companion_update", { id, title, body, pinned }),
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
  attentionSettingsSave: (settings: AttentionSettings) => invoke<void>("attention_settings_save", { settings }),
  shellAction: (action: "cleanKeys" | "pickColor" | "showClipboard") =>
    invoke<void>("shell_action", { action }),

  clipboardList: (query: string) => invoke<ClipRow[]>("clipboard_list", { query }),
  clipboardRemove: (id: number) => invoke<void>("clipboard_remove", { id }),
  clipboardClear: () => invoke<void>("clipboard_clear"),

  presenterState: () => invoke<PresenterView>("presenter_state"),
  /** One of enable, disable, start, stop, dismiss, resume. */
  presenterSet: (action: string) => invoke<void>("presenter_set", { action }),

  alertsView: () => invoke<AlertsView>("alerts_view"),
  /** Posts one sample banner without consuming a real alert's edge. */
  alertsTest: () => invoke<string>("alerts_test"),
  /** Forgets the remembered levels, so the next poll starts fresh. */
  alertsReset: () => invoke<void>("alerts_reset"),

  colorFormats: () => invoke<CopyFormatOption[]>("color_formats"),
  colorSwatches: () => invoke<SwatchRow[]>("color_swatches"),
  /** Opens the compositor's eyedropper; resolves once a pixel is clicked. */
  colorPick: () => invoke<PickResult>("color_pick"),
  /** `format` of null uses the configured one. */
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
  machinesContainers: (id: string) =>
    invoke<ContainerInfo[]>("machines_containers", { id }),
  machinesContainerAction: (id: string, engine: string, container: string, action: "start" | "stop" | "restart") =>
    invoke<void>("machines_container_action", { id, engine, container, action }),
  machinesContainerLogs: (id: string, engine: string, container: string, lines = 200) =>
    invoke<string>("machines_container_logs", { id, engine, container, lines }),
  /** Refused for this computer, and never escalated on the far end. */
  machinesPower: (id: string, action: "restart" | "shutdown") =>
    invoke<void>("machines_power", { id, action }),
  machinesWake: (id: string) => invoke<void>("machines_wake", { id }),

  systemSnapshot: () => invoke<SystemSnapshot>("system_snapshot"),
  powerStatus: () => invoke<PowerStatus>("power_status"),
  updateCheck: () => invoke<UpdateInfo>("update_check"),
  systemProcesses: () => invoke<RunningProcess[]>("system_processes"),
  systemQuitProcess: (pid: number) => invoke<void>("system_quit_process", { pid }),
  /** Fetches the pages being audited and nothing else; no result leaves this computer. */
  auditSite: (site: string, limit: number, concurrency: number) =>
    invoke<AuditReport>("audit_site", { site, limit, concurrency }),

  databaseConnections: () => invoke<DatabaseConnection[]>("database_connections"),
  databaseTest: (connection: string) => invoke<unknown>("database_test", { connection }),
  databaseBrowse: (connection: string, path: string[]) =>
    invoke<DatabaseObject[]>("database_browse", { connection, path }),
  /** Reads only; a statement that would write is refused by the adapter. */
  databaseQuery: (connection: string, statement: string, limit: number, offset: number) =>
    invoke<DatabasePage>("database_query", { connection, statement, limit, offset }),
  databaseRead: (connection: string, path: string[], limit: number, offset: number) =>
    invoke<DatabasePage>("database_read", { connection, path, limit, offset }),
  databaseCapabilities: (connection: string) =>
    invoke<unknown>("database_capabilities", { connection }),
  databaseOperations: (limit: number) =>
    invoke<DatabaseOperation[]>("database_operations", { limit }),

  packagesSources: () => invoke<PackageSource[]>("packages_sources"),
  /** Discovery only; installs nothing. */
  packagesUpdates: () => invoke<PackageUpdate[]>("packages_updates"),
  packagesInventory: () => invoke<PackageInstalled[]>("packages_inventory"),
  /** An empty `names` means every update from that source. */
  packagesUpdate: (source: string, names: string[]) =>
    invoke<PackageUpdateResult>("packages_update", { source, names }),
  packagesRemovalPlan: (packageName: string) =>
    invoke<RemovalPlan>("packages_removal_plan", { package: packageName }),

  cleanerCategories: () => invoke<CleanerCategory[]>("cleaner_categories"),
  /** An empty list means every category. Reads only. */
  cleanerScan: (categories: string[]) => invoke<CleanerScan>("cleaner_scan", { categories }),
  /** Moves exactly these items to the Trash — the ones the user was shown. */
  cleanerClean: (items: CleanerItem[]) => invoke<CleanReport>("cleaner_clean", { items }),
  toolsReadiness: () => invoke<ToolSurvey>("tools_readiness"),
  herdrBoard: () => invoke<HerdrBoard>("herdr_board"),
  herdrOpen: (session: string, paneId: string | null = null) =>
    invoke<void>("herdr_open", { session, paneId }),
  quinjetProjects: () => invoke<QuinjetBoard>("quinjet_projects"),
  /** Opens Quinjet's review TUI in the installed terminal. */
  quinjetOpen: (worktree: string) => invoke<void>("quinjet_open", { worktree }),
  microphoneState: () => invoke<VolumeState>("microphone_state"),
  microphoneToggle: () => invoke<VolumeState>("microphone_toggle"),
  audioStreams: () => invoke<AudioStream[]>("audio_streams"),
  audioStreamVolume: (id: number, volume: number) =>
    invoke<void>("audio_stream_volume", { id, volume }),
  audioStreamToggleMute: (id: number) => invoke<boolean>("audio_stream_toggle_mute", { id }),
  bluetoothState: () => invoke<BluetoothState>("bluetooth_state"),

  emojiGroups: () => invoke<EmojiGroup[]>("emoji_groups"),
  /** `group` is a catalogue index; null searches every group. */
  emojiSearch: (query: string, group: number | null, limit: number) =>
    invoke<EmojiRow[]>("emoji_search", { query, group, limit }),
  emojiRecents: (limit: number) => invoke<EmojiRow[]>("emoji_recents", { limit }),
  emojiCopy: (character: string, insert: boolean) =>
    invoke<EmojiCopyResult>("emoji_copy", { character, insert }),

  /** `withLinks` costs one D-Bus round trip per event; skip it for the notch. */
  calendarAgenda: (days: number, withLinks: boolean) =>
    invoke<AgendaView>("calendar_agenda", { days, withLinks }),
  calendarOpen: () => invoke<void>("calendar_open"),

  mediaNowPlaying: () => invoke<NowPlaying | null>("media_now_playing"),
  mediaControl: (action: MediaAction) =>
    invoke<void>("media_control", { action }),
  musicLibrary: () => invoke<LocalTrack[]>("music_library"),

  notificationsList: () => invoke<DesktopNotification[]>("notifications_list"),
  notificationsDismiss: (id: number) =>
    invoke<void>("notifications_dismiss", { id }),
  notificationsClear: () => invoke<void>("notifications_clear"),

  showMainWindow: () => invoke<void>("show_main_window"),
  openExternal: (target: string) => invoke<void>("open_external", { target }),
};

export type { AgendaView, Dashboard, Diagnostics, NowPlaying, SystemSnapshot, UsageView, VolumeState };
