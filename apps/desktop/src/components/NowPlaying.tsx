import { Pause, Play, SkipBack, SkipForward } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { trackTimeline } from "../lib/format";
import { ipc } from "../lib/ipc";
import { startVisiblePolling } from "../lib/polling";
import type { MediaAction, NowPlaying as NowPlayingState } from "../lib/types";
import { Badge, IconButton, ProgressBar } from "../ui";
import "./NowPlaying.css";

const POLL_MS = 1000;

export function useNowPlaying(active = true) {
  const [playing, setPlaying] = useState<NowPlayingState | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  const [loaded, setLoaded] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setPlaying(await ipc.mediaNowPlaying());
      setUnavailable(false);
    } catch {
      setUnavailable(true);
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    if (!active) {
      return;
    }
    return startVisiblePolling(refresh, POLL_MS);
  }, [active, refresh]);

  const control = useCallback(
    async (action: MediaAction) => {
      try {
        await ipc.mediaControl(action);
        await refresh();
      } catch {
        return;
      }
    },
    [refresh],
  );

  return { playing, unavailable, loaded, control, refresh };
}

export function Transport({
  playing,
  canPlay = true,
  canGoPrevious,
  canGoNext,
  onToggle,
  onPrevious,
  onNext,
}: {
  playing: boolean;
  canPlay?: boolean;
  canGoPrevious: boolean;
  canGoNext: boolean;
  onToggle: () => void;
  onPrevious: () => void;
  onNext: () => void;
}) {
  return (
    <fieldset className="np-transport" aria-label="Playback">
      <IconButton
        label="Previous track"
        tooltip="Play previous track"
        icon={<SkipBack />}
        disabled={!canGoPrevious}
        onClick={onPrevious}
      />
      <IconButton
        className="np-toggle"
        variant="secondary"
        label={playing ? "Pause" : "Play"}
        tooltip={playing ? "Pause playback" : "Resume playback"}
        icon={playing ? <Pause /> : <Play />}
        disabled={!canPlay}
        onClick={onToggle}
      />
      <IconButton
        label="Next track"
        tooltip="Play next track"
        icon={<SkipForward />}
        disabled={!canGoNext}
        onClick={onNext}
      />
    </fieldset>
  );
}

export function TransportControls({
  playing,
  control,
}: {
  playing: NowPlayingState;
  control: (action: MediaAction) => void;
}) {
  return (
    <Transport
      playing={playing.status === "playing"}
      canGoPrevious={playing.canGoPrevious}
      canGoNext={playing.canGoNext}
      onToggle={() => control("toggle")}
      onPrevious={() => control("previous")}
      onNext={() => control("next")}
    />
  );
}

export function TrackProgress({ playing }: { playing: NowPlayingState }) {
  const { positionUs, lengthUs } = playing;
  const fraction =
    positionUs !== null && lengthUs !== null && lengthUs > 0
      ? Math.min(1, Math.max(0, positionUs / lengthUs))
      : null;
  const timeline = trackTimeline(positionUs, lengthUs);
  const live = fraction === null && lengthUs === null && positionUs !== null;

  if (fraction === null && !timeline) {
    return null;
  }

  return (
    <div className="np-progress">
      {fraction !== null && <ProgressBar value={fraction} size="thin" label="Track position" />}
      <div className="np-progress-times">
        {timeline && <span className="ui-num">{timeline}</span>}
        {live && <Badge tone="accent">Live</Badge>}
      </div>
    </div>
  );
}
