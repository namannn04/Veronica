import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { AudioLines, Music, Radio } from "lucide-react";

import { TrackProgress, Transport, TransportControls, useNowPlaying } from "../components/NowPlaying";
import { stopwatch } from "../lib/format";
import { useLocalPlayer } from "../lib/localPlayer";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  List,
  ListRow,
  Notice,
  Page,
  PageHeader,
  SearchField,
  Section,
  Skeleton,
  SkeletonRows,
  Slider,
  Tabs,
} from "../ui";
import "./MediaPage.css";

type MediaTab = "library" | "players";

const TABS = [
  { id: "library", label: "Library" },
  { id: "players", label: "Other players" },
] as const;

const SUBTITLES: Record<MediaTab, string> = {
  library: "Audio files under ~/Music, played right here",
  players: "Whatever Spotify, a browser or another Linux player is playing",
};

export function MediaPage() {
  const external = useNowPlaying(true);
  const [tab, setTab] = useState<MediaTab>("library");

  return (
    <Page
      header={
        <PageHeader
          title="Music"
          subtitle={SUBTITLES[tab]}
          accessory={<Tabs items={TABS} value={tab} onChange={setTab} label="Music views" />}
        />
      }
    >
      {tab === "library" ? <LibraryView /> : <PlayersView external={external} />}
    </Page>
  );
}

function Artwork({ src, label }: { src: string | null | undefined; label: string }) {
  return (
    <div className="media-art" aria-hidden={src ? undefined : true}>
      {src ? <img src={src} alt={label} /> : <Music />}
    </div>
  );
}

function LibraryView() {
  const { tracks, libraryStatus, index, current, playing, position, duration, play, toggle, next, previous, seek, loadLibrary } = useLocalPlayer();
  const [query, setQuery] = useState("");

  useEffect(() => {
    if (libraryStatus === "idle") loadLibrary();
  }, [libraryStatus, loadLibrary]);

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return tracks
      .map((track, position) => ({ track, position }))
      .filter(({ track }) => !needle || `${track.title} ${track.artist} ${track.album}`.toLowerCase().includes(needle));
  }, [query, tracks]);

  const loading = libraryStatus === "idle" || libraryStatus === "loading";

  if (libraryStatus === "failed") {
    return (
      <Notice tone="critical" title="Could not read ~/Music" actions={<Button size="small" onClick={loadLibrary}>Retry</Button>}>
        The music library did not load. Check that the folder exists and can be read.
      </Notice>
    );
  }

  if (!loading && tracks.length === 0) {
    return (
      <EmptyState icon={<Music />} title="No local tracks">
        Put MP3, FLAC, OGG, Opus, WAV, M4A, AAC or WebM files under ~/Music, then reopen this page.
      </EmptyState>
    );
  }

  return (
    <>
      <Card className="media-now">
        {loading ? (
          <NowPlayingSkeleton />
        ) : (
          <>
            <Artwork src={current?.artPath ? convertFileSrc(current.artPath) : null} label={current ? `${current.album} artwork` : ""} />
            <div className="media-now-body">
              <div className="media-now-titles">
                <p className="media-now-title">{current?.title ?? "Choose a track"}</p>
                <p className="media-now-artist">{current ? `${current.artist} · ${current.album}` : `${tracks.length} tracks ready to play`}</p>
              </div>
              <div className="media-seek">
                <Slider value={position} max={duration || 0} onChange={seek} label="Position" disabled={!current || !duration} />
                <div className="media-seek-times ui-num">
                  <span>{stopwatch(current ? position : 0)}</span>
                  <span>{stopwatch(current ? duration : 0)}</span>
                </div>
              </div>
              <Transport
                playing={playing}
                canPlay={Boolean(current)}
                canGoPrevious={Boolean(current)}
                canGoNext={Boolean(current)}
                onToggle={toggle}
                onPrevious={previous}
                onNext={next}
              />
            </div>
          </>
        )}
      </Card>

      <Section
        title="Tracks"
        description={loading ? "Reading ~/Music…" : `${tracks.length} in your library`}
        actions={<SearchField size="small" className="media-search" value={query} onChange={setQuery} placeholder="Search library" />}
      >
        {loading ? (
          <List label="Tracks">
            <SkeletonRows rows={6} label="Reading ~/Music" />
          </List>
        ) : visible.length === 0 ? (
          <List label="Tracks">
            <EmptyState size="compact" title={`Nothing matches “${query.trim()}”`}>
              Try a title, an artist or an album name.
            </EmptyState>
          </List>
        ) : (
          <List label="Tracks" className="media-library">
            {visible.map(({ track, position: trackIndex }) => {
              const isCurrent = trackIndex === index;
              return (
                <ListRow
                  key={track.id}
                  leading={
                    <span className="media-row-icon" data-current={isCurrent || undefined}>
                      {isCurrent && playing ? <AudioLines /> : <Music />}
                    </span>
                  }
                  title={track.title}
                  subtitle={`${track.artist} · ${track.album}`}
                  meta={isCurrent ? <Badge tone="accent">{playing ? "Playing" : "Paused"}</Badge> : undefined}
                  selected={isCurrent}
                  onClick={() => (isCurrent ? toggle() : play(trackIndex))}
                />
              );
            })}
          </List>
        )}
      </Section>
    </>
  );
}

function NowPlayingSkeleton() {
  return (
    <>
      <Skeleton className="media-art-skeleton" width={112} height={112} radius={8} />
      <div className="media-now-body">
        <div className="media-now-titles">
          <Skeleton width="48%" height={16} />
          <Skeleton width="32%" height={11} />
        </div>
        <Skeleton width="100%" height={4} />
        <Skeleton width={112} height={30} radius={8} />
      </div>
    </>
  );
}

function PlayersView({ external }: { external: ReturnType<typeof useNowPlaying> }) {
  if (!external.loaded) {
    return (
      <Card className="media-now">
        <NowPlayingSkeleton />
      </Card>
    );
  }

  if (external.unavailable) {
    return (
      <Notice tone="critical" title="No desktop media session" actions={<Button size="small" onClick={() => void external.refresh()}>Retry</Button>}>
        Veronica could not reach the session bus, so other players cannot be shown or controlled.
      </Notice>
    );
  }

  if (!external.playing) {
    return (
      <EmptyState icon={<Radio />} title="No player is active">
        Start Spotify, Rhythmbox, VLC or media in a browser, and it appears here.
      </EmptyState>
    );
  }

  const now = external.playing;
  return (
    <Card className="media-now">
      <div className="media-now-identity blur-music">
        <Artwork src={now.artUrl} label={`${now.album || now.title} artwork`} />
      </div>
      <div className="media-now-body">
        <div className="media-now-titles blur-music">
          <p className="media-now-source">{now.identity}</p>
          <p className="media-now-title">{now.title}</p>
          <p className="media-now-artist">{[now.artist, now.album].filter(Boolean).join(" · ")}</p>
        </div>
        <TrackProgress playing={now} />
        <TransportControls playing={now} control={external.control} />
      </div>
    </Card>
  );
}
