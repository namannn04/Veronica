export function trackClock(microseconds: number): string {
  const seconds = Math.max(0, Math.floor(microseconds / 1_000_000));
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

export function trackTimeline(positionUs: number | null, lengthUs: number | null): string {
  if (positionUs === null && lengthUs === null) {
    return "";
  }
  if (lengthUs === null) {
    return positionUs === null ? "" : trackClock(positionUs);
  }
  const position = positionUs === null ? "?" : trackClock(positionUs);
  return `${position} / ${trackClock(lengthUs)}`;
}

export function stopwatch(totalSeconds: number): string {
  const seconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = String(seconds % 60).padStart(2, "0");
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${rest}` : `${minutes}:${rest}`;
}
