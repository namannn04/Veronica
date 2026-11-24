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

const FIRST_BATCH_TIMEOUT: StdDuration = StdDuration::from_millis(2500);
const QUIET_PERIOD: StdDuration = StdDuration::from_millis(400);

type RawEvent = (String, String, i64, i64, HashMap<String, OwnedValue>);

fn as_text(value: &OwnedValue) -> Option<String> {
    match &**value {
        Value::Str(s) => Some(s.as_str().to_string()),
        _ => None,
    }
}

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
        other => matches!(other, Value::Value(_)) && false,
    })
}

pub async fn events(connection: &Connection, days: i64) -> Result<Vec<Event>> {
    let now = Local::now();
    let (since, until) = agenda::default_range(now, days);

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
        .call_method(
            Some(BUS),
            PATH,
            Some(INTERFACE),
            "SetTimeRange",
            &(since, until, true),
        )
        .await
        .context("the calendar server refused the time range")?;

    let mut collected: Vec<Event> = Vec::new();
    let mut timeout = FIRST_BATCH_TIMEOUT;

    loop {
        let next = tokio::time::timeout(timeout, stream.next()).await;
        let Ok(Some(Ok(message))) = next else {
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
                timeout = QUIET_PERIOD;
            }
            Some("EventsRemoved") => {
                let body = message.body();
                if let Ok((ids,)) = body.deserialize::<(Vec<String>,)>() {
                    let removed: Vec<(String, String)> =
                        ids.iter().map(|id| agenda::parse_event_id(id)).collect();
                    collected.retain(|event| {
                        !removed.iter().any(|(source, uid)| {
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

fn convert(raw: RawEvent) -> Option<Event> {
    let (id, summary, start_unix, end_unix, extras) = raw;
    let (source_uid, event_uid) = agenda::parse_event_id(&id);
    let start = agenda::from_unix(start_unix)?;
    let end = agenda::from_unix(end_unix)?;
    let end = if end < start { start } else { end };

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

const EDS_BUS: &str = "org.gnome.evolution.dataserver.Calendar8";
const EDS_FACTORY_PATH: &str = "/org/gnome/evolution/dataserver/CalendarFactory";
const EDS_FACTORY: &str = "org.gnome.evolution.dataserver.CalendarFactory";
const EDS_CALENDAR: &str = "org.gnome.evolution.dataserver.Calendar";

pub const MAX_ENRICHED: usize = 60;

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

async fn fetch_object(connection: &Connection, path: &str, event_uid: &str) -> Result<String> {
    let reply = connection
        .call_method(
            Some(EDS_BUS),
            path,
            Some(EDS_CALENDAR),
            "GetObject",
            &(event_uid, ""),
        )
        .await
        .with_context(|| format!("cannot read event {event_uid}"))?;
    let body = reply.body();
    let (ical,): (String,) = body.deserialize()?;
    Ok(ical)
}

pub async fn enrich_join_links(connection: &Connection, events: &mut [Event]) {
    let mut paths: HashMap<String, Option<String>> = HashMap::new();
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
                    tracing::debug!("cannot open calendar {} for join links", event.source_uid);
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

pub async fn events_with_links(connection: &Connection, days: i64) -> Result<Vec<Event>> {
    let mut list = events(connection, days).await?;
    enrich_join_links(connection, &mut list).await;
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_the_tuple_the_server_actually_sends() {
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
        assert_eq!(event.duration().num_seconds(), 0);
        assert!(!event.has_ended(event.start - chrono::Duration::seconds(1)));
        assert!(event.has_ended(event.start));
    }
}
