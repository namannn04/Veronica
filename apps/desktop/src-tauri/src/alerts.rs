//! The alert runner.
//!
//! Edith evaluates its alerts inside the limits refresh it already performs,
//! and hands future reminders to `UNUserNotificationCenter`. Neither applies
//! here: Veronica's limits are read on demand rather than on a schedule, and
//! freedesktop notifications cannot be scheduled at all. So this owns a poll of
//! its own.
//!
//! Two properties matter:
//!
//! - **It costs nothing when alerts are off.** Reading Claude's limits is a
//!   request to the provider, so the poll only happens while the master switch
//!   is on. Turning alerts off stops the traffic entirely.
//! - **It is edge-triggered across restarts.** The notifier state is persisted,
//!   so relaunching Veronica while a window sits at 90% does not re-announce it.
//!
//! The poll also emits `limits-updated`, so screens already open get fresh
//! figures out of a request that was being made anyway.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use veronica_system::notify::{self, Timeout, Urgency};
use veronica_usage::alerts::{self, LimitAlert, NotifierState, NotifySettings};
use veronica_usage::limits::LimitWindow;

use crate::state::AppState;

/// How often to re-read the limits while alerts are on.
pub const DEFAULT_POLL_SECS: u64 = 60;
/// Bounds for the configured interval. Below the floor this would hammer the
/// provider; above the ceiling a reminder would be uselessly late.
pub const MIN_POLL_SECS: u64 = 30;
pub const MAX_POLL_SECS: u64 = 900;

/// While alerts are off, how often to look at the switch again. Cheap: it reads
/// one small JSON file and makes no network request.
const IDLE_CHECK_SECS: u64 = 10;

pub fn poll_interval(settings: &veronica_core::Settings) -> Duration {
    let seconds = settings
        .get("notifyPollSeconds")
        .and_then(|value| value.as_u64())
        .unwrap_or(DEFAULT_POLL_SECS)
        .clamp(MIN_POLL_SECS, MAX_POLL_SECS);
    Duration::from_secs(seconds)
}

/// Run until the process exits.
pub async fn run(app: AppHandle) {
    // A separate connection from the notification monitor's, so a monitor that
    // the bus refuses cannot stop alerts from being posted.
    let connection = match zbus::Connection::session().await {
        Ok(connection) => connection,
        Err(error) => {
            tracing::info!(target: "veronica", "alerts are unavailable without a session bus: {error}");
            return;
        }
    };

    loop {
        let (settings, state_path) = {
            let state = app.state::<AppState>();
            (
                state.settings_snapshot(),
                state.directories.alerts_state_file(),
            )
        };
        let notify_settings = NotifySettings::from_settings(&settings);

        if !notify_settings.master {
            // Alerts being off clears the remembered levels, so switching them
            // back on does not immediately announce a state the user never saw a
            // transition into. Edith clears the same four values.
            //
            // Driven by what the state holds rather than by whether the switch
            // was flipped during *this* run: Veronica may have been restarted
            // since, and a launch with alerts already off must still clear. It is
            // also idempotent, so an idle loop does not rewrite the file on every
            // tick.
            let mut state = NotifierState::load(&state_path);
            if state.has_tracking() {
                state.reset_tracking();
                if let Err(error) = state.save(&state_path) {
                    tracing::warn!(target: "veronica", "cannot clear alert state: {error:#}");
                } else {
                    tracing::info!(
                        target: "veronica",
                        "alerts are off; forgot the tracked levels"
                    );
                }
            }
            tokio::time::sleep(Duration::from_secs(IDLE_CHECK_SECS)).await;
            continue;
        }

        tick(&app, &connection, &notify_settings, &state_path).await;
        tokio::time::sleep(poll_interval(&settings)).await;
    }
}

/// One evaluation: read the limits, decide, post.
async fn tick(
    app: &AppHandle,
    connection: &zbus::Connection,
    settings: &NotifySettings,
    state_path: &std::path::Path,
) {
    let now = chrono::Utc::now();
    let mut state = NotifierState::load(state_path);
    let before = state.clone();
    let mut alerts_to_post: Vec<LimitAlert> = Vec::new();

    match veronica_usage::claude::limits_for_user(now).await {
        Ok(Some(limits)) => {
            alerts_to_post.extend(alerts::decide(
                limits.session,
                limits.week,
                settings,
                &mut state,
                now,
            ));
            alerts_to_post.extend(alerts::due_reminders(
                limits.session,
                limits.week,
                settings,
                &mut state,
                now,
            ));
            // A successful read means the credentials are good again.
            state.token_expired_at = None;
            publish(app, limits.session, limits.week);
        }
        // Not signed in at all: nothing to alert about, and telling the user to
        // log in again would be wrong when they never logged in.
        Ok(None) => {}
        Err(error) => {
            let message = format!("{error:#}");
            // The one failure the user can act on. Everything else — a flaky
            // network, a provider outage — is logged and retried silently
            // rather than becoming a banner.
            if is_credential_failure(&message) {
                if let Some(alert) = alerts::token_expired(settings, &mut state, now) {
                    alerts_to_post.push(alert);
                }
            } else {
                tracing::debug!(target: "veronica", "alert poll could not read limits: {message}");
            }
        }
    }

    for alert in &alerts_to_post {
        let notification = notify::Notification::new(&alert.title, &alert.body)
            .urgency(urgency_for(&alert.id))
            .timeout(timeout_for(&alert.id))
            // Replacing this alert's own previous banner is what keeps a rising
            // threshold to one notification rather than a stack of five.
            .replacing(state.banner(&alert.id).unwrap_or(0));

        match notify::post(connection, &notification).await {
            Ok(id) => state.record_banner(&alert.id, id),
            Err(error) => {
                tracing::warn!(target: "veronica", "cannot post {}: {error:#}", alert.id);
            }
        }
    }

    if state != before {
        if let Err(error) = state.save(state_path) {
            tracing::warn!(target: "veronica", "cannot persist alert state: {error:#}");
        }
    }
    if !alerts_to_post.is_empty() {
        let _ = app.emit("alerts-posted", &alerts_to_post);
    }
}

/// Hand the freshly read windows to any open screen, so the poll's request is
/// not wasted on the alerts alone.
fn publish(app: &AppHandle, session: Option<LimitWindow>, week: Option<LimitWindow>) {
    let _ = app.emit(
        "limits-updated",
        serde_json::json!({
            "session": session.map(window_json),
            "week": week.map(window_json),
        }),
    );
}

fn window_json(window: LimitWindow) -> serde_json::Value {
    serde_json::json!({
        "percent": window.percent,
        "resetsAt": window.resets_at.map(|at| at.to_rfc3339()),
    })
}

/// Whether a limits failure is the user's credentials rather than the network.
///
/// Matched on the message because that is what the provider client reports; the
/// wording it uses for an expired login is stable and specific.
pub fn is_credential_failure(message: &str) -> bool {
    let lowered = message.to_lowercase();
    [
        "expired",
        "sign in",
        "log in",
        "unauthorized",
        "unauthenticated",
        "401",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

/// A window actually running out is worth interrupting for; pace advice is not.
fn urgency_for(alert_id: &str) -> Urgency {
    if alert_id.starts_with("escalation_") || alert_id == "token_expired" {
        Urgency::Critical
    } else if alert_id.starts_with("recovery_") {
        Urgency::Low
    } else {
        Urgency::Normal
    }
}

/// The alerts a user must not miss stay until dismissed; the rest time out.
fn timeout_for(alert_id: &str) -> Timeout {
    if alert_id.starts_with("escalation_") || alert_id == "token_expired" {
        Timeout::Never
    } else {
        Timeout::Default
    }
}

/// Fire one alert of each kind, so the user can confirm banners arrive at all.
///
/// Deliberately does not touch the notifier state: a test must not consume the
/// edge that a real alert would fire on.
pub async fn send_test(connection: &zbus::Connection) -> anyhow::Result<u32> {
    let notification = notify::Notification::new(
        "Alerts are working",
        "This is what a Veronica rate-limit alert looks like.",
    )
    .urgency(Urgency::Normal);
    notify::post(connection, &notification).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_poll_interval_is_clamped_to_something_sane() {
        let mut settings = veronica_core::Settings::default();
        assert_eq!(poll_interval(&settings).as_secs(), DEFAULT_POLL_SECS);

        settings.set("notifyPollSeconds", serde_json::json!(1));
        assert_eq!(poll_interval(&settings).as_secs(), MIN_POLL_SECS);

        settings.set("notifyPollSeconds", serde_json::json!(86_400));
        assert_eq!(poll_interval(&settings).as_secs(), MAX_POLL_SECS);

        settings.set("notifyPollSeconds", serde_json::json!(120));
        assert_eq!(poll_interval(&settings).as_secs(), 120);

        // A string where a number belongs falls back rather than panicking.
        settings.set("notifyPollSeconds", serde_json::json!("soon"));
        assert_eq!(poll_interval(&settings).as_secs(), DEFAULT_POLL_SECS);
    }

    #[test]
    fn only_a_credential_failure_becomes_a_banner() {
        assert!(is_credential_failure(
            "the saved Claude credentials have expired; sign in with Claude Code again"
        ));
        assert!(is_credential_failure("HTTP 401 Unauthorized"));
        // A flaky network must not tell the user to log in again.
        assert!(!is_credential_failure(
            "error sending request for url: connection refused"
        ));
        assert!(!is_credential_failure("the provider returned HTTP 503"));
    }

    #[test]
    fn the_alerts_that_matter_interrupt_and_the_rest_do_not() {
        for id in ["escalation_session", "escalation_weekly", "token_expired"] {
            assert_eq!(urgency_for(id), Urgency::Critical, "{id}");
            assert_eq!(timeout_for(id), Timeout::Never, "{id}");
        }
        for id in ["recovery_session", "recovery_weekly"] {
            assert_eq!(urgency_for(id), Urgency::Low, "{id}");
            assert_eq!(timeout_for(id), Timeout::Default, "{id}");
        }
        for id in ["pacing_session_hot", "reminder_weekly"] {
            assert_eq!(urgency_for(id), Urgency::Normal, "{id}");
            assert_eq!(timeout_for(id), Timeout::Default, "{id}");
        }
    }

    #[test]
    fn a_window_serialises_with_the_field_names_the_interface_expects() {
        let json = window_json(LimitWindow {
            percent: 81.5,
            resets_at: None,
        });
        assert_eq!(json["percent"], 81.5);
        assert!(json["resetsAt"].is_null());
    }
}
