import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import {
  DayBars,
  HourBars,
  RankedBars,
  SpendCalendar,
  colorScale,
} from "../components/charts";
import { ProjectList } from "../components/ProjectList";
import { ModelTable } from "../components/ModelTable";
import { ipc } from "../lib/ipc";
import { money, modelLabel, timeAgo, tokens } from "../lib/format";
import type { CollectorEvent, UsageView } from "../lib/types";

const RANGES: { label: string; days: number | null }[] = [
  { label: "7d", days: 7 },
  { label: "30d", days: 30 },
  { label: "90d", days: 90 },
  { label: "All", days: null },
];

export function UsagePage() {
  const [view, setView] = useState<UsageView | null>(null);
  const [days, setDays] = useState<number | null>(30);
  const [sources, setSources] = useState<string[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [progress, setProgress] = useState<string>("");
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setView(await ipc.usageView(days, sources));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [days, sources]);

  useEffect(() => {
    void load();
  }, [load]);

  // The collector streams its phases, and any surface can trigger a refresh, so
  // this listens rather than only reacting to its own button.
  useEffect(() => {
    const phases = listen<CollectorEvent>("usage-progress", (event) => {
      const payload = event.payload;
      if (payload.kind === "phase") setProgress(`${payload.name} — ${payload.detail}`);
      else if (payload.kind === "note") setProgress(payload.message);
      else if (payload.kind === "error") setError(payload.message);
    });
    const updated = listen("usage-updated", () => {
      void load();
    });
    return () => {
      void phases.then((un) => un());
      void updated.then((un) => un());
    };
  }, [load]);

  const refresh = async () => {
    setRefreshing(true);
    setError(null);
    setProgress("starting the collector…");
    try {
      await ipc.usageRefresh();
      await load();
      setProgress("");
    } catch (e) {
      setError(String(e));
      setProgress("");
    } finally {
      setRefreshing(false);
    }
  };

  const toggleSource = (id: string) => {
    setSources((current) => {
      // An empty list means "all", so the first deselection has to become an
      // explicit list of everything that stays.
      const all = view?.sources.map((s) => s.id) ?? [];
      const active = current.length === 0 ? all : current;
      const next = active.includes(id)
        ? active.filter((s) => s !== id)
        : [...active, id];
      // Deselecting the last source would show an empty dashboard with no way
      // back, so treat that as reselecting everything.
      if (next.length === 0) return [];
      return next.length === all.length ? [] : next;
    });
  };

  const board = view?.dashboard ?? null;
  const isActive = useCallback(
    (id: string) => sources.length === 0 || sources.includes(id),
    [sources],
  );

  // Both scales come from document-wide lists, so narrowing the range or
  // deselecting a source never repaints the series that remain.
  const sourceColor = useMemo(
    () => colorScale(view?.sources.map((s) => s.id) ?? []),
    [view?.sources],
  );
  const modelColor = useMemo(() => colorScale(view?.models ?? []), [view?.models]);

  const cacheHitRate = useMemo(() => {
    if (!board) return null;
    const read = board.totals.cacheReadTokens;
    const total = board.totals.tokens;
    return total > 0 ? (read / total) * 100 : null;
  }, [board]);

  if (view && !view.hasData) {
    return (
      <>
        <Head generatedAt={null} refreshing={refreshing} onRefresh={refresh} />
        <div className="empty">
          <h3>No usage collected yet</h3>
          <p>
            Veronica reads your local Claude, Codex, Cursor and Command Code history.
            Nothing is sent anywhere. Run a collection to build the dashboard.
          </p>
          <button className="button primary" onClick={refresh} disabled={refreshing}>
            {refreshing ? "Collecting…" : "Collect usage"}
          </button>
          {progress && <div className="progress-log">{progress}</div>}
          {error && <div className="banner error">{error}</div>}
        </div>
      </>
    );
  }

  return (
    <>
      <Head generatedAt={view?.generatedAt ?? null} refreshing={refreshing} onRefresh={refresh} />

      {error && <div className="banner error">{error}</div>}
      {refreshing && progress && (
        <div className="banner">
          <span className="progress-log">{progress}</span>
        </div>
      )}

      <div className="toolbar">
        <div className="segmented" role="group" aria-label="Time range">
          {RANGES.map((range) => (
            <button
              key={range.label}
              aria-pressed={days === range.days}
              onClick={() => setDays(range.days)}
            >
              {range.label}
            </button>
          ))}
        </div>
        <span style={{ width: 6 }} />
        {view?.sources.map((source) => (
          <button
            key={source.id}
            className="chip"
            aria-pressed={isActive(source.id)}
            onClick={() => toggleSource(source.id)}
            title={`Toggle ${source.label}`}
          >
            <span className="swatch" style={{ background: sourceColor(source.id) }} />
            {source.label}
          </button>
        ))}
      </div>

      {board && (
        <>
          <div className="grid tiles">
            <Tile
              label="Spend"
              value={money(board.totals.cost)}
              note={`${board.activeDays} active ${board.activeDays === 1 ? "day" : "days"}`}
            />
            <Tile
              label="Tokens"
              value={tokens(board.totals.tokens)}
              note={`${tokens(board.totals.outputTokens)} generated`}
            />
            <Tile label="Sessions" value={String(board.sessionCount)} note="chats collected" />
            <Tile
              label="Cache reads"
              value={cacheHitRate === null ? "—" : `${cacheHitRate.toFixed(0)}%`}
              note="of all tokens"
            />
          </div>

          <div className="grid two-col">
            <section className="card">
              <div className="card-head">
                <h2>Spend per day</h2>
                <span className="card-note">
                  {board.days.length} {board.days.length === 1 ? "day" : "days"}
                </span>
              </div>
              <DayBars days={board.days} />
            </section>

            <section className="card">
              <div className="card-head">
                <h2>By source</h2>
              </div>
              <RankedBars rows={board.bySource} colorOf={sourceColor} />
            </section>
          </div>

          <div className="grid two-col" style={{ marginTop: 12 }}>
            <section className="card">
              <div className="card-head">
                <h2>By hour of day</h2>
              </div>
              <HourBars hours={board.byHour} />
            </section>

            <section className="card">
              <div className="card-head">
                <h2>By model</h2>
                <span className="card-note">{board.byModel.length} models</span>
              </div>
              <RankedBars
                rows={board.byModel}
                colorOf={modelColor}
                labelOf={(row) => modelLabel(row.name)}
              />
            </section>
          </div>

          <section className="card" style={{ marginTop: 12 }}>
            <div className="card-head">
              <h2>Activity calendar</h2>
              <span className="card-note">Ranked against your own busiest day</span>
            </div>
            <SpendCalendar cells={board.heatmap} />
          </section>

          <section className="card" style={{ marginTop: 12 }}>
            <div className="card-head">
              <h2>Model detail</h2>
              <span className="card-note">Sortable · every column</span>
            </div>
            <ModelTable rows={board.byModel} colorOf={modelColor} />
          </section>

          <section className="card" style={{ marginTop: 12 }}>
            <div className="card-head">
              <h2>Projects</h2>
              <span className="card-note">{board.projects.length} with spend</span>
            </div>
            <ProjectList projects={board.projects} />
          </section>
        </>
      )}
    </>
  );
}

function Head({
  generatedAt,
  refreshing,
  onRefresh,
}: {
  generatedAt: string | null;
  refreshing: boolean;
  onRefresh: () => void;
}) {
  return (
    <div className="page-head">
      <div>
        <h1>Agent Usage</h1>
        <div className="page-sub">
          {generatedAt
            ? `Collected ${timeAgo(generatedAt)} · stays on this computer`
            : "Nothing collected yet"}
        </div>
      </div>
      <button className="button" onClick={onRefresh} disabled={refreshing}>
        {refreshing ? "Collecting…" : "Refresh"}
      </button>
    </div>
  );
}

function Tile({ label, value, note }: { label: string; value: string; note: string }) {
  return (
    <div className="card">
      <div className="tile-label">{label}</div>
      <div className="tile-value">{value}</div>
      <div className="tile-note">{note}</div>
    </div>
  );
}
