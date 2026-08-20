import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { MonthGrid, isoDay, sameDay } from "./components/MonthGrid";
import { TrackProgress, TransportControls, useNowPlaying } from "./components/NowPlaying";
import { ipc } from "./lib/ipc";
import {
  agoFromMillis,
  clockTime,
  money,
  percent,
  tokens,
  untilLabel,
} from "./lib/format";
import type {
  AgendaView,
  CalendarEvent,
  Dashboard,
  DesktopNotification,
  SystemSnapshot,
  VolumeState,
} from "./lib/types";
import "./notch.css";

/** Delay before a hover-opened island closes, so crossing the seam is forgiving. */
const CLOSE_DELAY_MS = 260;

/**
 * The notch island.
 *
 * Collapsed it is a pill under the top bar. It opens two ways: hovering peeks,
 * and clicking pins it open until dismissed. Pinning also takes keyboard focus,
 * which is what makes Escape work — an unfocused window never sees the key.
 *
 * Open, it mirrors what the shell's own clock dropdown offers, in one place:
 * now-playing with transport, the notification history, a month grid, and the
 * day's agenda — plus the things the shell has no idea about, like agent spend
 * and machine load.
 */
export function Notch() {
  const [open, setOpen] = useState(false);
  const [pinned, setPinned] = useState(false);
  const [clock, setClock] = useState(() => new Date());
  const [board, setBoard] = useState<Dashboard | null>(null);
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [mic, setMic] = useState<VolumeState | null>(null);
  const [agenda, setAgenda] = useState<AgendaView | null>(null);
  const [notifications, setNotifications] = useState<DesktopNotification[]>([]);
  const [month, setMonth] = useState(() => new Date());
  const [selectedDay, setSelectedDay] = useState<Date | null>(null);
  const [files, setFiles] = useState<string[]>([]);
  const [dragOver, setDragOver] = useState(false);
  const closeTimer = useRef<number | null>(null);
  const { playing, control } = useNowPlaying(true);

  // -- open and close ------------------------------------------------------

  const applyOpen = useCallback(
    (nextOpen: boolean, nextPinned: boolean, remember = false) => {
      setOpen(nextOpen);
      setPinned(nextPinned);
      void ipc.notchSetExpanded(nextOpen, nextPinned);
      // Only a deliberate pin or dismiss is remembered; a hover peek is not,
      // or the island would reopen itself after any stray pointer movement.
      if (remember) void ipc.settingsSet("notchPinned", nextPinned);
    },
    [],
  );

  // Restore the pinned state from the last session.
  useEffect(() => {
    let live = true;
    ipc
      .settingsAll()
      .then((settings) => {
        if (live && settings.notchPinned === true) applyOpen(true, true);
      })
      .catch(() => {
        // Settings are optional; the island simply starts collapsed.
      });
    return () => {
      live = false;
    };
  }, [applyOpen]);

  const cancelClose = useCallback(() => {
    if (closeTimer.current !== null) {
      window.clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
  }, []);

  const peek = useCallback(() => {
    cancelClose();
    if (!open) applyOpen(true, false);
  }, [cancelClose, open, applyOpen]);

  const scheduleClose = useCallback(() => {
    // A pinned island ignores the pointer leaving; only an explicit dismiss
    // closes it.
    if (pinned) return;
    cancelClose();
    closeTimer.current = window.setTimeout(() => {
      applyOpen(false, false);
      closeTimer.current = null;
    }, CLOSE_DELAY_MS);
  }, [pinned, cancelClose, applyOpen]);

  const togglePinned = useCallback(() => {
    cancelClose();
    if (pinned) {
      applyOpen(false, false, true);
    } else {
      applyOpen(true, true, true);
    }
  }, [pinned, cancelClose, applyOpen]);

  const dismiss = useCallback(() => {
    cancelClose();
    applyOpen(false, false, true);
  }, [cancelClose, applyOpen]);

  // Escape closes a pinned island; it only arrives while the window has focus.
  useEffect(() => {
    if (!pinned) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") dismiss();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pinned, dismiss]);

  useEffect(() => cancelClose, [cancelClose]);

  // -- data ---------------------------------------------------------------

  useEffect(() => {
    const timer = setInterval(() => setClock(new Date()), 1000);
    return () => clearInterval(timer);
  }, []);

  const loadUsage = useCallback(async () => {
    try {
      const view = await ipc.usageView(7, []);
      setBoard(view.dashboard);
    } catch {
      // The island must keep showing the clock even with no usage data.
    }
  }, []);

  useEffect(() => {
    void loadUsage();
    const updated = listen("usage-updated", () => void loadUsage());
    void updated.catch((error) =>
      console.error("cannot listen for usage updates", error),
    );
    return () => {
      void updated.then((un) => un()).catch(() => {});
    };
  }, [loadUsage]);

  const loadAgenda = useCallback(async () => {
    try {
      // A 35-day window covers the visible month grid without join-link
      // lookups, which would cost a round trip per event.
      setAgenda(await ipc.calendarAgenda(35, false));
    } catch {
      // No calendars or no bus: the section is simply omitted.
    }
  }, []);

  useEffect(() => {
    void loadAgenda();
    const timer = setInterval(loadAgenda, 120_000);
    return () => clearInterval(timer);
  }, [loadAgenda]);

  const [notifError, setNotifError] = useState<string | null>(null);
  const loadNotifications = useCallback(async () => {
    try {
      setNotifications(await ipc.notificationsList());
      setNotifError(null);
    } catch (e) {
      // Surfaced rather than swallowed: an empty list and a broken bridge look
      // identical otherwise, which hides real faults.
      setNotifError(String(e));
    }
  }, []);

  useEffect(() => {
    void loadNotifications();
    const received = listen("notifications-received", () => void loadNotifications());
    void received.catch((error) =>
      console.error("cannot listen for notifications", error),
    );
    return () => {
      void received.then((un) => un()).catch(() => {});
    };
  }, [loadNotifications]);

  // Metrics and mic are only needed while the island can be seen.
  useEffect(() => {
    if (!open) return;
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
  }, [open]);

  // -- derived ------------------------------------------------------------

  const eventDays = useMemo(() => {
    const days = new Set<string>();
    agenda?.days.forEach((day) => {
      if (day.events.length > 0) days.add(day.date);
    });
    return days;
  }, [agenda]);

  const shownDay = selectedDay ?? clock;
  const dayEvents: CalendarEvent[] = useMemo(() => {
    const key = isoDay(shownDay);
    return agenda?.days.find((day) => day.date === key)?.events ?? [];
  }, [agenda, shownDay]);

  const today = board?.days.at(-1);
  const nextEvent = agenda?.happeningNow ?? agenda?.nextUp ?? null;
  const criticalCount = notifications.filter((n) => n.urgency === "critical").length;

  // Indicators in priority order, because only two fit beside the clock.
  // A muted microphone leads: it is a state the user needs to notice, unlike a
  // figure they can look up. Then unread notifications, an imminent event, a
  // playing track, and finally today's spend.
  const indicators: JSX.Element[] = [];
  if (mic?.muted) {
    indicators.push(
      <span className="glyph muted" key="mic" title="Microphone muted">
        ▲ mic
      </span>,
    );
  }
  if (notifications.length > 0) {
    indicators.push(
      <span
        className={`glyph${criticalCount > 0 ? " alert" : ""}`}
        key="notif"
        title={`${notifications.length} notifications`}
      >
        ● {notifications.length}
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
    <div className="notch-root" onMouseEnter={peek} onMouseLeave={scheduleClose}>
      <div className={`island${open ? " expanded" : ""}`}>
        <button
          className="pill"
          onClick={togglePinned}
          aria-expanded={open}
          title={pinned ? "Click to close" : "Click to keep open"}
        >
          <span className="clock">
            {dateLabel} {timeLabel}
          </span>
          <span className="sep" />
          {/* The pill is a fixed width, so only the two most useful indicators
              are shown; the rest are visible once the island is open. */}
          {indicators.slice(0, 2)}
        </button>

        {open && (
          <div className="island-body">
            <div className="island-cols">
              <div className="island-col">
                {playing && (
                  <div className="card-mini">
                    <div className="mini-head">
                      <span className="mini-app">{playing.identity}</span>
                    </div>
                    <div className="mini-media">
                      {playing.artUrl ? (
                        <img className="mini-art" src={playing.artUrl} alt="" />
                      ) : (
                        <span className="mini-art glyphy">♪</span>
                      )}
                      <div className="mini-media-text">
                        <div className="mini-title" title={playing.title}>
                          {playing.title || "Untitled"}
                        </div>
                        <div className="mini-sub">{playing.artist || playing.identity}</div>
                      </div>
                      <TransportControls playing={playing} control={control} compact />
                    </div>
                    <TrackProgress playing={playing} />
                  </div>
                )}

                <div className="notif-head">
                  <span className="island-label">Notifications</span>
                  {notifications.length > 0 && (
                    <button
                      className="mini-btn"
                      onClick={async () => {
                        await ipc.notificationsClear();
                        void loadNotifications();
                      }}
                    >
                      Clear
                    </button>
                  )}
                </div>
                <div className="notif-list">
                  {notifError ? (
                    <div className="notif-empty">{notifError}</div>
                  ) : notifications.length === 0 ? (
                    <div className="notif-empty">No notifications</div>
                  ) : (
                    notifications.slice(0, 8).map((entry) => (
                      <div className={`notif urgency-${entry.urgency}`} key={entry.id}>
                        <div className="notif-top">
                          <span className="notif-app">{entry.appName || "System"}</span>
                          <span className="notif-when">{agoFromMillis(entry.receivedAt)}</span>
                          <button
                            className="notif-x"
                            aria-label="Remove from history"
                            title="Remove from history"
                            onClick={async () => {
                              await ipc.notificationsDismiss(entry.id);
                              void loadNotifications();
                            }}
                          >
                            ✕
                          </button>
                        </div>
                        {entry.summary && <div className="notif-summary">{entry.summary}</div>}
                        {entry.body && <div className="notif-body">{entry.body}</div>}
                      </div>
                    ))
                  )}
                </div>
              </div>

              <div className="island-col right">
                <div className="date-head">
                  <div className="date-dow">
                    {shownDay.toLocaleDateString(undefined, { weekday: "long" })}
                  </div>
                  <div className="date-full">
                    {shownDay.toLocaleDateString(undefined, {
                      month: "long",
                      day: "numeric",
                      year: "numeric",
                    })}
                  </div>
                </div>

                <MonthGrid
                  month={month}
                  today={clock}
                  eventDays={eventDays}
                  selected={selectedDay}
                  onMonthChange={(delta) =>
                    setMonth((current) => {
                      const next = new Date(current);
                      next.setDate(1);
                      next.setMonth(next.getMonth() + delta);
                      return next;
                    })
                  }
                  onSelect={(date) => {
                    setSelectedDay(date);
                    if (date.getMonth() !== month.getMonth()) setMonth(date);
                  }}
                />

                <div className="day-events">
                  <div className="island-label">
                    {sameDay(shownDay, clock) ? "Today" : "Events"}
                  </div>
                  {dayEvents.length === 0 ? (
                    <div className="notif-empty">No events</div>
                  ) : (
                    dayEvents.slice(0, 4).map((event) => (
                      <div className="island-event" key={`${event.eventUid}-${event.start}`}>
                        <span className="when">
                          {event.allDay ? "all day" : clockTime(event.start)}
                        </span>
                        <span className="what">{event.summary}</span>
                      </div>
                    ))
                  )}
                </div>
              </div>
            </div>

            <div className="island-foot">
              <div className="foot-stats">
                {board && (
                  <span title="Agent spend, last 7 days">
                    <strong>{money(board.totals.cost)}</strong> 7d ·{" "}
                    {tokens(board.totals.tokens)}
                  </span>
                )}
                {snapshot && (
                  <span title="This computer">
                    CPU <strong>{percent(snapshot.cpu.usagePercent)}</strong> · MEM{" "}
                    <strong>
                      {percent(
                        (snapshot.memory.usedBytes / Math.max(snapshot.memory.totalBytes, 1)) *
                          100,
                      )}
                    </strong>
                  </span>
                )}
              </div>
              <div className="foot-actions">
                <div
                  className={`shelf-chip${dragOver ? " over" : ""}`}
                  onDragOver={(event) => {
                    event.preventDefault();
                    setDragOver(true);
                  }}
                  onDragLeave={() => setDragOver(false)}
                  onDrop={(event) => {
                    event.preventDefault();
                    setDragOver(false);
                    const dropped = Array.from(event.dataTransfer.files).map((f) => f.name);
                    setFiles((current) => [...dropped, ...current].slice(0, 5));
                  }}
                  title={files.length > 0 ? files.join("\n") : "Drop files to stage them"}
                >
                  ⬓ {files.length > 0 ? `${files.length} staged` : "Shelf"}
                </div>
                <button
                  className="mini-btn"
                  onClick={() =>
                    control(playing?.status === "playing" ? "pause" : "toggle")
                  }
                  disabled={!playing}
                >
                  {playing?.status === "playing" ? "Pause" : "Play"}
                </button>
                <button
                  className={`mini-btn${mic?.muted ? " on" : ""}`}
                  onClick={async () => {
                    try {
                      setMic(await ipc.microphoneToggle());
                    } catch {
                      /* leave the previous state visible */
                    }
                  }}
                >
                  {mic?.muted ? "Unmute" : "Mute"}
                </button>
                <button className="mini-btn" onClick={() => void ipc.showMainWindow()}>
                  Open
                </button>
                <button className="mini-btn" onClick={dismiss} title="Close (Esc)">
                  ✕
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
