use std::collections::HashMap;

use anyhow::{Context, Result};
use zbus::zvariant::Value;
use zbus::Connection;

pub const NOTIFY_BUS: &str = "org.freedesktop.Notifications";
pub const NOTIFY_PATH: &str = "/org/freedesktop/Notifications";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timeout {
    Default,
    Never,
    Milliseconds(i32),
}

impl Timeout {
    fn as_i32(self) -> i32 {
        match self {
            Self::Default => -1,
            Self::Never => 0,
            Self::Milliseconds(ms) => ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

impl Urgency {
    fn as_u8(self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Normal => 1,
            Self::Critical => 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    pub timeout: Timeout,
    pub replaces_id: u32,
}

impl Notification {
    pub fn new(summary: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            body: body.into(),
            urgency: Urgency::Normal,
            timeout: Timeout::Default,
            replaces_id: 0,
        }
    }

    pub fn urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }

    pub fn timeout(mut self, timeout: Timeout) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn replacing(mut self, id: u32) -> Self {
        self.replaces_id = id;
        self
    }
}

pub async fn post(connection: &Connection, notification: &Notification) -> Result<u32> {
    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
    hints.insert("urgency", Value::U8(notification.urgency.as_u8()));
    hints.insert("desktop-entry", Value::Str(veronica_core::APP_ID.into()));

    let id: u32 = connection
        .call_method(
            Some(NOTIFY_BUS),
            NOTIFY_PATH,
            Some(NOTIFY_BUS),
            "Notify",
            &(
                "Veronica",
                notification.replaces_id,
                veronica_core::APP_ID,
                notification.summary.as_str(),
                notification.body.as_str(),
                &[] as &[&str],
                hints,
                notification.timeout.as_i32(),
            ),
        )
        .await
        .context("the notification service refused the message")?
        .body()
        .deserialize()
        .context("the notification service returned no id")?;

    Ok(id)
}

pub async fn close(connection: &Connection, id: u32) -> Result<()> {
    connection
        .call_method(
            Some(NOTIFY_BUS),
            NOTIFY_PATH,
            Some(NOTIFY_BUS),
            "CloseNotification",
            &(id,),
        )
        .await
        .context("cannot close the notification")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeouts_use_the_freedesktop_sentinels() {
        assert_eq!(Timeout::Default.as_i32(), -1);
        assert_eq!(Timeout::Never.as_i32(), 0);
        assert_eq!(Timeout::Milliseconds(5000).as_i32(), 5000);
    }

    #[test]
    fn urgencies_map_to_the_spec_values() {
        assert_eq!(Urgency::Low.as_u8(), 0);
        assert_eq!(Urgency::Normal.as_u8(), 1);
        assert_eq!(Urgency::Critical.as_u8(), 2);
    }

    #[test]
    fn a_new_notification_does_not_replace_anything() {
        let notification = Notification::new("Claude at 85%", "Resets in 2h 14m");
        assert_eq!(notification.replaces_id, 0);
        assert_eq!(notification.urgency, Urgency::Normal);
    }

    #[test]
    fn builders_compose_so_a_rising_alert_can_update_one_banner() {
        let notification = Notification::new("Claude at 95%", "Resets in 40m")
            .urgency(Urgency::Critical)
            .timeout(Timeout::Never)
            .replacing(42);
        assert_eq!(notification.replaces_id, 42);
        assert_eq!(notification.urgency, Urgency::Critical);
        assert_eq!(notification.timeout, Timeout::Never);
    }
}
