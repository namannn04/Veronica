import { useCallback, useEffect, useState } from "react";

import { ipc } from "../lib/ipc";
import { eventSlot, untilLabel } from "../lib/format";
import type { AgendaView, CalendarEvent } from "../lib/types";

const RANGES = [7, 14, 30];

/**
 * The agenda.
 *
 * Events come from GNOME's calendar server, so every configured calendar is
 * included — local, Google, Nextcloud — with recurrences already expanded.
 */
export function CalendarPage() {
  const [view, setView] = useState<AgendaView | null>(null);
  const [days, setDays] = useState(7);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setView(await ipc.calendarAgenda(days, true));
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [days]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Calendar</h1>
          <div className="page-sub">
            {view?.nextUp
              ? `Next: ${view.nextUp.summary} ${untilLabel(view.nextUp.start)}`
              : "Every calendar configured on this computer"}
          </div>
        </div>
        <button className="button" onClick={load} disabled={loading}>
          {loading ? "Reading…" : "Refresh"}
        </button>
      </div>

      {error && <div className="banner error">{error}</div>}

      <div className="toolbar">
        <div className="segmented" role="group" aria-label="Range">
          {RANGES.map((range) => (
            <button key={range} aria-pressed={days === range} onClick={() => setDays(range)}>
              {range}d
            </button>
          ))}
        </div>
      </div>

      {view && !view.hasCalendars && (
        <div className="empty">
          <h3>No calendars configured</h3>
          <p>
            Add an account in Settings › Online Accounts, or create a local
            calendar, and your agenda appears here.
          </p>
        </div>
      )}

      {view?.hasCalendars && view.days.length === 0 && !loading && (
        <div className="empty">
          <h3>Nothing scheduled</h3>
          <p>No events in the next {days} days.</p>
        </div>
      )}

      {view?.happeningNow && (
        <section className="card" style={{ marginBottom: 12 }}>
          <div className="card-head">
            <h2>Happening now</h2>
          </div>
          <EventRow event={view.happeningNow} highlight />
        </section>
      )}

      {view?.days.map((day) => (
        <section className="card" key={day.date} style={{ marginBottom: 12 }}>
          <div className="card-head">
            <h2>{day.label}</h2>
            <span className="card-note">{day.date}</span>
          </div>
          {day.events.map((event) => (
            <EventRow key={`${event.eventUid}-${event.start}`} event={event} />
          ))}
        </section>
      ))}
    </>
  );
}

function EventRow({ event, highlight }: { event: CalendarEvent; highlight?: boolean }) {
  return (
    <div className="event-row">
      <span className={`event-slot${event.allDay ? " all-day" : ""}`}>
        {eventSlot(event.start, event.end, event.allDay)}
      </span>
      <span className="event-summary" title={event.summary}>
        {event.summary}
      </span>
      {highlight && <span className="pill good">Now</span>}
      {event.joinUrl && (
        <button
          className="button primary"
          onClick={() => void ipc.openExternal(event.joinUrl!)}
          title={event.joinUrl}
        >
          Join
        </button>
      )}
    </div>
  );
}
