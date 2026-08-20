import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { colorScale } from "./components/charts";
import {
  TrackProgress,
  TransportControls,
  useNowPlaying,
} from "./components/NowPlaying";
import { ipc } from "./lib/ipc";
import { clockTime, countdown, money, percent, tokens, untilLabel } from "./lib/format";
import type { AgendaView, Dashboard, SystemSnapshot, VolumeState } from "./lib/types";
import "./notch.css";

/**
 * The hover island.
 *
 * Collapsed it shows the clock plus live indicators. Hovering asks the Rust side
 * to grow the window and reveals today's spend, the usage bars, system load and
 * a drop target for staging files.
 *
 * Expansion is debounced on the way out: a pointer crossing the seam between the
 * pill and the expanded body would otherwise collapse and re-expand it.
 */
export function Notch() {
  const [expanded, setExpanded] = useState(false);
  const [board, setBoard] = useState<Dashboard | null>(null);
  const [sourceOrder, setSourceOrder] = useState<string[]>([]);
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [mic, setMic] = useState<VolumeState | null>(null);
  const [clock, setClock] = useState(() => new Date());
  const [files, setFiles] = useState<string[]>([]);
  const [dragOver, setDragOver] = useState(false);
  const collapseTimer = useRef<number | null>(null);
  const { playing, control } = useNowPlaying(true);
  const [agenda, setAgenda] = useState<AgendaView | null>(null);

  useEffect(() => {
    const timer = setInterval(() => setClock(new Date()), 1000);
    return () => clearInterval(timer);
  }, []);

  // Read without join links: the pill only needs times and titles, and looking
  // each event up in Evolution would add a round trip per event.
  useEffect(() => {
    let live = true;
    const load = async () => {
      try {
        const next = await ipc.calendarAgenda(3, false);
        if (live) setAgenda(next);
      } catch {
        // No calendars or no bus: the island simply omits the section.
      }
    };
    void load();
    const timer = setInterval(load, 120_000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, []);

  const loadUsage = useCallback(async () => {
    try {
      // The island only needs recent activity, not the whole history.
      const view = await ipc.usageView(7, []);
      setBoard(view.dashboard);
      // Colour by the document's source order, not by rank in this window: a
      // source with no recent spend drops out of the ranking, and indexing by
      // position would hand its colour to a different source.
      setSourceOrder(view.sources.map((s) => s.id));
    } catch {
      // The island must keep showing the clock even with no usage data.
    }
  }, []);

  useEffect(() => {
    void loadUsage();
    const updated = listen("usage://updated", () => void loadUsage());
    return () => {
      void updated.then((un) => un());
    };
  }, [loadUsage]);

  // Metrics and mic state are only needed while the island is open.
  useEffect(() => {
    if (!expanded) return;
    let live = true;
    const tick = async () => {
      try {
        const next = await ipc.systemSnapshot();
        if (live) setSnapshot(next);
      } catch {
        /* a dropped sample is fine */
      }
    };
    void tick();
    ipc.microphoneState().then(setMic).catch(() => setMic(null));
    const timer = setInterval(tick, 2000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [expanded]);

  const open = () => {
    if (collapseTimer.current !== null) {
      window.clearTimeout(collapseTimer.current);
      collapseTimer.current = null;
    }
    if (!expanded) {
      setExpanded(true);
      void ipc.notchSetExpanded(true);
    }
  };

  const close = () => {
    if (collapseTimer.current !== null) window.clearTimeout(collapseTimer.current);
    collapseTimer.current = window.setTimeout(() => {
      setExpanded(false);
      void ipc.notchSetExpanded(false);
      collapseTimer.current = null;
    }, 260);
  };

  const toggleMic = async () => {
    try {
      setMic(await ipc.microphoneToggle());
    } catch {
      /* leave the previous state visible */
    }
  };

  const sourceColor = useMemo(() => colorScale(sourceOrder), [sourceOrder]);
  // Prefer what is running now over what is next, because that is the more
  // useful thing to glance at.
  const nextEvent = agenda?.happeningNow ?? agenda?.nextUp ?? null;
  const today = board?.days.at(-1);

  // Indicators in priority order, because only two fit beside the clock.
  // A muted microphone leads: it is a state the user needs to notice, unlike a
  // figure they can look up. An imminent event beats a playing track, which
  // beats today's spend.
  const indicators: JSX.Element[] = [];
  if (mic?.muted) {
    indicators.push(
      <span className="glyph muted" key="mic" title="Microphone muted">
        ▲ mic
      </span>,
    );
  }
  if (nextEvent) {
    indicators.push(
      <span
        className="glyph event"
        key="event"
        title={`${nextEvent.summary} ${untilLabel(nextEvent.start)}`}
      >
        ▦ {clockTime(nextEvent.start)}
      </span>,
    );
  }
  if (playing) {
    indicators.push(
      <span
        className="glyph track"
        key="track"
        title={`${playing.title || "Untitled"} — ${playing.artist || playing.identity}`}
      >
        {/* The equaliser only animates while something is actually playing; a
            paused player gets a static glyph. */}
        <span className={`eq${playing.status === "playing" ? " live" : ""}`} aria-hidden="true">
          <span />
          <span />
          <span />
        </span>
        <span className="track-name">{playing.title || playing.identity}</span>
      </span>,
    );
  }
  if (today && today.cost > 0) {
    indicators.push(
      <span className="glyph" key="spend" title="Spent today">
        {money(today.cost)}
      </span>,
    );
  }

  const timeLabel = clock.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });
  const dateLabel = clock.toLocaleDateString(undefined, { month: "short", day: "numeric" });

  return (
    <div className="notch-root" onMouseEnter={open} onMouseLeave={close}>
      <div className={`island${expanded ? " expanded" : ""}`}>
        <div className="pill">
          <span className="clock">
            {dateLabel} {timeLabel}
          </span>
          <span className="sep" />
          {/* The pill is a fixed width, so only the two most useful indicators
              are shown; the rest are visible once the island is open. */}
          {indicators.slice(0, 2)}
        </div>

        {expanded && (
          <div className="island-body">
            {playing && (
              <div className="island-section">
                <div className="island-label">Now playing</div>
                <div className="island-now">
                  <div className="island-now-text">
                    <div className="island-now-title" title={playing.title}>
                      {playing.title || "Untitled"}
                    </div>
                    <div className="island-now-artist">
                      {playing.artist || playing.identity}
                    </div>
                  </div>
                  <TransportControls playing={playing} control={control} compact />
                </div>
                <TrackProgress playing={playing} />
              </div>
            )}

            <div className="island-section">
              <div className="island-label">Agent usage · last 7 days</div>
              {board ? (
                <>
                  <div className="island-stat">
                    <span>Spend</span>
                    <strong>{money(board.totals.cost)}</strong>
                  </div>
                  <div className="island-stat">
                    <span>Tokens</span>
                    <strong>{tokens(board.totals.tokens)}</strong>
                  </div>
                  <div className="island-rings" style={{ marginTop: 8 }}>
                    {board.bySource.slice(0, 3).map((source) => {
                      const share =
                        board.totals.cost > 0 ? (source.cost / board.totals.cost) * 100 : 0;
                      return (
                        <div className="island-ring-row" key={source.name}>
                          <span className="name">{source.label}</span>
                          <span className="track">
                            <span
                              className="fill"
                              style={{
                                width: `${Math.max(share, 2)}%`,
                                background: sourceColor(source.name),
                              }}
                            />
                          </span>
                          <span className="value">{money(source.cost)}</span>
                        </div>
                      );
                    })}
                  </div>
                </>
              ) : (
                <div className="island-stat">
                  <span>No usage collected yet</span>
                </div>
              )}
            </div>

            {snapshot && (
              <div className="island-section">
                <div className="island-label">This computer</div>
                <div className="island-ring-row">
                  <span className="name">CPU</span>
                  <span className="track">
                    <span
                      className="fill"
                      style={{
                        width: `${Math.max(snapshot.cpu.usagePercent, 2)}%`,
                        background:
                          snapshot.cpu.usagePercent >= 85
                            ? "var(--status-critical)"
                            : snapshot.cpu.usagePercent >= 60
                              ? "var(--status-warning)"
                              : "var(--sage)",
                      }}
                    />
                  </span>
                  <span className="value">{percent(snapshot.cpu.usagePercent)}</span>
                </div>
                <div className="island-ring-row" style={{ marginTop: 7 }}>
                  <span className="name">Memory</span>
                  <span className="track">
                    <span
                      className="fill"
                      style={{
                        width: `${Math.max(
                          (snapshot.memory.usedBytes / Math.max(snapshot.memory.totalBytes, 1)) *
                            100,
                          2,
                        )}%`,
                        background: "var(--series-2)",
                      }}
                    />
                  </span>
                  <span className="value">
                    {percent(
                      (snapshot.memory.usedBytes / Math.max(snapshot.memory.totalBytes, 1)) * 100,
                    )}
                  </span>
                </div>
                <div className="island-stat" style={{ marginTop: 7 }}>
                  <span>Up</span>
                  <strong>{countdown(snapshot.uptimeSecs)}</strong>
                </div>
              </div>
            )}

            {agenda && agenda.days.length > 0 && (
              <div className="island-section">
                <div className="island-label">Agenda</div>
                {agenda.days.slice(0, 2).map((day) => (
                  <div key={day.date} style={{ marginBottom: 6 }}>
                    <div className="island-day">{day.label}</div>
                    {day.events.slice(0, 3).map((event) => (
                      <div className="island-event" key={`${event.eventUid}-${event.start}`}>
                        <span className="when">
                          {event.allDay ? "all day" : clockTime(event.start)}
                        </span>
                        <span className="what">{event.summary}</span>
                      </div>
                    ))}
                  </div>
                ))}
              </div>
            )}

            <div className="island-section">
              <div className="island-label">Shelf</div>
              <div
                className={`shelf${dragOver ? " over" : ""}`}
                onDragOver={(event) => {
                  event.preventDefault();
                  setDragOver(true);
                }}
                onDragLeave={() => setDragOver(false)}
                onDrop={(event) => {
                  event.preventDefault();
                  setDragOver(false);
                  const dropped = Array.from(event.dataTransfer.files).map((file) => file.name);
                  setFiles((current) => [...dropped, ...current].slice(0, 5));
                }}
              >
                {files.length === 0 ? "Drop files here to stage them" : `${files.length} staged`}
                {files.length > 0 && (
                  <div className="shelf-files">
                    {files.map((name) => (
                      <div className="shelf-file" key={name}>
                        <span className="nm">{name}</span>
                        <button
                          onClick={() => setFiles((c) => c.filter((f) => f !== name))}
                          aria-label={`Remove ${name}`}
                        >
                          ✕
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>

            <div className="island-actions">
              <button onClick={() => void ipc.showMainWindow()}>Open Veronica</button>
              <button className={mic?.muted ? "on" : ""} onClick={toggleMic}>
                {mic?.muted ? "Unmute mic" : "Mute mic"}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
