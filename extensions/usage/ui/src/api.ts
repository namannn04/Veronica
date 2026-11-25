import { frameClient } from "@veronica/ui/frame";
import type { AlertsView, GaugeReport, UsageView } from "./types";

const LONG_CALL_MS = 600_000;

const TOPICS: Record<string, string> = {
  "usage-progress": "usage.progress",
  "usage-updated": "usage.updated",
  "usage-refresh-started": "usage.refreshStarted",
  "usage-refresh-finished": "usage.refreshFinished",
  "alerts-posted": "alerts.posted",
  "limits-updated": "usage.limits",
};

function call<T>(method: string, params: unknown = {}, timeoutMs?: number): Promise<T> {
  return frameClient().backend.call<T>(method, params, timeoutMs ? { timeoutMs } : {});
}

export const ipc = {
  usageView: (days: number | null, sources: string[]) => call<UsageView>("view", { days, sources }),
  usageRefresh: () => call<string>("refresh", {}, LONG_CALL_MS),
  usageRefreshing: () => call<boolean>("refreshing"),
  usageExport: (cards: string[], days: number | null, sources: string[]) =>
    call<string[]>("export", { cards, days, sources }, LONG_CALL_MS),
  usageLimits: () => call<GaugeReport>("limits"),
  alertsView: () => call<AlertsView>("alertsView"),
  alertsTest: () => call<string>("alertsTest"),
  alertsReset: () => call<null>("alertsReset"),
  settingsAll: () => call<Record<string, unknown>>("settings"),
  settingsSet: (key: string, value: unknown) => call<null>("configure", { key, value }),
  openExternal: (url: string) => frameClient().ui.openExternal(url),
};

export function listenEvent<T = unknown>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  const topic = TOPICS[event];
  if (topic === undefined) {
    return Promise.resolve(() => undefined);
  }
  return Promise.resolve(
    frameClient().backend.subscribe<T>(topic, (payload) => handler({ payload })),
  );
}

export function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}
