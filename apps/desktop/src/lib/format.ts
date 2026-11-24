const MONEY = new Intl.NumberFormat(undefined, {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

export function money(value: number): string {
  return MONEY.format(value);
}

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

export function tokens(value: number): string {
  const decimals = (scaled: number) => (scaled >= 100 ? 0 : scaled >= 10 ? 1 : 2);
  const round = (scaled: number, places: number) =>
    Math.round(scaled * 10 ** places) / 10 ** places;
  for (let index = 0; index < UNITS.length; index++) {
    const [scale, suffix] = UNITS[index];
    if (value >= scale) {
      const scaled = value / scale;
      const rounded = round(scaled, decimals(scaled));
      if (rounded >= 1000 && index > 0) return `1.00${UNITS[index - 1][1]}`;
      return `${rounded.toFixed(decimals(rounded))}${suffix}`;
    }
  }
  const whole = Math.round(value);
  return whole >= 1000 ? `1.00${UNITS[UNITS.length - 1][1]}` : String(whole);
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

export function modelLabel(name: string): string {
  const withoutVendor = name.includes("/") ? name.slice(name.lastIndexOf("/") + 1) : name;
  return withoutVendor.replace(/^claude-/, "").replace(/-\d{8}$/, "");
}

export function trackClock(microseconds: number): string {
  const seconds = Math.max(0, Math.floor(microseconds / 1_000_000));
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

export function trackTimeline(positionUs: number | null, lengthUs: number | null): string {
  if (positionUs === null && lengthUs === null) return "";
  if (lengthUs === null) return positionUs === null ? "" : trackClock(positionUs);
  const position = positionUs === null ? "?" : trackClock(positionUs);
  return `${position} / ${trackClock(lengthUs)}`;
}

export function clockTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

export function eventSlot(start: string, end: string, allDay: boolean): string {
  if (allDay) return "All day";
  return `${clockTime(start)} – ${clockTime(end)}`;
}

export function untilLabel(iso: string): string {
  const seconds = Math.floor((new Date(iso).getTime() - Date.now()) / 1000);
  if (Number.isNaN(seconds)) return "";
  if (seconds <= 0) return "now";
  return `in ${countdown(seconds)}`;
}

export function agoFromMillis(millis: number): string {
  const seconds = Math.floor((Date.now() - millis) / 1000);
  if (seconds < 45) return "now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}

export function stopwatch(totalSeconds: number): string {
  const seconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = String(seconds % 60).padStart(2, "0");
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${rest}` : `${minutes}:${rest}`;
}
