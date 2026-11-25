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
  BackupSummary,
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
  PlannedShortcut,
  PowerStatus,
  PresenterView,
  ShellExtensionStatus,
  ToolInstallOutcome,
  ToolSurvey,
  UpdateInfo,
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

  powerStatus: () => invoke<PowerStatus>("power_status"),
  updateCheck: () => invoke<UpdateInfo>("update_check"),
  updateInstall: (version: string) => invoke<string>("update_install", { version }),
  appRestart: () => invoke<void>("app_restart"),

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

  toolsReadiness: () => invoke<ToolSurvey>("tools_readiness"),
  toolsInstall: (tool: string) => invoke<ToolInstallOutcome>("tools_install", { tool }),

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

export type { Diagnostics };
