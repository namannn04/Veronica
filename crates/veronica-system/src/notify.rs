//! Desktop notifications over D-Bus.
//!
//! Edith posts through `UNUserNotificationCenter`; the freedesktop equivalent
//! is `org.freedesktop.Notifications`. Replacing a notification in place matters
//! for the usage alerts: a threshold that keeps rising should update one banner
//! rather than stack five.

use std::collections::HashMap;

use anyhow::{Context, Result};
use zbus::zvariant::Value;
use zbus::Connection;

pub const NOTIFY_BUS: &str = "org.freedesktop.Notifications";
pub const NOTIFY_PATH: &str = "/org/freedesktop/Notifications";

/// How long a banner stays up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timeout {
    /// Let the desktop decide.
    Default,
    /// Stay until dismissed, for alerts the user must acknowledge.
    Never,
    Milliseconds(i32),
}

impl Timeout {
    fn as_i32(self) -> i32 {
        match self {
            Timeout::Default => -1,
            Timeout::Never => 0,
            Timeout::Milliseconds(ms) => ms,
        }
    }
}

/// Freedesktop urgency. `Critical` banners bypass do-not-disturb, so it is
/// reserved for a limit actually being exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

impl Urgency {
    fn as_u8(self) -> u8 {
        match self {
            Urgency::Low => 0,
            Urgency::Normal => 1,
            Urgency::Critical => 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    pub timeout: Timeout,
    /// Pass the id returned by a previous post to update that banner in place.
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

/// Post a notification and return its id, which can replace it later.
pub async fn post(connection: &Connection, notification: &Notification) -> Result<u32> {
    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
    hints.insert("urgency", Value::U8(notification.urgency.as_u8()));
    // Lets the desktop group Veronica's banners and show the right icon.
    hints.insert(
        "desktop-entry",
        Value::Str(veronica_core::APP_ID.into()),
    );

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

/// Withdraw a notification that is still on screen.
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
