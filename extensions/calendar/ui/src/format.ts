export function countdown(seconds: number): string {
  if (seconds <= 0) {
    return "now";
  }
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) {
    return `${days}d ${hours}h`;
  }
  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  if (minutes > 0) {
    return `${minutes}m`;
  }
  return `${seconds}s`;
}

export function clockTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return "";
  }
  return date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

export function eventSlot(start: string, end: string, allDay: boolean): string {
  if (allDay) {
    return "All day";
  }
  return `${clockTime(start)} – ${clockTime(end)}`;
}

export function untilLabel(iso: string, now = Date.now()): string {
  const seconds = Math.floor((new Date(iso).getTime() - now) / 1000);
  if (Number.isNaN(seconds)) {
    return "";
  }
  if (seconds <= 0) {
    return "now";
  }
  return `in ${countdown(seconds)}`;
}
