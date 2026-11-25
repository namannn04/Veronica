export type PlaybackStatus = "playing" | "paused" | "stopped";

export interface NowPlaying {
  player: string;
  identity: string;
  status: PlaybackStatus | null;
  title: string;
  artist: string;
  album: string;
  artUrl: string | null;
  lengthUs: number | null;
  positionUs: number | null;
  canGoNext: boolean;
  canGoPrevious: boolean;
}

export type MediaAction = "play" | "pause" | "toggle" | "next" | "previous" | "stop";

export interface LocalTrack {
  id: string;
  path: string;
  title: string;
  artist: string;
  album: string;
  artPath: string | null;
}
