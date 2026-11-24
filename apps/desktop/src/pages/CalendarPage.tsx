import { useCallback, useEffect, useState } from "react";
import { CalendarCheck, CalendarDays, ExternalLink, RefreshCw, Video } from "lucide-react";

import { startVisiblePolling } from "../lib/polling";
import { ipc } from "../lib/ipc";
import { eventSlot, untilLabel } from "../lib/format";
import type { AgendaView, CalendarEvent } from "../lib/types";
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
  Section,
  SkeletonRows,
  Stack,
} from "../ui";
import "./CalendarPage.css";

const PAGE_DAYS = 14;
const RELOAD_MS = 60_000;
const TICK_MS = 30_000;

export function CalendarPage() {
  const [view, setView] = useState<AgendaView | null>(null);
  const [days, setDays] = useState(PAGE_DAYS);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [, setTick] = useState(0);

  const load = useCallback(
    async (quiet = false) => {
      if (!quiet) setLoading(true);
      try {
        setView(await ipc.calendarAgenda(days, true));
        setError(null);
      } catch (reason) {
        setError(String(reason));
      } finally {
        if (!quiet) setLoading(false);
      }
    },
    [days],
  );

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const stop = startVisiblePolling(() => load(true), RELOAD_MS, document, false);
    const tick = window.setInterval(() => {
      if (!document.hidden) setTick((value) => value + 1);
    }, TICK_MS);
    return () => {
      stop();
      window.clearInterval(tick);
    };
  }, [load]);

  const openCalendar = () => void ipc.calendarOpen().catch((reason) => setError(String(reason)));
  const nextUp = view?.nextUp ?? null;
  const subtitle = nextUp
    ? `Next: ${nextUp.summary} ${untilLabel(nextUp.start)}`
    : `Today and the next ${days} days`;

  return (
    <Page
      header={
        <PageHeader
          title="Calendar"
          subtitle={<span className={nextUp ? "event-summary" : undefined}>{subtitle}</span>}
          actions={
            <>
              <Button icon={<RefreshCw />} busy={loading} onClick={() => void load()}>
                Refresh
              </Button>
              <Button trailingIcon={<ExternalLink />} onClick={openCalendar}>
                Open Calendar
              </Button>
            </>
          }
        />
      }
    >
      {error && (
        <Notice
          tone="critical"
          title="Could not read your calendars"
          actions={
            <Button size="small" onClick={() => void load()}>
              Retry
            </Button>
          }
        >
          {error}
        </Notice>
      )}

      {!view && loading && (
        <Section title="Today">
          <List label="Loading events">
            <SkeletonRows rows={4} label="Reading calendar" />
          </List>
        </Section>
      )}

      {view && !view.hasCalendars && (
        <EmptyState
          icon={<CalendarDays />}
          title="No calendars yet"
          action={
            <Button variant="primary" onClick={openCalendar}>
              Open Calendar
            </Button>
          }
        >
          Add an account in Settings › Online Accounts, or create a local calendar. Every configured
          calendar appears here.
        </EmptyState>
      )}

      {view?.hasCalendars && view.days.every((day) => day.events.length === 0) && !loading && (
        <EmptyState icon={<CalendarCheck />} title="Nothing scheduled">
          Your next {days} days are free.
        </EmptyState>
      )}

      {view?.happeningNow && <HappeningNow event={view.happeningNow} />}

      {view && view.days.some((day) => day.events.length > 0) && (
        <Stack className="cal-agenda blur-calendar">
          {view.days
            .filter((day) => day.events.length > 0)
            .map((day) => (
              <Section
                key={day.date}
                title={day.label}
                description={dayDescription(day.date, day.label)}
              >
                <List label={day.label}>
                  {day.events.map((event) => (
                    <EventRow
                      key={`${event.eventUid}-${event.start}`}
                      event={event}
                      next={
                        nextUp !== null &&
                        nextUp.eventUid === event.eventUid &&
                        nextUp.start === event.start
                      }
                    />
                  ))}
                </List>
              </Section>
            ))}
        </Stack>
      )}

      {view?.hasCalendars && view.days.length > 0 && (
        <div className="cal-more">
          <Button busy={loading} onClick={() => setDays((value) => value + PAGE_DAYS)}>
            {loading ? "Loading…" : `Show ${PAGE_DAYS} more days`}
          </Button>
        </div>
      )}
    </Page>
  );
}

function HappeningNow({ event }: { event: CalendarEvent }) {
  return (
    <Card tone="accent" className="cal-now">
      <div className="cal-now-text blur-calendar">
        <p className="cal-now-label">Happening now</p>
        <p className="cal-now-title event-summary">{event.summary}</p>
        <p className="cal-now-slot ui-num">{eventSlot(event.start, event.end, event.allDay)}</p>
      </div>
      {event.joinUrl && (
        <Button
          variant="primary"
          icon={<Video />}
          onClick={() => void ipc.openExternal(event.joinUrl!)}
        >
          Join meeting
        </Button>
      )}
    </Card>
  );
}

function EventRow({ event, next }: { event: CalendarEvent; next: boolean }) {
  const start = new Date(event.start);
  const end = new Date(event.end);
  const time = event.allDay
    ? "All day"
    : start.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  const until = event.allDay
    ? null
    : end.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  return (
    <ListRow
      className={next ? "cal-event cal-event-next" : "cal-event"}
      leading={
        <span className="cal-time ui-num">
          <span>{time}</span>
          {until && <span className="cal-time-end">{until}</span>}
        </span>
      }
      title={<span className="event-summary">{event.summary}</span>}
      subtitle={event.allDay ? "All-day event" : durationLabel(start, end)}
      meta={next ? <Badge tone="accent">Up next</Badge> : undefined}
      trailing={
        event.joinUrl ? (
          <Button
            size="small"
            icon={<Video />}
            title={event.joinUrl}
            onClick={() => void ipc.openExternal(event.joinUrl!)}
          >
            Join
          </Button>
        ) : undefined
      }
    />
  );
}

function dayDescription(date: string, label: string): string {
  const day = new Date(`${date}T12:00:00`);
  const weekday = day.toLocaleDateString([], { weekday: "long" });
  const monthDay = day.toLocaleDateString([], { month: "long", day: "numeric" });
  return label === weekday ? monthDay : `${weekday} ${monthDay}`;
}

function durationLabel(start: Date, end: Date): string {
  const minutes = Math.round((end.getTime() - start.getTime()) / 60_000);
  if (!Number.isFinite(minutes) || minutes <= 0) return "No duration";
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `${hours} h ${rest} min` : `${hours} h`;
}
