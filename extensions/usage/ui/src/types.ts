import type { DayPoint, HeatmapCell, HourPoint, NamedAmount } from "@veronica/ui/charts";

export interface Totals {
  cost: number;
  tokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheCreationTokens: number;
  cacheReadTokens: number;
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

export type UsageLevel = "green" | "orange" | "red";

export type PacingZone = "chill" | "onTrack" | "warning" | "hot";

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
