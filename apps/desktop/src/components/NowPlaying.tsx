import { useCallback, useEffect, useState } from "react";

import { ipc } from "../lib/ipc";
import { trackTimeline } from "../lib/format";
import type { MediaAction, NowPlaying as NowPlayingState } from "../lib/types";

/** Poll interval while a surface showing now-playing is visible. */
const POLL_MS = 1000;

/**
 * Live now-playing state.
 *
 * MPRIS has no position-changed signal that players implement reliably, so the
 * position has to be polled. Polling stops as soon as the caller unmounts, which
 * is why the notch only mounts this while it is open.
 */
export function useNowPlaying(active = true) {
  const [playing, setPlaying] = useState<NowPlayingState | null>(null);
  const [unavailable, setUnavailable] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setPlaying(await ipc.mediaNowPlaying());
      setUnavailable(false);
    } catch {
      // No session bus, or no player: neither is worth an error banner.
      setUnavailable(true);
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    let live = true;
    const tick = () => {
      if (live) void refresh();
    };
    tick();
    const timer = setInterval(tick, POLL_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [active, refresh]);

  const control = useCallback(
    async (action: MediaAction) => {
      try {
        await ipc.mediaControl(action);
        // Read straight back so the button reflects the new state at once
        // rather than after the next poll.
        await refresh();
      } catch {
        // A player that refuses a command is not an application error.
      }
    },
    [refresh],
  );

  return { playing, unavailable, control, refresh };
}

/** Transport buttons. Skip buttons disable themselves when the player says so. */
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

/** Progress bar, hidden entirely when the player reports no duration. */
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
