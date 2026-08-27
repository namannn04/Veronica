import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { LimitRing } from "./charts";
import { ProviderSelector } from "./ProviderSelector";
import { ipc } from "../lib/ipc";
import { countdown } from "../lib/format";
import {
  limitProviderOf,
  storedLimitProvider,
  type LimitProvider,
} from "../lib/preferences";
import type { Gauge, GaugeReport } from "../lib/types";

/**
 * Rate-limit rings.
 *
 * The figures come from the provider, so this is a network read rather than a
 * local file: it refreshes on a slow timer and on demand, never on a tight poll.
 * The countdown ticks locally between reads so it stays honest without asking
 * the provider every second.
 */
const REFRESH_MS = 5 * 60 * 1000;

export function LimitRings() {
  const [report, setReport] = useState<GaugeReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [provider, setProvider] = useState<LimitProvider>("Claude");
  // Ticks once a second so the "resets in" text counts down between reads.
  const [, setTick] = useState(0);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [limits, settings] = await Promise.all([ipc.usageLimits(), ipc.settingsAll()]);
      setReport(limits);
      setProvider(limitProviderOf(settings.limitsProvider));
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
    const refresh = setInterval(load, REFRESH_MS);
    const tick = setInterval(() => setTick((n) => n + 1), 1000);
    return () => {
      clearInterval(refresh);
      clearInterval(tick);
    };
  }, [load]);

  useEffect(() => {
    const changed = listen("settings-updated", () => {
      void ipc.settingsAll().then((settings) => setProvider(limitProviderOf(settings.limitsProvider)));
    });
    return () => { void changed.then((unlisten) => unlisten()); };
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

  return (
    <section className="card" style={{ marginBottom: 12 }}>
      <div className="card-head">
        <div className="limits-title"><ProviderSelector value={provider} onChange={(next) => void selectProvider(next)} /><h2>Rate limits</h2></div>
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span className="card-note">Straight from your provider</span>
          <button className="button" onClick={load} disabled={loading}>
            {loading ? "Reading…" : "Refresh"}
          </button>
        </div>
      </div>

      {error && <div className="banner error">{error}</div>}

      {gauges.length > 0 && (
        <div className="rings">
          {gauges.map((gauge) => (
            <LimitRing
              key={`${gauge.provider}-${gauge.window}`}
              percent={gauge.percent}
              title={`${gauge.provider} · ${gauge.window}`}
              subtitle={subtitle(gauge)}
            />
          ))}
        </div>
      )}

      {!loading && gauges.length === 0 && !error && (
        <p className="card-note">No rate limits available.</p>
      )}

      {report?.notes.filter((note) => note.startsWith(provider)).map((note) => (
        <p className="card-note" key={note} style={{ marginTop: 8 }}>
          {note}
        </p>
      ))}
    </section>
  );
}

/**
 * The line under a ring: when it resets, and how the burn compares with a
 * linear one. The pace is what turns a bare percentage into something
 * actionable — 80% with a week left is fine, 60% with an hour is not.
 */
function subtitle(gauge: Gauge): string {
  const parts: string[] = [];
  if (gauge.resetsInSecs !== null) {
    parts.push(`resets in ${countdown(gauge.resetsInSecs)}`);
  }
  if (gauge.paceDelta !== null) {
    const delta = Math.round(gauge.paceDelta);
    if (delta > 0) parts.push(`${delta} points ahead of pace`);
    else if (delta < 0) parts.push(`${Math.abs(delta)} behind pace`);
    else parts.push("exactly on pace");
  }
  return parts.join(" · ");
}
