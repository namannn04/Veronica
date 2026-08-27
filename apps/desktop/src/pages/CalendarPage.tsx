import { useCallback, useEffect, useState } from "react";

import { ipc } from "../lib/ipc";
import { eventSlot, untilLabel } from "../lib/format";
import type { AgendaView, CalendarEvent } from "../lib/types";

const PAGE_DAYS = 14;

export function CalendarPage() {
  const [view, setView] = useState<AgendaView | null>(null);
  const [days, setDays] = useState(PAGE_DAYS);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try { setView(await ipc.calendarAgenda(days, true)); setError(null); }
    catch (reason) { setError(String(reason)); }
    finally { setLoading(false); }
  }, [days]);

  useEffect(() => { void load(); }, [load]);

  return <div className="calendar-page">
    <div className="page-head edith-head"><div><h1>Calendar</h1>{view?.nextUp && <div className="page-sub">Next: {view.nextUp.summary} {untilLabel(view.nextUp.start)}</div>}</div><div className="head-actions"><button className="button" onClick={() => void load()} disabled={loading}>{loading ? "Reading…" : "Refresh"}</button><button className="button primary" onClick={() => void ipc.calendarOpen().catch((reason) => setError(String(reason)))}>Open Calendar ↗</button></div></div>

    {error && <div className="banner error">{error}</div>}
    {!view && loading && <CalendarSkeleton />}
    {view && !view.hasCalendars && <div className="empty calendar-empty"><div className="empty-glyph">▣</div><h3>Calendar access is ready</h3><p>Add an account in Ubuntu Settings › Online Accounts, or create a local calendar. Every configured calendar appears here automatically.</p><button className="button primary" onClick={() => void ipc.calendarOpen()}>Open Calendar</button></div>}
    {view?.hasCalendars && view.days.length === 0 && !loading && <div className="empty calendar-empty"><div className="empty-glyph">✓</div><h3>Clear runway</h3><p>Nothing scheduled in the next {days} days.</p></div>}

    {view?.happeningNow && <section className="calendar-now blur-calendar"><div><span>Happening now</span><strong>{view.happeningNow.summary}</strong><small>{eventSlot(view.happeningNow.start, view.happeningNow.end, view.happeningNow.allDay)}</small></div>{view.happeningNow.joinUrl && <button className="button primary" onClick={() => void ipc.openExternal(view.happeningNow!.joinUrl!)}>Join meeting</button>}</section>}

    {view && view.days.length > 0 && <div className="agenda-list blur-calendar">{view.days.map((day) => <section className="agenda-day" key={day.date}><div className="agenda-date"><strong>{day.label}</strong><span>{new Date(`${day.date}T12:00:00`).toLocaleDateString([], { month: "short", day: "numeric" })}</span></div><div className="agenda-events">{day.events.map((event) => <EventRow key={`${event.eventUid}-${event.start}`} event={event} />)}</div></section>)}</div>}

    {view?.hasCalendars && view.days.length > 0 && <button className="load-more" onClick={() => setDays((value) => value + PAGE_DAYS)} disabled={loading}>{loading ? "Loading…" : `Load ${PAGE_DAYS} more days`}</button>}
  </div>;
}

function EventRow({ event }: { event: CalendarEvent }) {
  const start = new Date(event.start);
  return <article className="agenda-event"><div className="agenda-time"><strong>{event.allDay ? "All day" : start.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}</strong>{!event.allDay && <span>{eventSlot(event.start, event.end, false).split("–").at(-1)}</span>}</div><i aria-hidden="true" /><div className="agenda-summary"><strong className="event-summary" title={event.summary}>{event.summary}</strong><span>{event.allDay ? "All-day event" : start.toLocaleDateString([], { weekday: "long" })}</span></div>{event.joinUrl && <button className="join-button" onClick={() => void ipc.openExternal(event.joinUrl!)} title={event.joinUrl}>Join <span>▸</span></button>}</article>;
}

function CalendarSkeleton() { return <div className="calendar-skeleton" aria-label="Reading calendar"><i /><i /><i /></div>; }
