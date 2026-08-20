/** Typed wrappers over the Tauri command layer. */

import { invoke } from "@tauri-apps/api/core";

import type {
  AgendaView,
  ClipRow,
  DesktopNotification,
  Machine,
  MachineReport,
  Dashboard,
  Diagnostics,
  MediaAction,
  NowPlaying,
  SystemSnapshot,
  UsageView,
  VolumeState,
} from "./types";

export const ipc = {
  diagnostics: () => invoke<Diagnostics>("diagnostics"),

  /** `days` of null means the whole history; empty `sources` means all. */
  usageView: (days: number | null, sources: string[]) =>
    invoke<UsageView>("usage_view", { days, sources }),

  usageRefresh: () => invoke<string>("usage_refresh"),

  settingsAll: () => invoke<Record<string, unknown>>("settings_all"),
  settingsSet: (key: string, value: unknown) =>
    invoke<void>("settings_set", { key, value }),

  clipboardList: (query: string) => invoke<ClipRow[]>("clipboard_list", { query }),
  clipboardRemove: (id: number) => invoke<void>("clipboard_remove", { id }),
  clipboardClear: () => invoke<void>("clipboard_clear"),

  machinesProbe: () => invoke<MachineReport[]>("machines_probe"),
  machinesAdd: (target: string, name: string | null, port: number | null) =>
    invoke<Machine>("machines_add", { target, name, port }),
  machinesRemove: (id: string) => invoke<void>("machines_remove", { id }),
  machinesDiscover: () => invoke<string[]>("machines_discover"),

  systemSnapshot: () => invoke<SystemSnapshot>("system_snapshot"),
  microphoneState: () => invoke<VolumeState>("microphone_state"),
  microphoneToggle: () => invoke<VolumeState>("microphone_toggle"),

  /** `withLinks` costs one D-Bus round trip per event; skip it for the notch. */
  calendarAgenda: (days: number, withLinks: boolean) =>
    invoke<AgendaView>("calendar_agenda", { days, withLinks }),

  mediaNowPlaying: () => invoke<NowPlaying | null>("media_now_playing"),
  mediaControl: (action: MediaAction) =>
    invoke<void>("media_control", { action }),

  notificationsList: () => invoke<DesktopNotification[]>("notifications_list"),
  notificationsDismiss: (id: number) =>
    invoke<void>("notifications_dismiss", { id }),
  notificationsClear: () => invoke<void>("notifications_clear"),

  /** `pinned` keeps the island open and gives it focus so Escape works. */
  notchSetExpanded: (expanded: boolean, pinned: boolean) =>
    invoke<void>("notch_set_expanded", { expanded, pinned }),
  showMainWindow: () => invoke<void>("show_main_window"),
  openExternal: (target: string) => invoke<void>("open_external", { target }),
};

export type { AgendaView, Dashboard, Diagnostics, NowPlaying, SystemSnapshot, UsageView, VolumeState };
