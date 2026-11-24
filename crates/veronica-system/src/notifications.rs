use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::Serialize;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, MessageStream};

pub const NOTIFY_BUS: &str = "org.freedesktop.Notifications";
pub const NOTIFY_PATH: &str = "/org/freedesktop/Notifications";

pub const HISTORY_LIMIT: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

impl Urgency {
    fn from_hint(value: u8) -> Self {
        match value {
            0 => Urgency::Low,
            2 => Urgency::Critical,
            _ => Urgency::Normal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: u64,
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    pub received_at: i64,
    pub desktop_entry: Option<String>,
}

type NotifyBody = (
    String,
    u32,
    String,
    String,
    String,
    Vec<String>,
    HashMap<String, OwnedValue>,
    i32,
);

fn hint_text(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    match &**hints.get(key)? {
        Value::Str(s) => Some(s.as_str().to_string()),
        _ => None,
    }
}

fn hint_u8(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<u8> {
    match &**hints.get(key)? {
        Value::U8(v) => Some(*v),
        Value::U32(v) => u8::try_from(*v).ok(),
        Value::I32(v) => u8::try_from(*v).ok(),
        _ => None,
    }
}

pub fn from_body(body: NotifyBody, id: u64, received_at: i64) -> Notification {
    let (app_name, _replaces, app_icon, summary, notification_body, _actions, hints, _timeout) =
        body;
    Notification {
        id,
        app_name,
        app_icon,
        summary,
        body: notification_body,
        urgency: hint_u8(&hints, "urgency")
            .map(Urgency::from_hint)
            .unwrap_or(Urgency::Normal),
        received_at,
        desktop_entry: hint_text(&hints, "desktop-entry"),
    }
}

pub fn is_interesting(notification: &Notification) -> bool {
    !(notification.summary.trim().is_empty() && notification.body.trim().is_empty())
}

pub async fn watch<F>(mut on_notification: F) -> Result<()>
where
    F: FnMut(Notification) + Send + 'static,
{
    let probe = Connection::session()
        .await
        .context("cannot reach the session bus")?;
    let server = zbus::fdo::DBusProxy::new(&probe)
        .await?
        .get_name_owner(NOTIFY_BUS.try_into()?)
        .await
        .context("no notification server is running")?
        .to_string();
    drop(probe);

    let connection = Connection::session().await?;
    let monitor = zbus::fdo::MonitoringProxy::builder(&connection)
        .destination("org.freedesktop.DBus")?
        .path("/org/freedesktop/DBus")?
        .build()
        .await?;

    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::MethodCall)
        .interface(NOTIFY_BUS)?
        .member("Notify")?
        .destination(zbus::names::UniqueName::try_from(server.as_str())?)?
        .build();
    monitor
        .become_monitor(&[rule], 0)
        .await
        .context("the bus refused to let Veronica monitor notifications")?;

    tracing::debug!(target: "veronica", "monitoring notifications for {server}");
    let mut stream = MessageStream::from(connection);
    let mut next_id = 1u64;

    while let Some(Ok(message)) = futures_util::StreamExt::next(&mut stream).await {
        let header = message.header();
        if header.member().map(|m| m.as_str()) != Some("Notify") {
            continue;
        }
        if header.destination().map(|d| d.to_string()).as_deref() != Some(server.as_str()) {
            continue;
        }

        let body = message.body();
        let decoded = match body.deserialize::<NotifyBody>() {
            Ok(decoded) => decoded,
            Err(error) => {
                tracing::debug!(target: "veronica", "cannot decode Notify body: {error}");
                continue;
            }
        };
        let notification = from_body(decoded, next_id, now_millis());
        if !is_interesting(&notification) {
            continue;
        }
        next_id += 1;
        on_notification(notification);
    }

    tracing::info!(target: "veronica", "notification monitor stream ended");
    Ok(())
}

fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints(pairs: &[(&str, Value<'static>)]) -> HashMap<String, OwnedValue> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), OwnedValue::try_from(value).unwrap()))
            .collect()
    }

    fn body(summary: &str, text: &str, hint_map: HashMap<String, OwnedValue>) -> NotifyBody {
        (
            "notify-send".to_string(),
            0,
            String::new(),
            summary.to_string(),
            text.to_string(),
            Vec::new(),
            hint_map,
            -1,
        )
    }

    #[test]
    fn decodes_the_call_shape_the_bus_carries() {
        let notification = from_body(
            body(
                "Veronica test",
                "Checking whether monitoring works",
                hints(&[("urgency", Value::U8(1))]),
            ),
            7,
            1_787_218_143_000,
        );
        assert_eq!(notification.id, 7);
        assert_eq!(notification.app_name, "notify-send");
        assert_eq!(notification.summary, "Veronica test");
        assert_eq!(notification.urgency, Urgency::Normal);
        assert_eq!(notification.received_at, 1_787_218_143_000);
    }

    #[test]
    fn maps_the_urgency_hint() {
        for (raw, expected) in [
            (0u8, Urgency::Low),
            (1, Urgency::Normal),
            (2, Urgency::Critical),
        ] {
            let n = from_body(body("s", "b", hints(&[("urgency", Value::U8(raw))])), 1, 0);
            assert_eq!(n.urgency, expected);
        }
    }

    #[test]
    fn a_missing_urgency_hint_defaults_to_normal() {
        let n = from_body(body("s", "b", HashMap::new()), 1, 0);
        assert_eq!(n.urgency, Urgency::Normal);
        assert_eq!(n.desktop_entry, None);
    }

    #[test]
    fn urgency_is_accepted_as_a_wider_integer_too() {
        let n = from_body(body("s", "b", hints(&[("urgency", Value::U32(2))])), 1, 0);
        assert_eq!(n.urgency, Urgency::Critical);
    }

    #[test]
    fn reads_the_desktop_entry_hint_for_grouping() {
        let n = from_body(
            body("s", "b", hints(&[("desktop-entry", Value::from("code"))])),
            1,
            0,
        );
        assert_eq!(n.desktop_entry.as_deref(), Some("code"));
    }

    #[test]
    fn an_entirely_empty_notification_is_uninteresting() {
        let empty = from_body(body("   ", "", HashMap::new()), 1, 0);
        assert!(!is_interesting(&empty));
        let titled = from_body(body("Build finished", "", HashMap::new()), 2, 0);
        assert!(is_interesting(&titled));
        let described = from_body(body("", "3 tests failed", HashMap::new()), 3, 0);
        assert!(is_interesting(&described));
    }
}
