use std::time::Duration;

use serde_json::{json, Value};
use veronica_calendar::Event;
use veronica_sdk::HostClient;

use crate::rpc::agenda;

pub(crate) const SURFACE: &str = "agenda";
const REFRESH: Duration = Duration::from_mins(5);
const DAYS: i64 = 2;
const MAX_ITEMS: usize = 5;

fn slot(event: &Event) -> String {
    if event.all_day {
        return "All day".to_string();
    }
    format!(
        "{} – {}",
        event.start.format("%H:%M"),
        event.end.format("%H:%M")
    )
}

pub(crate) fn state(events: &[&Event]) -> Value {
    if events.is_empty() {
        return Value::Null;
    }
    let items: Vec<Value> = events
        .iter()
        .take(MAX_ITEMS)
        .map(|event| json!({ "title": event.summary, "detail": slot(event) }))
        .collect();
    json!({ "items": items })
}

pub(crate) async fn keep_current(host: HostClient) {
    loop {
        if let Ok(view) = agenda(DAYS, false).await {
            let upcoming: Vec<&Event> = view.days.iter().flat_map(|day| &day.events).collect();
            let _ = host.surface_update(SURFACE, state(&upcoming)).await;
        }
        tokio::time::sleep(REFRESH).await;
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Local, TimeZone};

    use super::*;

    fn event(summary: &str, hour: u32, all_day: bool) -> Event {
        let start = Local.with_ymd_and_hms(2026, 11, 25, hour, 0, 0).unwrap();
        Event {
            source_uid: "source".into(),
            event_uid: summary.into(),
            summary: summary.into(),
            start,
            end: start + chrono::Duration::minutes(30),
            all_day,
            join_url: None,
        }
    }

    #[test]
    fn the_home_card_lists_the_next_events() {
        let standup = event("Standup", 9, false);
        let holiday = event("Holiday", 0, true);
        assert_eq!(
            state(&[&standup, &holiday]),
            json!({ "items": [
                { "title": "Standup", "detail": "09:00 – 09:30" },
                { "title": "Holiday", "detail": "All day" }
            ] })
        );
        assert_eq!(state(&[]), Value::Null);
        let many: Vec<Event> = (8..16).map(|hour| event("Busy", hour, false)).collect();
        let refs: Vec<&Event> = many.iter().collect();
        assert_eq!(state(&refs)["items"].as_array().unwrap().len(), MAX_ITEMS);
    }
}
