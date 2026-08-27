import { TrackProgress, TransportControls, useNowPlaying } from "../components/NowPlaying";

export function MediaPage() {
  const { playing, unavailable, control, refresh } = useNowPlaying(true);

  return <div className="music-page">
    <div className="page-head edith-head"><div><h1>Music</h1><div className="page-sub">Every Linux player, through MPRIS</div></div><div className="head-actions"><button className="icon-button" onClick={() => void refresh()} title="Refresh" aria-label="Refresh players">↻</button>{playing && <button className="button" onClick={() => control("stop")}>Stop</button>}</div></div>

    {unavailable ? <div className="empty music-empty"><div className="empty-glyph">♫</div><h3>No desktop media session</h3><p>Music control needs the graphical session D-Bus. Open Veronica from your Ubuntu desktop session.</p></div> : !playing ? <div className="empty music-empty"><div className="empty-glyph">♫</div><h3>No player is active</h3><p>Start Spotify, Rhythmbox, VLC or media in a browser. Veronica automatically follows the player that is actually playing.</p><div className="supported-players"><span>Spotify</span><span>Rhythmbox</span><span>VLC</span><span>Browsers</span></div></div> : <>
      <section className="music-player blur-music">
        <div className="music-art">{playing.artUrl ? <img src={playing.artUrl} alt="" /> : <span>♪</span>}<i className={playing.status === "playing" ? "playing" : ""} /></div>
        <div className="music-copy"><span className="music-source">{playing.identity}</span><h2 className="now-title" title={playing.title}>{playing.title || "Untitled"}</h2><p className="now-artist">{playing.artist || "Unknown artist"}{playing.album ? <><b> · </b>{playing.album}</> : null}</p><TrackProgress playing={playing} /><div className="music-controls"><TransportControls playing={playing} control={control} /><span className={`pill ${playing.status === "playing" ? "good" : ""}`}>{playing.status || "stopped"}</span></div></div>
      </section>
      <section className="card music-info"><div><span>ACTIVE PLAYER</span><strong>{playing.identity}</strong><small>{playing.player}</small></div><div><span>PLAYBACK</span><strong>{playing.status || "Stopped"}</strong><small>MPRIS session</small></div><div><span>QUEUE CONTROL</span><strong>{playing.canGoPrevious || playing.canGoNext ? "Available" : "Not exposed"}</strong><small>Previous and next</small></div></section>
    </>}
  </div>;
}
