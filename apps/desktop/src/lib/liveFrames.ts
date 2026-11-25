import type { Route } from "./navigation";

export const MAX_LIVE_FRAMES = 4;

export interface LiveFrame {
  route: Route;
  serial: number;
  params: unknown;
}

export interface Launch {
  serial: number;
  params: unknown;
}

export function visitFrame(
  frames: readonly LiveFrame[],
  route: Route,
  opened: Launch | null,
): LiveFrame[] {
  const existing = frames.find((frame) => frame.route === route);
  const visited =
    opened === null
      ? (existing ?? { route, serial: 0, params: null })
      : { route, serial: opened.serial, params: opened.params };
  return [visited, ...frames.filter((frame) => frame.route !== route)].slice(0, MAX_LIVE_FRAMES);
}

export function frameKey(frame: LiveFrame): string {
  return `${frame.route}#${frame.serial}`;
}
