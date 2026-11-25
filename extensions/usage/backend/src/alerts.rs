use std::time::Duration;

use veronica_sdk::paths::AppDirectories;
use veronica_sdk::settings::Settings;
use veronica_sdk::Context;
use veronica_system::notify::{self, Timeout, Urgency};
use veronica_usage::alerts::{self, LimitAlert, NotifierState, NotifySettings};
use veronica_usage::limits::LimitWindow;

pub(crate) const POSTED: &str = "alerts.posted";
pub(crate) const LIMITS: &str = "usage.limits";

pub(crate) const DEFAULT_POLL_SECS: u64 = 60;
pub(crate) const MIN_POLL_SECS: u64 = 30;
pub(crate) const MAX_POLL_SECS: u64 = 900;

const IDLE_CHECK_SECS: u64 = 10;

pub(crate) fn poll_interval(settings: &Settings) -> Duration {
    let seconds = settings
        .get("notifyPollSeconds")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(DEFAULT_POLL_SECS)
        .clamp(MIN_POLL_SECS, MAX_POLL_SECS);
    Duration::from_secs(seconds)
}

pub(crate) async fn run(context: Context, directories: AppDirectories) {
    let connection = match zbus::Connection::session().await {
        Ok(connection) => connection,
        Err(error) => {
            tracing::info!("alerts are unavailable without a session bus: {error}");
            return;
        }
    };
    let state_path = directories.alerts_state_file();

    loop {
        let settings = Settings::load(&directories.settings_file()).unwrap_or_default();
        let notify_settings = NotifySettings::from_settings(&settings);

        if !notify_settings.master {
            let mut state = NotifierState::load(&state_path);
            if state.has_tracking() {
                state.reset_tracking();
                if let Err(error) = state.save(&state_path) {
                    tracing::warn!("cannot clear alert state: {error:#}");
                } else {
                    tracing::info!("alerts are off; forgot the tracked levels");
                }
            }
            tokio::time::sleep(Duration::from_secs(IDLE_CHECK_SECS)).await;
            continue;
        }

        tick(&context, &connection, &notify_settings, &state_path).await;
        tokio::time::sleep(poll_interval(&settings)).await;
    }
}

async fn tick(
    context: &Context,
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
            state.token_expired_at = None;
            publish(context, limits.session, limits.week);
        }
        Ok(None) => {}
        Err(error) => {
            let message = format!("{error:#}");
            if is_credential_failure(&message) {
                if let Some(alert) = alerts::token_expired(settings, &mut state, now) {
                    alerts_to_post.push(alert);
                }
            } else {
                tracing::debug!("alert poll could not read limits: {message}");
            }
        }
    }

    for alert in &alerts_to_post {
        let notification = notify::Notification::new(&alert.title, &alert.body)
            .urgency(urgency_for(&alert.id))
            .timeout(timeout_for(&alert.id))
            .replacing(state.banner(&alert.id).unwrap_or(0));

        match notify::post(connection, &notification).await {
            Ok(id) => state.record_banner(&alert.id, id),
            Err(error) => {
                tracing::warn!("cannot post {}: {error:#}", alert.id);
            }
        }
    }

    if state != before {
        if let Err(error) = state.save(state_path) {
            tracing::warn!("cannot persist alert state: {error:#}");
        }
    }
    if !alerts_to_post.is_empty() {
        let _ = context.host().emit(
            POSTED,
            serde_json::to_value(&alerts_to_post).unwrap_or_default(),
        );
    }
}

fn publish(context: &Context, session: Option<LimitWindow>, week: Option<LimitWindow>) {
    let _ = context.host().emit(
        LIMITS,
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

pub(crate) fn is_credential_failure(message: &str) -> bool {
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

fn urgency_for(alert_id: &str) -> Urgency {
    if alert_id.starts_with("escalation_") || alert_id == "token_expired" {
        Urgency::Critical
    } else if alert_id.starts_with("recovery_") {
        Urgency::Low
    } else {
        Urgency::Normal
    }
}

fn timeout_for(alert_id: &str) -> Timeout {
    if alert_id.starts_with("escalation_") || alert_id == "token_expired" {
        Timeout::Never
    } else {
        Timeout::Default
    }
}

pub(crate) async fn send_test(connection: &zbus::Connection) -> anyhow::Result<u32> {
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
        let mut settings = Settings::default();
        assert_eq!(poll_interval(&settings).as_secs(), DEFAULT_POLL_SECS);

        settings.set("notifyPollSeconds", serde_json::json!(1));
        assert_eq!(poll_interval(&settings).as_secs(), MIN_POLL_SECS);

        settings.set("notifyPollSeconds", serde_json::json!(86_400));
        assert_eq!(poll_interval(&settings).as_secs(), MAX_POLL_SECS);

        settings.set("notifyPollSeconds", serde_json::json!(120));
        assert_eq!(poll_interval(&settings).as_secs(), 120);

        settings.set("notifyPollSeconds", serde_json::json!("soon"));
        assert_eq!(poll_interval(&settings).as_secs(), DEFAULT_POLL_SECS);
    }

    #[test]
    fn only_a_credential_failure_becomes_a_banner() {
        assert!(is_credential_failure(
            "the saved Claude credentials have expired; sign in with Claude Code again"
        ));
        assert!(is_credential_failure("HTTP 401 Unauthorized"));
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
