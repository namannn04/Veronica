/** Number and date formatting, shared by every surface. */

const MONEY = new Intl.NumberFormat(undefined, {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

export function money(value: number): string {
  return MONEY.format(value);
}

/** Money for an axis tick, where two decimals is noise. */
export function moneyCompact(value: number): string {
  if (value >= 1000) return `$${(value / 1000).toFixed(value >= 10000 ? 0 : 1)}k`;
  if (value >= 10) return `$${value.toFixed(0)}`;
  if (value === 0) return "$0";
  return `$${value.toFixed(2)}`;
}

const UNITS: [number, string][] = [
  [1e12, "T"],
  [1e9, "B"],
  [1e6, "M"],
  [1e3, "K"],
];

/** Token counts, three significant figures, matching the CLI's output. */
export function tokens(value: number): string {
  for (const [scale, suffix] of UNITS) {
    if (value >= scale) {
      const scaled = value / scale;
      if (scaled >= 100) return `${scaled.toFixed(0)}${suffix}`;
      if (scaled >= 10) return `${scaled.toFixed(1)}${suffix}`;
      return `${scaled.toFixed(2)}${suffix}`;
    }
  }
  return String(Math.round(value));
}

export function bytes(value: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = value;
  let i = 0;
  while (v >= 1024 && i + 1 < units.length) {
    v /= 1024;
    i += 1;
  }
  if (i === 0) return `${value} B`;
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

export function percent(value: number, digits = 0): string {
  return `${value.toFixed(digits)}%`;
}

/** A duration as a compact countdown, e.g. "2h 14m". */
export function countdown(seconds: number): string {
  if (seconds <= 0) return "now";
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m`;
  return `${Math.floor(seconds)}s`;
}

/** `YYYY-MM-DD` as a local date, avoiding the UTC shift a bare parse causes. */
export function parseDay(period: string): Date {
  const [y, m, d] = period.split("-").map(Number);
  return new Date(y, (m ?? 1) - 1, d ?? 1);
}

export function dayLabel(period: string): string {
  return parseDay(period).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

export function dayLabelLong(period: string): string {
  return parseDay(period).toLocaleDateString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

export function hourLabel(hour: number): string {
  if (hour === 0) return "12a";
  if (hour === 12) return "12p";
  return hour < 12 ? `${hour}a` : `${hour - 12}p`;
}

export function timeAgo(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "unknown";
  const seconds = Math.floor((Date.now() - then) / 1000);
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}

/** A model id shortened for a table cell, keeping the distinguishing part. */
export function modelLabel(name: string): string {
  const withoutVendor = name.includes("/") ? name.slice(name.lastIndexOf("/") + 1) : name;
  return withoutVendor.replace(/^claude-/, "").replace(/-\d{8}$/, "");
}

/** A track clock from MPRIS microseconds, e.g. "1:42". */
export function trackClock(microseconds: number): string {
  const seconds = Math.max(0, Math.floor(microseconds / 1_000_000));
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

/**
 * Position over length, or just the position when the player reports no
 * duration — which is what a live stream does.
 */
export function trackTimeline(
  positionUs: number | null,
  lengthUs: number | null,
): string {
  if (positionUs === null && lengthUs === null) return "";
  if (lengthUs === null) return positionUs === null ? "" : trackClock(positionUs);
  const position = positionUs === null ? "?" : trackClock(positionUs);
  return `${position} / ${trackClock(lengthUs)}`;
}

/** A wall-clock time from an RFC 3339 timestamp. */
export function clockTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

/** An event's slot: a time range, or "All day". */
export function eventSlot(start: string, end: string, allDay: boolean): string {
  if (allDay) return "All day";
  return `${clockTime(start)} – ${clockTime(end)}`;
}

/** How long until a timestamp, e.g. "in 2h 14m" or "now". */
export function untilLabel(iso: string): string {
  const seconds = Math.floor((new Date(iso).getTime() - Date.now()) / 1000);
  if (Number.isNaN(seconds)) return "";
  if (seconds <= 0) return "now";
  return `in ${countdown(seconds)}`;
}

/** Relative time from a unix-millisecond stamp, e.g. "12h ago". */
export function agoFromMillis(millis: number): string {
  const seconds = Math.floor((Date.now() - millis) / 1000);
  if (seconds < 45) return "now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}
