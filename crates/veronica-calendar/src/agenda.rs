//! Turning a flat event list into an agenda.
//!
//! All of this is pure so it can be tested without a session bus. The shell's
//! calendar server hands back already-expanded instances in no particular
//! order, spanning whatever range was requested; the agenda groups them by
//! local day, marks all-day events, and orders everything the way a person
//! reads a schedule.

use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone};
use serde::Serialize;

/// One calendar event instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    /// Calendar this came from.
    pub source_uid: String,
    /// Event identifier within that calendar.
    pub event_uid: String,
    pub summary: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    /// True when the event occupies whole days rather than a time slot.
    pub all_day: bool,
    /// A meeting URL, when one could be recovered.
    pub join_url: Option<String>,
}

impl Event {
    /// Stable key for deduplication: the same instance can be delivered twice
    /// when the server re-emits a range.
    pub fn key(&self) -> (String, String, i64) {
        (
            self.source_uid.clone(),
            self.event_uid.clone(),
            self.start.timestamp(),
        )
    }

    pub fn duration(&self) -> Duration {
        self.end - self.start
    }

    /// Whether the event is over at `now`.
    ///
    /// An all-day event counts as current for the whole day, which is why the
    /// comparison is against its end rather than its start.
    pub fn has_ended(&self, now: DateTime<Local>) -> bool {
        self.end <= now
    }

    pub fn is_current(&self, now: DateTime<Local>) -> bool {
        self.start <= now && now < self.end
    }
}

/// Decode the shell's composite event id.
///
/// The server packs the calendar and event identifiers into one string
/// separated by newlines, with a trailing newline: `"source\nuid\n"`. Splitting
/// naively leaves an empty third field, and using the whole string as a key
/// makes two different events in the same calendar look related.
pub fn parse_event_id(raw: &str) -> (String, String) {
    let mut parts = raw.split('\n');
    let source = parts.next().unwrap_or_default().to_string();
    let uid = parts.next().unwrap_or_default().to_string();
    (source, uid)
}

/// Whether a span covers whole local days.
///
/// The server reports all-day events as midnight to midnight in local time
/// rather than flagging them, so the only way to tell is to check the boundaries
/// and that the span is at least a day.
pub fn is_all_day(start: DateTime<Local>, end: DateTime<Local>) -> bool {
    let at_midnight = |value: DateTime<Local>| {
        value.time() == chrono::NaiveTime::MIN
    };
    at_midnight(start) && at_midnight(end) && (end - start) >= Duration::days(1)
}

/// Events for one local day.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgendaDay {
    /// `YYYY-MM-DD`, local.
    pub date: String,
    /// "Today", "Tomorrow", or a formatted date.
    pub label: String,
    pub is_today: bool,
    pub events: Vec<Event>,
}

/// Human label for a day relative to today.
pub fn day_label(date: NaiveDate, today: NaiveDate) -> String {
    let delta = (date - today).num_days();
    match delta {
        0 => "Today".to_string(),
        1 => "Tomorrow".to_string(),
        -1 => "Yesterday".to_string(),
        // Within the coming week the weekday alone is unambiguous and shorter.
        2..=6 => date.format("%A").to_string(),
        _ => date.format("%a %-d %b").to_string(),
    }
}

/// Group events into days, ordered earliest first.
///
/// A multi-day event appears on each day it covers, because an agenda that only
/// listed it on its first day would leave the following days looking empty.
pub fn group_by_day(events: &[Event], now: DateTime<Local>) -> Vec<AgendaDay> {
    use std::collections::BTreeMap;

    let today = now.date_naive();
    let mut days: BTreeMap<NaiveDate, Vec<Event>> = BTreeMap::new();

    for event in events {
        let first = event.start.date_naive();
        // An all-day event's exclusive midnight end would otherwise add one
        // spurious extra day.
        let last_instant = if event.all_day {
            event.end - Duration::seconds(1)
        } else {
            event.end
        };
        let last = last_instant.date_naive().max(first);

        let mut date = first;
        while date <= last {
            days.entry(date).or_default().push(event.clone());
            date += Duration::days(1);
        }
    }

    days.into_iter()
        .map(|(date, mut events)| {
            // All-day events lead the day, then by start time, then by name so
            // the order never depends on delivery order.
            events.sort_by(|a, b| {
                b.all_day
                    .cmp(&a.all_day)
                    .then_with(|| a.start.cmp(&b.start))
                    .then_with(|| a.summary.cmp(&b.summary))
            });
            AgendaDay {
                date: date.format("%Y-%m-%d").to_string(),
                label: day_label(date, today),
                is_today: date == today,
                events,
            }
        })
        .collect()
}

/// Drop events that already finished, keeping anything still running.
pub fn upcoming(events: &[Event], now: DateTime<Local>) -> Vec<Event> {
    events
        .iter()
        .filter(|event| !event.has_ended(now))
        .cloned()
        .collect()
}

/// Remove duplicate instances, keeping the last delivered version.
///
/// The server re-emits an event when it is edited, so a naive append would show
/// both the old and the new copy.
pub fn deduplicate(events: Vec<Event>) -> Vec<Event> {
    use std::collections::HashMap;
    let mut latest: HashMap<(String, String, i64), Event> = HashMap::new();
    for event in events {
        latest.insert(event.key(), event);
    }
    let mut list: Vec<Event> = latest.into_values().collect();
    list.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.summary.cmp(&b.summary)));
    list
}

/// The next event that has not started yet, for the notch's one-line readout.
pub fn next_up(events: &[Event], now: DateTime<Local>) -> Option<&Event> {
    events
        .iter()
        .filter(|event| event.start > now && !event.all_day)
        .min_by_key(|event| event.start)
}

/// The event happening right now, if any.
pub fn happening_now(events: &[Event], now: DateTime<Local>) -> Option<&Event> {
    events
        .iter()
        .filter(|event| event.is_current(now) && !event.all_day)
        .min_by_key(|event| event.end)
}

/// Convert a unix timestamp from the server into local time.
pub fn from_unix(seconds: i64) -> Option<DateTime<Local>> {
    match Local.timestamp_opt(seconds, 0) {
        chrono::offset::LocalResult::Single(value) => Some(value),
        // An ambiguous local time across a DST boundary resolves to the earlier
        // reading rather than being discarded.
        chrono::offset::LocalResult::Ambiguous(earlier, _) => Some(earlier),
        chrono::offset::LocalResult::None => None,
    }
}

/// Local midnight today, and `days` later: the range the agenda asks for.
pub fn default_range(now: DateTime<Local>, days: i64) -> (i64, i64) {
    let start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| from_unix(Local.from_local_datetime(&naive).single()?.timestamp()))
        .unwrap_or(now);
    let end = start + Duration::days(days.max(1));
    (start.timestamp(), end.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(date: &str, hour: u32, minute: u32) -> DateTime<Local> {
        let naive = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap();
        Local.from_local_datetime(&naive).unwrap()
    }

    fn event(summary: &str, start: DateTime<Local>, end: DateTime<Local>) -> Event {
        Event {
            source_uid: "cal".into(),
            event_uid: summary.to_lowercase().replace(' ', "-"),
            summary: summary.into(),
            start,
            end,
            all_day: is_all_day(start, end),
            join_url: None,
        }
    }

    #[test]
    fn parses_the_newline_packed_event_id() {
        // The server sends "source\nuid\n", trailing newline included.
        assert_eq!(
            parse_event_id("veronica-test\nveronica-test-1\n"),
            ("veronica-test".to_string(), "veronica-test-1".to_string())
        );
    }

    #[test]
    fn a_malformed_event_id_still_yields_two_fields() {
        assert_eq!(parse_event_id(""), (String::new(), String::new()));
        assert_eq!(parse_event_id("onlysource"), ("onlysource".to_string(), String::new()));
    }

    #[test]
    fn recognises_a_midnight_to_midnight_span_as_all_day() {
        assert!(is_all_day(at("2026-08-20", 0, 0), at("2026-08-21", 0, 0)));
        // A multi-day all-day event too.
        assert!(is_all_day(at("2026-08-20", 0, 0), at("2026-08-23", 0, 0)));
    }

    #[test]
    fn a_timed_event_is_not_all_day_even_if_it_starts_at_midnight() {
        assert!(!is_all_day(at("2026-08-20", 0, 0), at("2026-08-20", 1, 0)));
        assert!(!is_all_day(at("2026-08-20", 9, 30), at("2026-08-20", 10, 0)));
        // A zero-length midnight event is not a day.
        assert!(!is_all_day(at("2026-08-20", 0, 0), at("2026-08-20", 0, 0)));
    }

    #[test]
    fn day_labels_are_relative_near_today_and_absolute_further_out() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 20).unwrap();
        assert_eq!(day_label(today, today), "Today");
        assert_eq!(day_label(today + Duration::days(1), today), "Tomorrow");
        assert_eq!(day_label(today - Duration::days(1), today), "Yesterday");
        // Two to six days out reads as a weekday.
        assert_eq!(
            day_label(today + Duration::days(3), today),
            (today + Duration::days(3)).format("%A").to_string()
        );
        // Beyond a week a weekday would be ambiguous, so show the date.
        assert!(day_label(today + Duration::days(20), today).contains("Sep"));
    }

    #[test]
    fn groups_events_into_ordered_days() {
        let now = at("2026-08-20", 9, 0);
        let events = vec![
            event("Design review", at("2026-08-21", 15, 0), at("2026-08-21", 16, 0)),
            event("Standup", at("2026-08-20", 9, 30), at("2026-08-20", 10, 0)),
        ];
        let days = group_by_day(&events, now);
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].date, "2026-08-20");
        assert!(days[0].is_today);
        assert_eq!(days[0].label, "Today");
        assert_eq!(days[0].events[0].summary, "Standup");
        assert_eq!(days[1].label, "Tomorrow");
    }

    #[test]
    fn all_day_events_lead_the_day() {
        let now = at("2026-08-20", 9, 0);
        let events = vec![
            event("Standup", at("2026-08-20", 9, 30), at("2026-08-20", 10, 0)),
            event("Holiday", at("2026-08-20", 0, 0), at("2026-08-21", 0, 0)),
        ];
        let days = group_by_day(&events, now);
        assert_eq!(days[0].events[0].summary, "Holiday");
        assert!(days[0].events[0].all_day);
        assert_eq!(days[0].events[1].summary, "Standup");
    }

    #[test]
    fn an_all_day_event_does_not_leak_into_the_following_day() {
        // Its end is an exclusive midnight, so a naive range would add a day.
        let now = at("2026-08-20", 9, 0);
        let events = vec![event("Holiday", at("2026-08-20", 0, 0), at("2026-08-21", 0, 0))];
        let days = group_by_day(&events, now);
        assert_eq!(days.len(), 1, "should cover one day only");
        assert_eq!(days[0].date, "2026-08-20");
    }

    #[test]
    fn a_multi_day_event_appears_on_every_day_it_covers() {
        let now = at("2026-08-20", 9, 0);
        let events = vec![event("Conference", at("2026-08-20", 0, 0), at("2026-08-23", 0, 0))];
        let days = group_by_day(&events, now);
        assert_eq!(days.len(), 3);
        assert_eq!(days.iter().map(|d| d.date.as_str()).collect::<Vec<_>>(),
                   vec!["2026-08-20", "2026-08-21", "2026-08-22"]);
    }

    #[test]
    fn a_timed_event_spanning_midnight_appears_on_both_days() {
        let now = at("2026-08-20", 9, 0);
        let events = vec![event("Deploy window", at("2026-08-20", 23, 0), at("2026-08-21", 1, 0))];
        let days = group_by_day(&events, now);
        assert_eq!(days.len(), 2);
    }

    #[test]
    fn upcoming_drops_finished_events_but_keeps_one_in_progress() {
        let now = at("2026-08-20", 9, 45);
        let events = vec![
            event("Finished", at("2026-08-20", 8, 0), at("2026-08-20", 9, 0)),
            event("Running", at("2026-08-20", 9, 30), at("2026-08-20", 10, 0)),
            event("Later", at("2026-08-20", 14, 0), at("2026-08-20", 15, 0)),
        ];
        let remaining = upcoming(&events, now);
        let kept: Vec<&str> = remaining.iter().map(|e| e.summary.as_str()).collect();
        assert_eq!(kept, vec!["Running", "Later"]);
    }

    #[test]
    fn deduplicate_keeps_the_latest_version_of_a_re_emitted_event() {
        let start = at("2026-08-20", 9, 30);
        let mut first = event("Standup", start, at("2026-08-20", 10, 0));
        let mut second = first.clone();
        second.summary = "Standup (moved room)".into();
        first.summary = "Standup".into();
        let list = deduplicate(vec![first, second]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].summary, "Standup (moved room)");
    }

    #[test]
    fn deduplicate_keeps_separate_instances_of_a_recurring_event() {
        // Same uid, different start: two occurrences, both real.
        let monday = event("Standup", at("2026-08-20", 9, 30), at("2026-08-20", 10, 0));
        let tuesday = event("Standup", at("2026-08-21", 9, 30), at("2026-08-21", 10, 0));
        assert_eq!(deduplicate(vec![monday, tuesday]).len(), 2);
    }

    #[test]
    fn next_up_ignores_all_day_events_and_anything_already_started() {
        let now = at("2026-08-20", 9, 45);
        let events = vec![
            event("Holiday", at("2026-08-20", 0, 0), at("2026-08-21", 0, 0)),
            event("Running", at("2026-08-20", 9, 30), at("2026-08-20", 10, 0)),
            event("Next", at("2026-08-20", 11, 0), at("2026-08-20", 12, 0)),
            event("After", at("2026-08-20", 14, 0), at("2026-08-20", 15, 0)),
        ];
        assert_eq!(next_up(&events, now).unwrap().summary, "Next");
    }

    #[test]
    fn happening_now_finds_the_soonest_ending_current_event() {
        let now = at("2026-08-20", 9, 45);
        let events = vec![
            event("Long block", at("2026-08-20", 9, 0), at("2026-08-20", 12, 0)),
            event("Standup", at("2026-08-20", 9, 30), at("2026-08-20", 10, 0)),
        ];
        assert_eq!(happening_now(&events, now).unwrap().summary, "Standup");
    }

    #[test]
    fn nothing_current_or_next_is_none_rather_than_a_panic() {
        let now = at("2026-08-20", 23, 59);
        assert!(next_up(&[], now).is_none());
        assert!(happening_now(&[], now).is_none());
    }

    #[test]
    fn the_default_range_starts_at_local_midnight_today() {
        let now = at("2026-08-20", 14, 30);
        let (since, until) = default_range(now, 7);
        assert_eq!(since, at("2026-08-20", 0, 0).timestamp());
        assert_eq!(until - since, 7 * 86_400);
    }

    #[test]
    fn the_default_range_is_never_empty() {
        let now = at("2026-08-20", 14, 30);
        let (since, until) = default_range(now, 0);
        assert!(until > since);
    }

    #[test]
    fn an_event_is_current_only_inside_its_span() {
        let e = event("Standup", at("2026-08-20", 9, 30), at("2026-08-20", 10, 0));
        assert!(!e.is_current(at("2026-08-20", 9, 29)));
        assert!(e.is_current(at("2026-08-20", 9, 30)));
        assert!(e.is_current(at("2026-08-20", 9, 59)));
        // The end is exclusive, so it is over at exactly 10:00.
        assert!(!e.is_current(at("2026-08-20", 10, 0)));
        assert!(e.has_ended(at("2026-08-20", 10, 0)));
    }
}
