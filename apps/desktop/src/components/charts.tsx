import { useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";

import { Badge } from "../ui";
import { dayLabel, dayLabelLong, hourLabel, money, moneyCompact, tokens } from "../lib/format";
import type { DayPoint, HeatmapCell, HourPoint, NamedAmount } from "../lib/types";
import { TipRow, TipTitle, useTooltip } from "./Tooltip";
import "./charts.css";

export function seriesColor(index: number): string {
  const slot = Math.min(index, 7) + 1;
  return `var(--series-${slot})`;
}

export type ColorScale = (name: string) => string;

export function colorScale(stableOrder: string[]): ColorScale {
  const slots = new Map<string, string>();
  stableOrder.forEach((name, index) => slots.set(name, seriesColor(index)));
  return (name: string) => slots.get(name) ?? "var(--axis)";
}

const MAX_SERIES = 8;
const PLOT = 100;

function niceMax(value: number): number {
  if (value <= 0) return 1;
  const magnitude = 10 ** Math.floor(Math.log10(value));
  const normalised = value / magnitude;
  const step = normalised <= 1 ? 1 : normalised <= 2 ? 2 : normalised <= 5 ? 5 : 10;
  return step * magnitude;
}

function barPath(x: number, y: number, w: number, h: number, rx: number, ry: number): string {
  if (h <= 0.2) return "";
  const radiusX = Math.min(rx, w / 2);
  const radiusY = Math.min(ry, h);
  return [
    `M${x},${y + h}`,
    `L${x},${y + radiusY}`,
    `Q${x},${y} ${x + radiusX},${y}`,
    `L${x + w - radiusX},${y}`,
    `Q${x + w},${y} ${x + w},${y + radiusY}`,
    `L${x + w},${y + h}`,
    "Z",
  ].join(" ");
}

function ColumnPlot({
  max,
  height,
  label,
  children,
  xLabels,
}: {
  max: number;
  height: number;
  label: string;
  children: ReactNode;
  xLabels: string[];
}) {
  const ticks = [max, max / 2, 0];
  return (
    <div className="chart-columns">
      <div className="chart-y" style={{ height }} aria-hidden="true">
        {ticks.map((tick) => (
          <span key={tick}>{moneyCompact(tick)}</span>
        ))}
      </div>
      <svg className="chart-svg" viewBox={`0 0 ${PLOT} ${PLOT}`} preserveAspectRatio="none" style={{ height }} role="img" aria-label={label}>
        {[0, PLOT / 2].map((y) => (
          <line key={y} className="chart-grid" x1={0} y1={y} x2={PLOT} y2={y} vectorEffect="non-scaling-stroke" />
        ))}
        {children}
        <line className="chart-baseline" x1={0} y1={PLOT} x2={PLOT} y2={PLOT} vectorEffect="non-scaling-stroke" />
      </svg>
      <div className="chart-x" aria-hidden="true">
        {xLabels.map((text, index) => (
          <span key={`${text}-${index}`}>{text}</span>
        ))}
      </div>
    </div>
  );
}

export function DayBars({ days }: { days: DayPoint[] }) {
  const tip = useTooltip();
  const peak = useMemo(() => Math.max(...days.map((d) => d.cost), 0), [days]);
  const max = useMemo(() => niceMax(peak), [peak]);

  if (days.length === 0) return <p className="chart-empty">No days in this window.</p>;

  const slot = PLOT / days.length;
  const gap = Math.min(slot * 0.28, 0.5);
  const barW = Math.max(slot - gap, 0.35);
  const labelEvery = Math.max(1, Math.ceil(days.length / 7));
  const xLabels = days.filter((_, i) => i % labelEvery === 0 || i === days.length - 1).map((d) => dayLabel(d.period));

  return (
    <div className="chart">
      <ColumnPlot max={max} height={168} label={`Daily spend across ${days.length} days, peaking at ${money(peak)}`} xLabels={xLabels}>
        {days.map((day, index) => {
          const h = max > 0 ? (day.cost / max) * PLOT : 0;
          return (
            <path
              key={day.period}
              className="chart-bar"
              d={barPath(index * slot + gap / 2, PLOT - h, barW, h, 0.35, 2)}
              fill="var(--series-1)"
              onMouseMove={(e) =>
                tip.show(
                  e,
                  <>
                    <TipTitle>{dayLabelLong(day.period)}</TipTitle>
                    <TipRow label="Spend" value={money(day.cost)} color="var(--series-1)" />
                    <TipRow label="Tokens" value={tokens(day.tokens)} />
                  </>,
                )
              }
              onMouseLeave={tip.hide}
            />
          );
        })}
      </ColumnPlot>
      <p className="chart-caption">
        Peak day <span className="ui-num">{money(peak)}</span>
      </p>
      {tip.node}
    </div>
  );
}

export function HourBars({ hours }: { hours: HourPoint[] }) {
  const tip = useTooltip();
  const max = useMemo(() => niceMax(Math.max(...hours.map((h) => h.cost), 0)), [hours]);
  const busiest = useMemo(() => hours.reduce((best, h) => (h.cost > best.cost ? h : best), hours[0]), [hours]);

  if (!busiest || hours.every((h) => h.cost <= 0)) return <p className="chart-empty">No hourly detail in this window.</p>;

  const slot = PLOT / 24;
  const barW = slot - 0.45;

  return (
    <div className="chart">
      <ColumnPlot max={max} height={132} label={`Spend by hour of day, busiest at ${hourLabel(busiest.hour)}`} xLabels={[0, 6, 12, 18, 23].map(hourLabel)}>
        {hours.map((hour) => {
          const h = (hour.cost / max) * PLOT;
          return (
            <path
              key={hour.hour}
              className="chart-bar"
              d={barPath(hour.hour * slot + 0.22, PLOT - h, barW, h, 0.6, 2.5)}
              fill="var(--series-1)"
              onMouseMove={(e) =>
                tip.show(
                  e,
                  <>
                    <TipTitle>{hourLabel(hour.hour)}</TipTitle>
                    <TipRow label="Spend" value={money(hour.cost)} color="var(--series-1)" />
                    <TipRow label="Tokens" value={tokens(hour.tokens)} />
                  </>,
                )
              }
              onMouseLeave={tip.hide}
            />
          );
        })}
      </ColumnPlot>
      <p className="chart-caption">
        Busiest hour {hourLabel(busiest.hour)}, <span className="ui-num">{money(busiest.cost)}</span>
      </p>
      {tip.node}
    </div>
  );
}

export function RankedBars({
  rows,
  colorOf,
  labelOf,
  max: explicitMax,
}: {
  rows: NamedAmount[];
  colorOf: ColorScale;
  labelOf?: (row: NamedAmount) => string;
  max?: number;
}) {
  const tip = useTooltip();
  if (rows.length === 0) return <p className="chart-empty">Nothing recorded yet.</p>;

  const shown = rows.slice(0, MAX_SERIES);
  const folded = rows.slice(MAX_SERIES);
  const other: NamedAmount = folded.reduce(
    (sum, r) => ({
      ...sum,
      cost: sum.cost + r.cost,
      tokens: sum.tokens + r.tokens,
      inputTokens: sum.inputTokens + r.inputTokens,
      outputTokens: sum.outputTokens + r.outputTokens,
      cacheCreationTokens: sum.cacheCreationTokens + r.cacheCreationTokens,
      cacheReadTokens: sum.cacheReadTokens + r.cacheReadTokens,
    }),
    { name: "other", label: `Other (${folded.length})`, cost: 0, tokens: 0, inputTokens: 0, outputTokens: 0, cacheCreationTokens: 0, cacheReadTokens: 0 },
  );

  const entries = [
    ...shown.map((row) => ({ row, color: colorOf(row.name) })),
    ...(folded.length > 0 ? [{ row: other, color: "var(--axis)" }] : []),
  ];
  const max = explicitMax ?? Math.max(...entries.map(({ row }) => row.cost), 0);

  return (
    <div className="chart-ranked">
      {entries.map(({ row, color }) => {
        const share = max > 0 ? Math.min(100, (row.cost / max) * 100) : 0;
        const name = labelOf ? labelOf(row) : row.label || row.name;
        return (
          <div
            key={row.name}
            className="chart-ranked-row"
            onMouseMove={(e) =>
              tip.show(
                e,
                <>
                  <TipTitle>{name}</TipTitle>
                  <TipRow label="Spend" value={money(row.cost)} color={color} />
                  <TipRow label="Tokens" value={tokens(row.tokens)} />
                </>,
              )
            }
            onMouseLeave={tip.hide}
          >
            <div className="chart-ranked-head">
              <span className="chart-ranked-name">
                <span className="chart-swatch" style={{ background: color }} aria-hidden="true" />
                <span>{name}</span>
              </span>
              <span className="chart-ranked-value ui-num">{money(row.cost)}</span>
            </div>
            <div className="chart-track">
              <div className="chart-fill" style={{ width: `${Math.max(share, 1.5)}%`, background: color }} />
            </div>
          </div>
        );
      })}
      {tip.node}
    </div>
  );
}

const CELL_MIN = 9;
const CELL_MAX = 16;
const CELL_GAP = 3;
const DAY_LABEL_W = 26;
const MONTH_LABEL_H = 16;

export function SpendCalendar({ cells }: { cells: HeatmapCell[] }) {
  const tip = useTooltip();
  const scroller = useRef<HTMLDivElement>(null);
  const [available, setAvailable] = useState(0);

  useLayoutEffect(() => {
    const node = scroller.current;
    if (!node) return;
    const update = () => setAvailable(node.clientWidth);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(node);
    return () => observer.disconnect();
  }, [cells.length > 0]);

  const weeks = useMemo(() => {
    if (cells.length === 0) return [];
    const byDay = new Map(cells.map((c) => [c.period, c]));
    const first = new Date(cells[0].period + "T00:00:00");
    const last = new Date(cells[cells.length - 1].period + "T00:00:00");
    const start = new Date(first);
    start.setDate(start.getDate() - start.getDay());

    const columns: { start: Date; days: (HeatmapCell | null)[] }[] = [];
    let column: (HeatmapCell | null)[] = [];
    let columnStart = new Date(start);
    for (const cursor = new Date(start); cursor <= last; cursor.setDate(cursor.getDate() + 1)) {
      if (column.length === 0) columnStart = new Date(cursor);
      const key = `${cursor.getFullYear()}-${String(cursor.getMonth() + 1).padStart(2, "0")}-${String(cursor.getDate()).padStart(2, "0")}`;
      column.push(byDay.get(key) ?? null);
      if (column.length === 7) {
        columns.push({ start: columnStart, days: column });
        column = [];
      }
    }
    if (column.length > 0) {
      while (column.length < 7) column.push(null);
      columns.push({ start: columnStart, days: column });
    }
    return columns;
  }, [cells]);

  const fitted = weeks.length > 0 && available > 0 ? Math.floor((available - DAY_LABEL_W + CELL_GAP) / weeks.length) - CELL_GAP : CELL_MIN;
  const cellSize = Math.max(CELL_MIN, Math.min(CELL_MAX, fitted));

  useLayoutEffect(() => {
    const node = scroller.current;
    if (node) node.scrollLeft = node.scrollWidth;
  }, [cellSize, weeks.length]);

  if (weeks.length === 0) return <p className="chart-empty">No history yet.</p>;

  const step = cellSize + CELL_GAP;
  const width = DAY_LABEL_W + weeks.length * step - CELL_GAP;
  const height = MONTH_LABEL_H + 7 * step - CELL_GAP;
  const months = weeks
    .map((week, index) => ({ index, month: week.start.getMonth(), date: week.start }))
    .filter((week, index, all) => index === 0 || week.month !== all[index - 1].month)
    .filter((week, index, all) => index !== 0 || all.length < 2 || all[1].index - week.index >= 3);

  return (
    <div className="chart">
      <div className="chart-heatmap-scroll" ref={scroller}>
        <svg className="chart-heatmap" width={width} height={height} role="img" aria-label={`Daily spend calendar across ${cells.length} days`}>
          {months.map((month) => (
            <text key={month.index} x={DAY_LABEL_W + month.index * step} y={10}>
              {month.date.toLocaleDateString(undefined, { month: "short" })}
            </text>
          ))}
          {["Mon", "Wed", "Fri"].map((label, i) => (
            <text key={label} x={0} y={MONTH_LABEL_H + (i * 2 + 1) * step + cellSize - 2}>
              {label}
            </text>
          ))}
          {weeks.map((week, weekIndex) =>
            week.days.map((cell, dayIndex) => {
              const x = DAY_LABEL_W + weekIndex * step;
              const y = MONTH_LABEL_H + dayIndex * step;
              if (!cell) {
                return <rect key={`${weekIndex}-${dayIndex}`} className="chart-heat-cell" x={x} y={y} width={cellSize} height={cellSize} rx={Math.round(cellSize / 4)} fill="var(--seq-0)" opacity={0.45} />;
              }
              return (
                <rect
                  key={cell.period}
                  className="chart-heat-cell"
                  x={x}
                  y={y}
                  width={cellSize}
                  height={cellSize}
                  rx={Math.round(cellSize / 4)}
                  fill={`var(--seq-${cell.level})`}
                  onMouseMove={(e) =>
                    tip.show(
                      e,
                      <>
                        <TipTitle>{dayLabelLong(cell.period)}</TipTitle>
                        <TipRow label="Spend" value={money(cell.cost)} color={`var(--seq-${cell.level})`} />
                        <TipRow label="Tokens" value={tokens(cell.tokens)} />
                      </>,
                    )
                  }
                  onMouseLeave={tip.hide}
                />
              );
            }),
          )}
        </svg>
      </div>
      <div className="chart-heat-legend">
        <span>Quieter</span>
        {[0, 1, 2, 3, 4].map((level) => (
          <span key={level} className="chart-heat-step" style={{ background: `var(--seq-${level})` }} aria-hidden="true" />
        ))}
        <span>Busier</span>
      </div>
      {tip.node}
    </div>
  );
}

export type RingStatus = "good" | "warning" | "serious" | "critical";

export function ringStatus(percent: number, warn = 60, critical = 85): RingStatus {
  if (percent >= critical) return "critical";
  if (percent >= warn) return "warning";
  return "good";
}

const STATUS_WORD: Record<RingStatus, string> = {
  good: "On track",
  warning: "Warming",
  serious: "Ahead of pace",
  critical: "Critical",
};

const STATUS_TONE: Record<RingStatus, "good" | "warning" | "critical"> = {
  good: "good",
  warning: "warning",
  serious: "warning",
  critical: "critical",
};

export function LimitRing({
  percent,
  title,
  subtitle,
  size = 64,
}: {
  percent: number;
  title: string;
  subtitle: string;
  size?: number;
}) {
  const status = ringStatus(percent);
  const stroke = 6;
  const radius = (size - stroke) / 2;
  const circumference = 2 * Math.PI * radius;
  const clamped = Math.max(0, Math.min(100, percent));
  const filled = (clamped / 100) * circumference;

  return (
    <div className="chart-ring">
      <div className="chart-ring-dial" style={{ width: size, height: size }}>
        <svg width={size} height={size} role="img" aria-label={`${title} at ${clamped.toFixed(0)} percent, ${STATUS_WORD[status].toLowerCase()}`}>
          <circle cx={size / 2} cy={size / 2} r={radius} fill="none" stroke="var(--fill-pressed)" strokeWidth={stroke} />
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            stroke={`var(--status-${status})`}
            strokeWidth={stroke}
            strokeLinecap="round"
            strokeDasharray={`${filled} ${circumference - filled}`}
            transform={`rotate(-90 ${size / 2} ${size / 2})`}
          />
        </svg>
        <span className="chart-ring-value ui-num" aria-hidden="true">
          {clamped.toFixed(0)}
          <small>%</small>
        </span>
      </div>
      <div className="chart-ring-meta">
        <div className="chart-ring-title">{title}</div>
        <div className="chart-ring-subtitle">{subtitle}</div>
        <Badge tone={STATUS_TONE[status]} dot>
          {STATUS_WORD[status]}
        </Badge>
      </div>
    </div>
  );
}
