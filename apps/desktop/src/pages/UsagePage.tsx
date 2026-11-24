import { BarChart3, RefreshCw, Share2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { colorScale, DayBars, HourBars, RankedBars, SpendCalendar } from "../components/charts";
import { LimitRings } from "../components/LimitRings";
import { ModelTable } from "../components/ModelTable";
import { ProjectList } from "../components/ProjectList";
import { modelLabel, money, timeAgo, tokens } from "../lib/format";
import { ipc, listenEvent } from "../lib/ipc";
import type { CollectorEvent, UsageView } from "../lib/types";
import {
  Button,
  Card,
  CardHeader,
  Columns,
  EmptyState,
  Notice,
  Page,
  PageHeader,
  Segmented,
  Skeleton,
  Stat,
  StatGrid,
  Toolbar,
} from "../ui";
import "./UsagePage.css";

type Range = "7" | "30" | "90" | "all";

const RANGES: { value: Range; label: string; days: number | null }[] = [
  { value: "7", label: "7d", days: 7 },
  { value: "30", label: "30d", days: 30 },
  { value: "90", label: "90d", days: 90 },
  { value: "all", label: "All", days: null },
];

const STALE_AFTER_MS = 24 * 60 * 60 * 1000;

type ExportResult = { tone: "good" | "critical"; text: string };

export function UsagePage() {
  const [view, setView] = useState<UsageView | null>(null);
  const [days, setDays] = useState<number | null>(30);
  const [sources, setSources] = useState<string[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [progress, setProgress] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const [exported, setExported] = useState<ExportResult | null>(null);
  const [now, setNow] = useState(() => Date.now());

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

  useEffect(() => {
    const phases = listenEvent<CollectorEvent>("usage-progress", (event) => {
      const payload = event.payload;
      if (payload.kind === "phase") setProgress(`${payload.name}: ${payload.detail}`);
      else if (payload.kind === "note") setProgress(payload.message);
      else if (payload.kind === "error") setError(payload.message);
    });
    const updated = listenEvent("usage-updated", () => {
      void load();
    });
    return () => {
      void phases.then((un) => un());
      void updated.then((un) => un());
    };
  }, [load]);

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  const refresh = async () => {
    setRefreshing(true);
    setError(null);
    setProgress("Starting the collector…");
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

  const share = async () => {
    setExporting(true);
    setExported(null);
    try {
      const files = await ipc.usageExport([], days, sources);
      setExported({
        tone: "good",
        text: `Saved ${files.length} card${files.length === 1 ? "" : "s"} to ~/Pictures`,
      });
    } catch (reason) {
      setExported({ tone: "critical", text: String(reason) });
    } finally {
      setExporting(false);
    }
  };

  const toggleSource = (id: string) => {
    setSources((current) => {
      const all = view?.sources.map((s) => s.id) ?? [];
      const active = current.length === 0 ? all : current;
      const next = active.includes(id) ? active.filter((s) => s !== id) : [...active, id];
      if (next.length === 0) return [];
      return next.length === all.length ? [] : next;
    });
  };

  const board = view?.dashboard ?? null;
  const isActive = useCallback(
    (id: string) => sources.length === 0 || sources.includes(id),
    [sources],
  );
  const sourceColor = useMemo(
    () => colorScale(view?.sources.map((s) => s.id) ?? []),
    [view?.sources],
  );
  const modelColor = useMemo(() => colorScale(view?.models ?? []), [view?.models]);

  const cacheHitRate = useMemo(() => {
    if (!board) return null;
    const total = board.totals.tokens;
    return total > 0 ? (board.totals.cacheReadTokens / total) * 100 : null;
  }, [board]);

  const generatedAt = view?.generatedAt ?? null;
  const stale = generatedAt !== null && now - new Date(generatedAt).getTime() > STALE_AFTER_MS;
  const range = RANGES.find((item) => item.days === days)?.value ?? "all";
  const rangeLabel = days === null ? "All time" : `Last ${days} days`;

  const header = (
    <PageHeader
      title="Agent usage"
      subtitle={
        generatedAt
          ? `Collected ${timeAgo(generatedAt)}, stays on this computer`
          : "Nothing collected yet"
      }
      actions={
        <>
          <Button
            icon={<Share2 />}
            onClick={() => void share()}
            busy={exporting}
            disabled={!generatedAt}
            title="Branded PNGs with no repository, path, chat title or cost on them"
          >
            {exporting ? "Rendering…" : "Share cards"}
          </Button>
          <Button icon={<RefreshCw />} onClick={() => void refresh()} busy={refreshing}>
            {refreshing ? "Collecting…" : "Refresh"}
          </Button>
        </>
      }
    />
  );

  const messages = (
    <>
      {error && (
        <Notice
          tone="critical"
          title="Usage could not be loaded"
          actions={
            <Button size="small" onClick={() => void load()}>
              Retry
            </Button>
          }
        >
          {error}
        </Notice>
      )}
      {refreshing && progress && <Notice title="Collecting usage…">{progress}</Notice>}
      {exported && (
        <Notice
          tone={exported.tone}
          compact
          actions={
            <Button size="small" variant="plain" onClick={() => setExported(null)}>
              Dismiss
            </Button>
          }
        >
          {exported.text}
        </Notice>
      )}
      {stale && generatedAt && !refreshing && (
        <Notice
          tone="warning"
          title="These figures are out of date"
          actions={
            <Button size="small" onClick={() => void refresh()}>
              Refresh
            </Button>
          }
        >
          They were collected {timeAgo(generatedAt)}, so recent use is missing. Choose how often
          Veronica collects in Settings, General.
        </Notice>
      )}
    </>
  );

  if (view && !view.hasData) {
    return (
      <Page header={header}>
        {messages}
        <Card>
          <EmptyState
            icon={<BarChart3 />}
            title="No usage collected yet"
            action={
              <Button variant="primary" onClick={() => void refresh()} busy={refreshing}>
                {refreshing ? "Collecting…" : "Collect usage"}
              </Button>
            }
          >
            Veronica reads your local Claude, Codex, Cursor and Command Code history. Nothing is
            sent anywhere.
          </EmptyState>
        </Card>
      </Page>
    );
  }

  return (
    <Page header={header}>
      {messages}

      {view && (
        <Toolbar
          className="usage-toolbar"
          leading={
            <div className="usage-sources" role="group" aria-label="Sources">
              {view.sources.map((source) => (
                <button
                  key={source.id}
                  type="button"
                  className="usage-source"
                  aria-pressed={isActive(source.id)}
                  onClick={() => toggleSource(source.id)}
                  title={`Toggle ${source.label}`}
                >
                  <span
                    className="chart-swatch"
                    style={{ background: sourceColor(source.id) }}
                    aria-hidden="true"
                  />
                  {source.label}
                </button>
              ))}
            </div>
          }
          trailing={
            <Segmented
              label="Time range"
              size="small"
              options={RANGES}
              value={range}
              onChange={(value) =>
                setDays(RANGES.find((item) => item.value === value)?.days ?? null)
              }
            />
          }
        />
      )}

      <LimitRings />

      {!view && !error && <UsageSkeleton />}

      {board && (
        <div className="usage-sensitive usage-board">
          <StatGrid min={180}>
            <Stat
              label="Estimated API cost"
              value={money(board.totals.cost)}
              note={`${board.activeDays} active ${board.activeDays === 1 ? "day" : "days"}`}
            />
            <Stat
              label="Tokens"
              value={tokens(board.totals.tokens)}
              note={`${tokens(board.totals.outputTokens)} generated`}
            />
            <Stat label="Sessions" value={String(board.sessionCount)} note="Chats collected" />
            <Stat
              label="Cache reads"
              value={cacheHitRate === null ? "None" : cacheHitRate.toFixed(0)}
              unit={cacheHitRate === null ? undefined : "%"}
              note="Of all tokens"
            />
          </StatGrid>

          <Columns ratio="wide-start">
            <Card className="usage-chart-card">
              <CardHeader title="Estimated API cost per day" subtitle={rangeLabel} />
              <DayBars days={board.days} />
            </Card>
            <Card className="usage-chart-card">
              <CardHeader title="By source" subtitle="Estimated cost per tool" />
              <RankedBars rows={board.bySource} colorOf={sourceColor} />
            </Card>
          </Columns>

          <Columns ratio="wide-start">
            <Card className="usage-chart-card">
              <CardHeader title="By hour of day" subtitle="Local time, estimated cost" />
              <HourBars hours={board.byHour} />
            </Card>
            <Card className="usage-chart-card">
              <CardHeader
                title="By model"
                subtitle={`${board.byModel.length} ${board.byModel.length === 1 ? "model" : "models"}`}
              />
              <RankedBars
                rows={board.byModel}
                colorOf={modelColor}
                labelOf={(row) => modelLabel(row.name)}
              />
            </Card>
          </Columns>

          <Card>
            <CardHeader title="Activity calendar" subtitle="Ranked against your own busiest day" />
            <SpendCalendar cells={board.heatmap} />
          </Card>

          <Card>
            <CardHeader title="Model detail" subtitle="Select a column to sort" />
            <ModelTable rows={board.byModel} colorOf={modelColor} />
          </Card>

          <Card>
            <CardHeader title="Projects" subtitle={`${board.projects.length} with spend`} />
            <ProjectList projects={board.projects} />
          </Card>
        </div>
      )}
    </Page>
  );
}

function UsageSkeleton() {
  return (
    <div className="usage-board" role="status" aria-label="Loading usage">
      <StatGrid min={180}>
        {[0, 1, 2, 3].map((index) => (
          <Card key={index} className="usage-skeleton-stat">
            <Skeleton width="55%" height={10} />
            <Skeleton width="70%" height={26} />
            <Skeleton width="40%" height={9} />
          </Card>
        ))}
      </StatGrid>
      <Columns ratio="wide-start">
        <Card className="usage-chart-card">
          <Skeleton width="45%" height={12} />
          <div className="usage-skeleton-bars">
            {Array.from({ length: 18 }, (_, index) => (
              <Skeleton key={index} height={`${30 + ((index * 37) % 60)}%`} radius={3} />
            ))}
          </div>
        </Card>
        <Card className="usage-chart-card">
          <Skeleton width="35%" height={12} />
          {[90, 64, 40, 22].map((width) => (
            <div className="usage-skeleton-ranked" key={width}>
              <Skeleton width="50%" height={10} />
              <Skeleton width={`${width}%`} height={6} radius={3} />
            </div>
          ))}
        </Card>
      </Columns>
    </div>
  );
}
