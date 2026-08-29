/** Typed wrappers over the Tauri command layer. */

import { invoke } from "@tauri-apps/api/core";

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
  UsageView,
  VolumeState,
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

  systemSnapshot: () => invoke<SystemSnapshot>("system_snapshot"),
  powerStatus: () => invoke<PowerStatus>("power_status"),
  updateCheck: () => invoke<UpdateInfo>("update_check"),
  systemProcesses: () => invoke<RunningProcess[]>("system_processes"),
  systemQuitProcess: (pid: number) => invoke<void>("system_quit_process", { pid }),
  herdrBoard: () => invoke<HerdrBoard>("herdr_board"),
  herdrOpen: (session: string, paneId: string | null = null) =>
    invoke<void>("herdr_open", { session, paneId }),
  microphoneState: () => invoke<VolumeState>("microphone_state"),
  microphoneToggle: () => invoke<VolumeState>("microphone_toggle"),

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
