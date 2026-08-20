import { TrackProgress, TransportControls, useNowPlaying } from "../components/NowPlaying";

/**
 * Whatever is playing on this machine.
 *
 * Edith controls Spotify and Apple Music through AppleScript. On Linux MPRIS is
 * the standard, so one page controls Spotify, a browser tab, Rhythmbox or VLC
 * without knowing which is running.
 */
export function MediaPage() {
  const { playing, unavailable, control } = useNowPlaying(true);

  if (unavailable) {
    return (
      <>
        <Head />
        <div className="empty">
          <h3>No session bus</h3>
          <p>Media control needs a desktop session with D-Bus running.</p>
        </div>
      </>
    );
  }

  if (!playing) {
    return (
      <>
        <Head />
        <div className="empty">
          <h3>Nothing is playing</h3>
          <p>
            Start any player that speaks MPRIS — Spotify, Rhythmbox, VLC, or a
            browser tab — and it appears here with full transport control.
          </p>
        </div>
      </>
    );
  }

  const status = playing.status ?? "stopped";

  return (
    <>
      <Head />
      <section className="card">
        <div className="now-playing">
          <div className="art" aria-hidden="true">
            {/* The Rust side inlines art as a data URL and returns nothing when
                there is none to show, so an empty tile is never rendered. */}
            {playing.artUrl ? (
              <img src={playing.artUrl} alt="" />
            ) : (
              <span className="art-glyph">♪</span>
            )}
          </div>
          <div className="now-body">
            <div className="now-title" title={playing.title}>
              {playing.title || "Untitled"}
            </div>
            <div className="now-artist">
              {playing.artist || playing.identity}
              {playing.album ? ` · ${playing.album}` : ""}
            </div>
            <TrackProgress playing={playing} />
            <div className="now-foot">
              <span className={`pill ${status === "playing" ? "good" : ""}`}>{status}</span>
              <span className="card-note">{playing.identity}</span>
            </div>
            <TransportControls playing={playing} control={control} />
          </div>
        </div>
      </section>
    </>
  );
}

function Head() {
  return (
    <div className="page-head">
      <div>
        <h1>Media</h1>
        <div className="page-sub">
          Any player that speaks MPRIS, controlled from one place
        </div>
      </div>
    </div>
  );
}
