export interface CalendarEvent {
  sourceUid: string;
  eventUid: string;
  summary: string;
  start: string;
  end: string;
  allDay: boolean;
  joinUrl: string | null;
}

export interface AgendaDay {
  date: string;
  label: string;
  isToday: boolean;
  events: CalendarEvent[];
}

export interface AgendaView {
  hasCalendars: boolean;
  days: AgendaDay[];
  nextUp: CalendarEvent | null;
  happeningNow: CalendarEvent | null;
}
