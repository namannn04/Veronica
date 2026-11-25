import { Button, Card, CardHeader, IconButton, Notice, Skeleton, SkeletonText } from "@veronica/ui";
import { LimitRing } from "@veronica/ui/charts";
import { RotateCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { countdown } from "../lib/format";
import { ipc, listenEvent } from "../lib/ipc";
import { startVisiblePolling } from "../lib/polling";
import { type LimitProvider, limitProviderOf, storedLimitProvider } from "../lib/preferences";
import type { Gauge, GaugeReport } from "../lib/types";
import { ProviderSelector } from "./ProviderSelector";
import "./LimitRings.css";

const REFRESH_MS = 5 * 60 * 1000;

export function LimitRings() {
  const [report, setReport] = useState<GaugeReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [provider, setProvider] = useState<LimitProvider>("Claude");
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [, setTick] = useState(0);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [limits, settings] = await Promise.all([ipc.usageLimits(), ipc.settingsAll()]);
      setReport(limits);
      setProvider(limitProviderOf(settings.limitsProvider));
      setLastUpdated(new Date());
      setError(null);
    } catch (e) {
      setError(readableError(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    const stop = startVisiblePolling(load, REFRESH_MS);
    const tick = setInterval(() => {
      if (!document.hidden) {
        setTick((n) => n + 1);
      }
    }, 1000);
    return () => {
      stop();
      clearInterval(tick);
    };
  }, [load]);

  useEffect(() => {
    const changed = listenEvent("settings-updated", () => {
      void ipc
        .settingsAll()
        .then((settings) => setProvider(limitProviderOf(settings.limitsProvider)));
    });
    return () => {
      void changed.then((unlisten) => unlisten());
    };
  }, []);

  const selectProvider = async (next: LimitProvider) => {
    setProvider(next);
    try {
      await ipc.settingsSet("limitsProvider", storedLimitProvider(next));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const gauges = (report?.gauges ?? []).filter((gauge) => gauge.provider === provider);
  const elapsedSecs = lastUpdated ? (Date.now() - lastUpdated.getTime()) / 1000 : 0;
  const reset =
    elapsedSecs >= 30 &&
    gauges.some(
      (gauge) =>
        gauge.resetsInSecs !== null &&
        gauge.resetsInSecs > 0 &&
        gauge.resetsInSecs - elapsedSecs <= 0,
    );
  useEffect(() => {
    if (reset && !loading) {
      void load();
    }
  }, [reset, loading, load]);

  const notes = report?.notes.filter((note) => note.startsWith(provider)) ?? [];
  const firstLoad = loading && !report && !error;

  return (
    <Card className="limits-card">
      <CardHeader
        title="Rate limits"
        subtitle={
          lastUpdated
            ? `Updated ${lastUpdated.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`
            : "Straight from your provider"
        }
        actions={
          <>
            <ProviderSelector
              compact
              value={provider}
              onChange={(next) => void selectProvider(next)}
            />
            <IconButton
              size="small"
              label="Refresh rate limits"
              icon={<RotateCw />}
              onClick={() => void load()}
              disabled={loading}
            />
          </>
        }
      />

      {error && (
        <Notice
          tone="critical"
          compact
          title="Rate limits could not be read"
          actions={
            <Button size="small" onClick={() => void load()} busy={loading}>
              Retry
            </Button>
          }
        >
          {error}
        </Notice>
      )}

      {firstLoad && (
        <div className="limits-grid" role="status" aria-label="Reading rate limits">
          {[0, 1, 2].map((index) => (
            <div className="chart-ring" key={index}>
              <span className="limits-skeleton-ring" aria-hidden="true" />
              <div className="chart-ring-meta limits-skeleton-meta">
                <SkeletonText width="48%" className="chart-ring-title" />
                <SkeletonText width="82%" className="chart-ring-subtitle" />
                <Skeleton width={64} height={20} radius={10} />
              </div>
            </div>
          ))}
        </div>
      )}

      {gauges.length > 0 && (
        <div className="limits-grid">
          {gauges.map((gauge) => (
            <LimitRing
              key={`${gauge.provider}-${gauge.window}`}
              percent={gauge.percent}
              title={gauge.window}
              subtitle={subtitle(gauge, elapsedSecs)}
            />
          ))}
        </div>
      )}

      {!loading && gauges.length === 0 && !error && notes.length === 0 && (
        <p className="limits-empty">No rate limits available for {provider}.</p>
      )}

      {notes.map((note) => (
        <Notice key={note} tone="warning" compact title={`${provider} needs attention`}>
          {providerNote(note)}
        </Notice>
      ))}
    </Card>
  );
}

function readableError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  return message.replace(/^Error:\s*/i, "");
}

function providerNote(note: string): string {
  const detail = note.replace(/^[^:]+:\s*/, "");
  if (/not installed/i.test(detail)) {
    return "Codex could not be located. Install Codex or set VERONICA_CODEX_BIN to its executable.";
  }
  if (/sign in|log in|unauthorized|unauthenticated|401/i.test(detail)) {
    return `Sign in to ${note.split(":", 1)[0]} again, then refresh.`;
  }
  if (/network|dns|connect|sending request/i.test(detail)) {
    return "The provider could not be reached. Check your connection and refresh.";
  }
  return detail;
}

function subtitle(gauge: Gauge, elapsedSecs: number): string {
  const parts: string[] = [];
  if (gauge.resetsInSecs !== null) {
    parts.push(`resets in ${countdown(Math.max(0, gauge.resetsInSecs - elapsedSecs))}`);
  }
  if (gauge.paceDelta !== null) {
    const delta = Math.round(gauge.paceDelta);
    if (delta > 0) {
      parts.push(`${delta} points ahead of pace`);
    } else if (delta < 0) {
      parts.push(`${Math.abs(delta)} behind pace`);
    } else {
      parts.push("exactly on pace");
    }
  }
  return parts.join(" · ");
}
