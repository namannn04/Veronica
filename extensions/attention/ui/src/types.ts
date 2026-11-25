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
