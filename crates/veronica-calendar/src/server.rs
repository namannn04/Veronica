//! Reading the agenda from GNOME's calendar server.
//!
//! `org.gnome.Shell.CalendarServer` is what the shell's own calendar dropdown
//! uses. Going through it rather than Evolution Data Server directly buys two
//! things: it aggregates every configured calendar, including online accounts,
//! and it expands recurrence rules, so a weekly standup arrives as one instance
//! per occurrence instead of an RRULE to interpret.
//!
//! The interface is signal-driven rather than a getter. `SetTimeRange` asks for
//! a window, and the events arrive afterwards on `EventsAddedOrUpdated`. There
//! is no completion signal, so a read waits for a quiet period after the last
//! batch instead of a definite end.

use std::collections::HashMap;
use std::time::Duration as StdDuration;

use anyhow::{Context, Result};
use chrono::Local;
use futures_util::StreamExt;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, MessageStream};

use crate::agenda::{self, Event};

pub const BUS: &str = "org.gnome.Shell.CalendarServer";
pub const PATH: &str = "/org/gnome/Shell/CalendarServer";
pub const INTERFACE: &str = "org.gnome.Shell.CalendarServer";

/// How long to wait for the first batch of events.
const FIRST_BATCH_TIMEOUT: StdDuration = StdDuration::from_millis(2500);
/// How long to wait for another batch once one has arrived.
const QUIET_PERIOD: StdDuration = StdDuration::from_millis(400);

/// One event as the server delivers it: id, summary, start, end, extras.
type RawEvent = (String, String, i64, i64, HashMap<String, OwnedValue>);

/// Read a D-Bus string. `OwnedValue` is not `Clone` in this zbus version, so
/// the variant is matched through its `Value` deref rather than converted.
fn as_text(value: &OwnedValue) -> Option<String> {
    match &**value {
        Value::Str(s) => Some(s.as_str().to_string()),
        _ => None,
    }
}

/// Whether any calendar is configured at all.
///
/// Distinguishes "no events scheduled" from "no calendars set up", which are
/// very different things to show a user.
pub async fn has_calendars(connection: &Connection) -> Result<bool> {
    let reply = connection
        .call_method(
            Some(BUS),
            PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(INTERFACE, "HasCalendars"),
        )
        .await
        .context("the calendar server did not answer")?;
    let body = reply.body();
    let value: Value = body.deserialize()?;
    Ok(match value {
        Value::Bool(flag) => flag,
        // The property is declared as a boolean; anything else means the
        // interface changed, and claiming calendars exist would be worse.
        other => matches!(other, Value::Value(_)) && false,
    })
}

/// Read the agenda for `days` starting at local midnight today.
pub async fn events(connection: &Connection, days: i64) -> Result<Vec<Event>> {
    let now = Local::now();
    let (since, until) = agenda::default_range(now, days);

    // Subscribe before asking, or a fast reply is missed entirely.
    //
    // A plain MessageStream is not enough: these are broadcast signals, and the
    // bus only routes them to a connection that has registered a match rule for
    // them. `for_match_rule` registers it and removes it on drop.
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(BUS)?
        .path(PATH)?
        .interface(INTERFACE)?
        .build();
    let mut stream = MessageStream::for_match_rule(rule, connection, Some(64))
        .await
        .context("cannot subscribe to calendar events")?;

    connection
        .call_method(Some(BUS), PATH, Some(INTERFACE), "SetTimeRange", &(
            since, until, true,
        ))
        .await
        .context("the calendar server refused the time range")?;

    let mut collected: Vec<Event> = Vec::new();
    let mut timeout = FIRST_BATCH_TIMEOUT;

    loop {
        let next = tokio::time::timeout(timeout, stream.next()).await;
        let Ok(Some(Ok(message))) = next else {
            // A timeout or a closed stream both mean nothing more is coming.
            break;
        };

        let header = message.header();
        if header.interface().map(|i| i.as_str()) != Some(INTERFACE) {
            continue;
        }
        match header.member().map(|m| m.as_str()) {
            Some("EventsAddedOrUpdated") => {
                let body = message.body();
                if let Ok((raw,)) = body.deserialize::<(Vec<RawEvent>,)>() {
                    collected.extend(raw.into_iter().filter_map(convert));
                }
                // More batches may follow, but only briefly.
                timeout = QUIET_PERIOD;
            }
            Some("EventsRemoved") => {
                let body = message.body();
                if let Ok((ids,)) = body.deserialize::<(Vec<String>,)>() {
                    let removed: Vec<(String, String)> =
                        ids.iter().map(|id| agenda::parse_event_id(id)).collect();
                    collected.retain(|event| {
                        !removed
                            .iter()
                            .any(|(source, uid)| {
                                event.source_uid == *source && event.event_uid == *uid
                            })
                    });
                }
                timeout = QUIET_PERIOD;
            }
            _ => {}
        }
    }

    Ok(agenda::deduplicate(collected))
}

/// Build an `Event` from the server's tuple.
///
/// A timestamp the local timezone cannot represent is dropped rather than
/// guessed at, which is why this returns an option.
fn convert(raw: RawEvent) -> Option<Event> {
    let (id, summary, start_unix, end_unix, extras) = raw;
    let (source_uid, event_uid) = agenda::parse_event_id(&id);
    let start = agenda::from_unix(start_unix)?;
    let end = agenda::from_unix(end_unix)?;
    // A malformed event with the end before the start would render as negative
    // duration; treat the start as the end instead.
    let end = if end < start { start } else { end };

    // The server does not pass the location or description through, so a link is
    // recovered from whatever text is available: an extras entry when one is
    // present, otherwise the summary, where people often paste the meeting URL.
    let join_url = ["location", "description", "url"]
        .iter()
        .filter_map(|key| extras.get(*key))
        .filter_map(as_text)
        .find_map(|text| crate::links::extract(&text))
        .or_else(|| crate::links::extract(&summary));

    Some(Event {
        source_uid,
        event_uid,
        summary,
        all_day: agenda::is_all_day(start, end),
        join_url,
        start,
        end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_the_tuple_the_server_actually_sends() {
        // Captured from a live GNOME 50 session.
        let raw: RawEvent = (
            "veronica-test\nveronica-test-1\n".to_string(),
            "Standup".to_string(),
            1_787_178_600,
            1_787_180_400,
            HashMap::new(),
        );
        let event = convert(raw).expect("should convert");
        assert_eq!(event.source_uid, "veronica-test");
        assert_eq!(event.event_uid, "veronica-test-1");
        assert_eq!(event.summary, "Standup");
        assert_eq!(event.duration().num_minutes(), 30);
        assert!(!event.all_day);
    }

    #[test]
    fn an_end_before_the_start_is_clamped_rather_than_negative() {
        let raw: RawEvent = (
            "cal\nuid\n".to_string(),
            "Broken".to_string(),
            1_787_180_400,
            1_787_178_600,
            HashMap::new(),
        );
        let event = convert(raw).expect("should still convert");
        // Clamped to zero length rather than reported as negative. Such an event
        // is listed until its start passes, then counts as over, which is the
        // only consistent reading of a zero-length span.
        assert_eq!(event.duration().num_seconds(), 0);
        assert!(!event.has_ended(event.start - chrono::Duration::seconds(1)));
        assert!(event.has_ended(event.start));
    }
}

// -- Join links ------------------------------------------------------------

/// Evolution Data Server, which holds the detail the shell's server omits.
const EDS_BUS: &str = "org.gnome.evolution.dataserver.Calendar8";
const EDS_FACTORY_PATH: &str = "/org/gnome/evolution/dataserver/CalendarFactory";
const EDS_FACTORY: &str = "org.gnome.evolution.dataserver.CalendarFactory";
const EDS_CALENDAR: &str = "org.gnome.evolution.dataserver.Calendar";

/// Most events a single read will look up.
///
/// Each lookup is a D-Bus round trip, so a very wide range cannot be allowed to
/// turn one agenda read into hundreds of calls.
pub const MAX_ENRICHED: usize = 60;

/// Open a calendar and return the object path its subprocess lives on.
async fn open_calendar(connection: &Connection, source_uid: &str) -> Result<String> {
    let reply = connection
        .call_method(
            Some(EDS_BUS),
            EDS_FACTORY_PATH,
            Some(EDS_FACTORY),
            "OpenCalendar",
            &(source_uid,),
        )
        .await
        .with_context(|| format!("cannot open calendar {source_uid}"))?;
    let body = reply.body();
    let (path, _bus): (String, String) = body.deserialize()?;
    Ok(path)
}

/// Fetch one event's raw `VEVENT`.
async fn fetch_object(
    connection: &Connection,
    path: &str,
    event_uid: &str,
) -> Result<String> {
    let reply = connection
        .call_method(
            Some(EDS_BUS),
            path,
            Some(EDS_CALENDAR),
            "GetObject",
            // The empty recurrence id asks for the master event, which carries
            // the location and description shared by every occurrence.
            &(event_uid, ""),
        )
        .await
        .with_context(|| format!("cannot read event {event_uid}"))?;
    let body = reply.body();
    let (ical,): (String,) = body.deserialize()?;
    Ok(ical)
}

/// Fill in join links from Evolution Data Server.
///
/// Best effort throughout: a calendar that will not open, or an event that has
/// since been deleted, leaves that event without a link rather than failing the
/// whole agenda. Calendars are opened once each, because opening is the
/// expensive half.
pub async fn enrich_join_links(connection: &Connection, events: &mut [Event]) {
    let mut paths: HashMap<String, Option<String>> = HashMap::new();
    // Recurring events share one master, so the same lookup is reused.
    let mut links: HashMap<(String, String), Option<String>> = HashMap::new();
    let mut looked_up = 0usize;

    for event in events.iter_mut() {
        if event.join_url.is_some() || event.event_uid.is_empty() {
            continue;
        }
        let key = (event.source_uid.clone(), event.event_uid.clone());
        if let Some(cached) = links.get(&key) {
            event.join_url = cached.clone();
            continue;
        }
        if looked_up >= MAX_ENRICHED {
            break;
        }

        let path = match paths.get(&event.source_uid) {
            Some(cached) => cached.clone(),
            None => {
                let resolved = open_calendar(connection, &event.source_uid).await.ok();
                if resolved.is_none() {
                    tracing::debug!(
                        "cannot open calendar {} for join links",
                        event.source_uid
                    );
                }
                paths.insert(event.source_uid.clone(), resolved.clone());
                resolved
            }
        };

        let Some(path) = path else {
            links.insert(key, None);
            continue;
        };

        looked_up += 1;
        let url = match fetch_object(connection, &path, &event.event_uid).await {
            Ok(ical) => crate::ical::join_url(&ical),
            Err(error) => {
                tracing::debug!("no detail for {}: {error:#}", event.event_uid);
                None
            }
        };
        links.insert(key, url.clone());
        event.join_url = url;
    }
}

/// Read the agenda and fill in join links.
pub async fn events_with_links(connection: &Connection, days: i64) -> Result<Vec<Event>> {
    let mut list = events(connection, days).await?;
    enrich_join_links(connection, &mut list).await;
    Ok(list)
}
