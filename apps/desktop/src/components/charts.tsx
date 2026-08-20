/**
 * Charts, hand-rolled in SVG.
 *
 * No charting library: the forms here are simple, and drawing them directly
 * keeps the bundle small enough that the .deb stays a few megabytes and gives
 * exact control over the mark specs - thin marks, 4px rounded data-ends
 * anchored to the baseline, a 2px surface gap between adjacent fills, and
 * recessive grid and axis lines.
 *
 * Every chart carries a hover layer, because an SVG chart in a window is
 * interactive whether or not it was designed to be.
 */

import { useMemo } from "react";

import { dayLabel, dayLabelLong, hourLabel, money, moneyCompact, tokens } from "../lib/format";
import type { DayPoint, HeatmapCell, HourPoint, NamedAmount } from "../lib/types";
import { TipRow, useTooltip } from "./Tooltip";

/** Categorical slot for a series index. Fixed order, never cycled. */
export function seriesColor(index: number): string {
  // Past eight series the caller must fold into "Other" rather than repeat a
  // hue, so the last slot is returned as a deliberate signal.
  const slot = Math.min(index, 7) + 1;
  return `var(--series-${slot})`;
}

/** Resolves an entity name to its colour. */
export type ColorScale = (name: string) => string;

/**
 * Build a colour scale from a stable list of entity names.
 *
 * Colour follows the entity, never its rank: the list must come from something
 * that does not change with the current filter or sort, or narrowing the view
 * would repaint every surviving series. Anything not in the list gets the
 * neutral axis colour rather than a recycled hue.
 */
export function colorScale(stableOrder: string[]): ColorScale {
  const slots = new Map<string, string>();
  stableOrder.forEach((name, index) => slots.set(name, seriesColor(index)));
  return (name: string) => slots.get(name) ?? "var(--axis)";
}

const MAX_SERIES = 8;

/** Nice round axis maximum, so ticks land on readable values. */
function niceMax(value: number): number {
  if (value <= 0) return 1;
  const magnitude = 10 ** Math.floor(Math.log10(value));
  const normalised = value / magnitude;
  const step = normalised <= 1 ? 1 : normalised <= 2 ? 2 : normalised <= 5 ? 5 : 10;
  return step * magnitude;
}

/** Rounded-top bar anchored to the baseline. */
function barPath(x: number, y: number, w: number, h: number, r = 4): string {
  if (h <= 0.5) return "";
  const radius = Math.min(r, w / 2, h);
  return [
    `M${x},${y + h}`,
    `L${x},${y + radius}`,
    `Q${x},${y} ${x + radius},${y}`,
    `L${x + w - radius},${y}`,
    `Q${x + w},${y} ${x + w},${y + radius}`,
    `L${x + w},${y + h}`,
    "Z",
  ].join(" ");
}

// ------------------------------------------------------------------ day bars

export function DayBars({ days }: { days: DayPoint[] }) {
  const tip = useTooltip();
  const width = 100;
  const height = 34;
  const padLeft = 9;
  const padBottom = 5;
  const padTop = 2;

  // The axis maximum is rounded up to a readable value; the peak is the real
  // busiest day. Reporting the axis max as the peak overstates it.
  const peak = useMemo(() => Math.max(...days.map((d) => d.cost), 0), [days]);
  const max = useMemo(() => niceMax(peak), [peak]);

  if (days.length === 0) {
    return <p className="card-note">No days in this window.</p>;
  }

  const plotW = width - padLeft;
  const plotH = height - padBottom - padTop;
  // A 2px surface gap between adjacent bars, expressed in the viewBox scale.
  const slot = plotW / days.length;
  const gap = Math.min(slot * 0.28, 0.5);
  const barW = Math.max(slot - gap, 0.35);

  const ticks = [0, max / 2, max];
  // Only label a subset of days; a label on every bar is unreadable.
  const labelEvery = Math.max(1, Math.ceil(days.length / 8));

  return (
    <>
      <svg
        className="chart"
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        style={{ height: 168 }}
        role="img"
        aria-label={`Daily spend across ${days.length} days, peaking at ${money(peak)}`}
      >
        {ticks.map((tick) => {
          const y = padTop + plotH - (tick / max) * plotH;
          return (
            <g key={tick}>
              <line className="gridline" x1={padLeft} y1={y} x2={width} y2={y} vectorEffect="non-scaling-stroke" />
            </g>
          );
        })}
        {days.map((day, index) => {
          const h = max > 0 ? (day.cost / max) * plotH : 0;
          const x = padLeft + index * slot + gap / 2;
          const y = padTop + plotH - h;
          return (
            <path
              key={day.period}
              className="bar"
              d={barPath(x, y, barW, h, 0.4)}
              fill="var(--series-1)"
              onMouseMove={(e) =>
                tip.show(
                  e,
                  <>
                    <div className="t-title">{dayLabelLong(day.period)}</div>
                    <TipRow label="Spend" value={money(day.cost)} />
                    <TipRow label="Tokens" value={tokens(day.tokens)} />
                  </>,
                )
              }
              onMouseLeave={tip.hide}
            />
          );
        })}
        <line
          className="baseline"
          x1={padLeft}
          y1={padTop + plotH}
          x2={width}
          y2={padTop + plotH}
          vectorEffect="non-scaling-stroke"
        />
      </svg>
      {/* Axis labels live in HTML so they keep a fixed size while the plot
          stretches with preserveAspectRatio="none". */}
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          color: "var(--ink-muted)",
          fontSize: 10,
          marginTop: 2,
        }}
      >
        {days
          .filter((_, i) => i % labelEvery === 0 || i === days.length - 1)
          .map((d) => (
            <span key={d.period}>{dayLabel(d.period)}</span>
          ))}
      </div>
      <div className="card-note" style={{ marginTop: 6 }}>
        Peak day {money(peak)}
      </div>
      {tip.node}
    </>
  );
}

// --------------------------------------------------------------- hourly bars

export function HourBars({ hours }: { hours: HourPoint[] }) {
  const tip = useTooltip();
  const max = useMemo(() => niceMax(Math.max(...hours.map((h) => h.cost), 0)), [hours]);
  const busiest = useMemo(
    () => hours.reduce((best, h) => (h.cost > best.cost ? h : best), hours[0]),
    [hours],
  );

  if (max <= 0) {
    return <p className="card-note">No hourly detail in this window.</p>;
  }

  const width = 100;
  const height = 30;
  const slot = width / 24;
  const barW = slot - 0.45;

  return (
    <>
      <svg
        className="chart"
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        style={{ height: 120 }}
        role="img"
        aria-label={`Spend by hour of day, busiest at ${hourLabel(busiest.hour)}`}
      >
        <line className="gridline" x1={0} y1={height / 2} x2={width} y2={height / 2} vectorEffect="non-scaling-stroke" />
        {hours.map((hour) => {
          const h = (hour.cost / max) * (height - 1);
          return (
            <path
              key={hour.hour}
              className="bar"
              d={barPath(hour.hour * slot + 0.22, height - h, barW, h, 0.35)}
              fill="var(--series-1)"
              onMouseMove={(e) =>
                tip.show(
                  e,
                  <>
                    <div className="t-title">{hourLabel(hour.hour)}</div>
                    <TipRow label="Spend" value={money(hour.cost)} />
                    <TipRow label="Tokens" value={tokens(hour.tokens)} />
                  </>,
                )
              }
              onMouseLeave={tip.hide}
            />
          );
        })}
        <line className="baseline" x1={0} y1={height} x2={width} y2={height} vectorEffect="non-scaling-stroke" />
      </svg>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          color: "var(--ink-muted)",
          fontSize: 10,
          marginTop: 2,
        }}
      >
        {[0, 6, 12, 18, 23].map((h) => (
          <span key={h}>{hourLabel(h)}</span>
        ))}
      </div>
      <div className="card-note" style={{ marginTop: 6 }}>
        Busiest hour {hourLabel(busiest.hour)} · {money(busiest.cost)}
      </div>
      {tip.node}
    </>
  );
}

// ------------------------------------------------------------- ranked bars

/**
 * Horizontal ranked bars with the name and value labelled directly, so identity
 * never depends on colour alone and no legend box is needed.
 */
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
  if (rows.length === 0) return <p className="card-note">Nothing recorded yet.</p>;

  const shown = rows.slice(0, MAX_SERIES);
  const folded = rows.slice(MAX_SERIES);
  const foldedCost = folded.reduce((sum, r) => sum + r.cost, 0);
  const max = explicitMax ?? Math.max(...rows.map((r) => r.cost), 0);

  const entries = [
    ...shown.map((row) => ({ row, color: colorOf(row.name) })),
    // A ninth series is never a new hue; it folds into a neutral "Other".
    ...(folded.length > 0
      ? [
          {
            row: { ...folded[0], name: "other", label: `Other (${folded.length})`, cost: foldedCost },
            color: "var(--axis)",
          },
        ]
      : []),
  ];

  return (
    <>
      <div style={{ display: "grid", gap: 9 }}>
        {entries.map(({ row, color }) => {
          const share = max > 0 ? (row.cost / max) * 100 : 0;
          const name = labelOf ? labelOf(row) : row.label || row.name;
          return (
            <div
              key={row.name}
              onMouseMove={(e) =>
                tip.show(
                  e,
                  <>
                    <div className="t-title">{name}</div>
                    <TipRow label="Spend" value={money(row.cost)} />
                    <TipRow label="Tokens" value={tokens(row.tokens)} />
                  </>,
                )
              }
              onMouseLeave={tip.hide}
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  gap: 10,
                  fontSize: 12,
                  marginBottom: 3,
                }}
              >
                <span className="cell-name truncate">
                  <span className="swatch" style={{ background: color }} />
                  {name}
                </span>
                <span style={{ fontVariantNumeric: "tabular-nums", color: "var(--ink-secondary)" }}>
                  {money(row.cost)}
                </span>
              </div>
              <div style={{ height: 7, background: "var(--grid)", borderRadius: 4 }}>
                <div
                  style={{
                    width: `${Math.max(share, 1.5)}%`,
                    height: "100%",
                    background: color,
                    borderRadius: 4,
                  }}
                />
              </div>
            </div>
          );
        })}
      </div>
      {tip.node}
    </>
  );
}

// ------------------------------------------------------------------- heatmap

/**
 * GitHub-style spend calendar. Weeks run as columns and weekdays as rows, so a
 * long history stays readable by scrolling sideways.
 *
 * Level 0 is a neutral surface step rather than the bottom of the blue ramp,
 * because an idle day is missing data, not a small amount of spend.
 */
export function SpendCalendar({ cells }: { cells: HeatmapCell[] }) {
  const tip = useTooltip();

  const weeks = useMemo(() => {
    if (cells.length === 0) return [];
    const byDay = new Map(cells.map((c) => [c.period, c]));
    const first = new Date(cells[0].period + "T00:00:00");
    const last = new Date(cells[cells.length - 1].period + "T00:00:00");

    // Start on the Sunday at or before the first day so rows align to weekdays.
    const start = new Date(first);
    start.setDate(start.getDate() - start.getDay());

    const columns: (HeatmapCell | null)[][] = [];
    let column: (HeatmapCell | null)[] = [];
    for (const cursor = new Date(start); cursor <= last; cursor.setDate(cursor.getDate() + 1)) {
      const key = `${cursor.getFullYear()}-${String(cursor.getMonth() + 1).padStart(2, "0")}-${String(
        cursor.getDate(),
      ).padStart(2, "0")}`;
      column.push(byDay.get(key) ?? null);
      if (column.length === 7) {
        columns.push(column);
        column = [];
      }
    }
    if (column.length > 0) {
      while (column.length < 7) column.push(null);
      columns.push(column);
    }
    return columns;
  }, [cells]);

  if (weeks.length === 0) return <p className="card-note">No history yet.</p>;

  const size = 12;
  const gap = 3;
  const labelW = 22;
  const width = labelW + weeks.length * (size + gap);
  const height = 7 * (size + gap) + 14;

  return (
    <>
      <div className="heatmap-scroll">
        <svg
          className="chart"
          width={width}
          height={height}
          role="img"
          aria-label={`Daily spend calendar across ${cells.length} days`}
        >
          {["Mon", "Wed", "Fri"].map((label, i) => (
            <text key={label} x={0} y={(i * 2 + 1) * (size + gap) + 10 + 14}>
              {label}
            </text>
          ))}
          {weeks.map((column, weekIndex) =>
            column.map((cell, dayIndex) => {
              const x = labelW + weekIndex * (size + gap);
              const y = 14 + dayIndex * (size + gap);
              if (!cell) {
                return (
                  <rect
                    key={`${weekIndex}-${dayIndex}`}
                    className="heat-cell"
                    x={x}
                    y={y}
                    width={size}
                    height={size}
                    fill="var(--seq-0)"
                    opacity={0.45}
                  />
                );
              }
              return (
                <rect
                  key={cell.period}
                  className="heat-cell"
                  x={x}
                  y={y}
                  width={size}
                  height={size}
                  fill={`var(--seq-${cell.level})`}
                  onMouseMove={(e) =>
                    tip.show(
                      e,
                      <>
                        <div className="t-title">{dayLabelLong(cell.period)}</div>
                        <TipRow label="Spend" value={money(cell.cost)} />
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
      <div className="heat-legend">
        <span>Quieter</span>
        {[0, 1, 2, 3, 4].map((level) => (
          <span key={level} className="step" style={{ background: `var(--seq-${level})` }} />
        ))}
        <span>Busier</span>
      </div>
      {tip.node}
    </>
  );
}

// --------------------------------------------------------------------- ring

export type RingStatus = "good" | "warning" | "serious" | "critical";

/** Status band for a utilisation percentage, matching Edith's ring cutoffs. */
export function ringStatus(percent: number, warn = 60, critical = 85): RingStatus {
  if (percent >= critical) return "critical";
  if (percent >= warn) return "warning";
  return "good";
}

const STATUS_GLYPH: Record<RingStatus, string> = {
  good: "●",
  warning: "▲",
  serious: "▲",
  critical: "■",
};

const STATUS_WORD: Record<RingStatus, string> = {
  good: "On track",
  warning: "Warming",
  serious: "Ahead of pace",
  critical: "Critical",
};

/**
 * A utilisation gauge.
 *
 * The colour is a reserved status colour, so it always ships with the glyph and
 * word beside it - the state is never carried by hue alone.
 */
export function LimitRing({
  percent,
  title,
  subtitle,
  size = 58,
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
    <div className="ring">
      <svg width={size} height={size} role="img" aria-label={`${title} at ${clamped.toFixed(0)} percent`}>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke="var(--grid)"
          strokeWidth={stroke}
        />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke={`var(--status-${status})`}
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={`${filled} ${circumference - filled}`}
          // Start the arc at twelve o'clock rather than three.
          transform={`rotate(-90 ${size / 2} ${size / 2})`}
        />
        <text
          x="50%"
          y="50%"
          textAnchor="middle"
          dominantBaseline="central"
          style={{ fill: "var(--ink)", fontSize: 14, fontWeight: 650 }}
        >
          {clamped.toFixed(0)}
        </text>
      </svg>
      <div className="ring-meta">
        <div className="ring-title">{title}</div>
        <div className="ring-sub">{subtitle}</div>
        <div className="status-line" style={{ color: `var(--status-${status})` }}>
          <span className="glyph" aria-hidden="true">
            {STATUS_GLYPH[status]}
          </span>
          {STATUS_WORD[status]}
        </div>
      </div>
    </div>
  );
}

export { moneyCompact };
