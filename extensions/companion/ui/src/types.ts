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
