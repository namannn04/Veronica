import { useMemo } from "react";

/**
 * A month grid, matching the shell's own clock dropdown.
 *
 * Weeks start on Sunday and the grid is always six rows, so navigating months
 * does not change the panel's height — a jumping layout is far more noticeable
 * than a trailing week of greyed dates.
 */
export function MonthGrid({
  month,
  today,
  eventDays,
  onMonthChange,
  onSelect,
  selected,
}: {
  /** Any date within the month being shown. */
  month: Date;
  today: Date;
  /** `YYYY-MM-DD` days that have at least one event. */
  eventDays: Set<string>;
  onMonthChange: (delta: number) => void;
  onSelect?: (date: Date) => void;
  selected?: Date | null;
}) {
  const cells = useMemo(() => buildGrid(month), [month]);
  const monthLabel = month.toLocaleDateString(undefined, { month: "long" });
  const inMonth = month.getMonth();

  return (
    <div className="month">
      <div className="month-head">
        <button onClick={() => onMonthChange(-1)} aria-label="Previous month">
          ‹
        </button>
        <span className="month-name">{monthLabel}</span>
        <button onClick={() => onMonthChange(1)} aria-label="Next month">
          ›
        </button>
      </div>
      <div className="month-grid" role="grid">
        {["S", "M", "T", "W", "T", "F", "S"].map((day, index) => (
          <span className="month-dow" key={`${day}-${index}`} aria-hidden="true">
            {day}
          </span>
        ))}
        {cells.map((date) => {
          const key = isoDay(date);
          const isToday = sameDay(date, today);
          const isSelected = selected ? sameDay(date, selected) : false;
          return (
            <button
              key={key}
              className={[
                "month-day",
                date.getMonth() === inMonth ? "" : "outside",
                isToday ? "today" : "",
                isSelected && !isToday ? "selected" : "",
                eventDays.has(key) ? "has-events" : "",
              ]
                .filter(Boolean)
                .join(" ")}
              onClick={() => onSelect?.(date)}
              aria-current={isToday ? "date" : undefined}
            >
              {String(date.getDate()).padStart(2, "0")}
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** Six weeks of dates covering `month`, starting on the Sunday at or before the 1st. */
function buildGrid(month: Date): Date[] {
  const first = new Date(month.getFullYear(), month.getMonth(), 1);
  const start = new Date(first);
  start.setDate(start.getDate() - start.getDay());
  return Array.from({ length: 42 }, (_, index) => {
    const date = new Date(start);
    date.setDate(start.getDate() + index);
    return date;
  });
}

/** Local `YYYY-MM-DD`; `toISOString` would shift the day in most timezones. */
export function isoDay(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

export function sameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}
