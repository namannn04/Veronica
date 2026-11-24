import { useCallback, useEffect, useState } from "react";

import { startVisiblePolling } from "../lib/polling";
import { ipc } from "../lib/ipc";
import { trackTimeline } from "../lib/format";
import type { MediaAction, NowPlaying as NowPlayingState } from "../lib/types";

const POLL_MS = 1000;

export function useNowPlaying(active = true) {
  const [playing, setPlaying] = useState<NowPlayingState | null>(null);
  const [unavailable, setUnavailable] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setPlaying(await ipc.mediaNowPlaying());
      setUnavailable(false);
    } catch {
      setUnavailable(true);
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    return startVisiblePolling(refresh, POLL_MS);
  }, [active, refresh]);

  const control = useCallback(
    async (action: MediaAction) => {
      try {
        await ipc.mediaControl(action);
        await refresh();
      } catch {
      }
    },
    [refresh],
  );

  return { playing, unavailable, control, refresh };
}

export function TransportControls({
  playing,
  control,
  compact,
}: {
  playing: NowPlayingState;
  control: (action: MediaAction) => void;
  compact?: boolean;
}) {
  const isPlaying = playing.status === "playing";
  return (
    <div className={compact ? "transport compact" : "transport"}>
      <button
        onClick={() => control("previous")}
        disabled={!playing.canGoPrevious}
        aria-label="Previous track"
        title="Previous"
      >
        ⏮
      </button>
      <button
        className="primary"
        onClick={() => control("toggle")}
        aria-label={isPlaying ? "Pause" : "Play"}
        title={isPlaying ? "Pause" : "Play"}
      >
        {isPlaying ? "⏸" : "▶"}
      </button>
      <button
        onClick={() => control("next")}
        disabled={!playing.canGoNext}
        aria-label="Next track"
        title="Next"
      >
        ⏭
      </button>
    </div>
  );
}

export function TrackProgress({ playing }: { playing: NowPlayingState }) {
  const { positionUs, lengthUs } = playing;
  const fraction =
    positionUs !== null && lengthUs !== null && lengthUs > 0
      ? Math.min(1, Math.max(0, positionUs / lengthUs))
      : null;
  const timeline = trackTimeline(positionUs, lengthUs);

  return (
    <div className="track-progress">
      {fraction !== null && (
        <div className="track-track">
          <div className="track-fill" style={{ width: `${fraction * 100}%` }} />
        </div>
      )}
      {timeline && <span className="track-time">{timeline}</span>}
      {fraction === null && lengthUs === null && positionUs !== null && (
        <span className="track-live">LIVE</span>
      )}
    </div>
  );
}
