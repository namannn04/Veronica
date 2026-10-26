//! Collecting agent usage on a schedule.
//!
//! The collector used to run only when somebody pressed Refresh on the Agent
//! Usage page or in the tray, so the dashboard, Home and the notch showed
//! whatever was last collected — weeks old, on a machine where nobody pressed
//! it. While Agent Usage is enabled, the running app now collects every
//! `usageRefreshMinutes`, and once soon after launch when the stored figures
//! are already older than that.

use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::state::AppState;

pub const KEY: &str = "usageRefreshMinutes";
pub const DEFAULT_MINUTES: u64 = 15;
/// A collection walks every agent's history; more often than this costs more
/// than the freshness is worth.
pub const MIN_MINUTES: u64 = 5;
pub const MAX_MINUTES: u64 = 24 * 60;

/// How often to look at the clock and the switches. Cheap: no I/O beyond the
/// settings already in memory.
const CHECK_EVERY: Duration = Duration::from_secs(60);
/// Let launch finish before the first collection competes with it.
const STARTUP_DELAY: Duration = Duration::from_secs(20);

/// The interval, or `None` when scheduled collection is switched off (`0`).
pub fn interval(settings: &veronica_core::Settings) -> Option<Duration> {
    let minutes = settings
        .get(KEY)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(DEFAULT_MINUTES);
    (minutes > 0).then(|| Duration::from_secs(minutes.clamp(MIN_MINUTES, MAX_MINUTES) * 60))
}

/// Whether a collection is due.
///
/// `since_attempt` is how long ago this process last tried (`None` before its
/// first try); `data_age` is how old the stored figures are (`None` when there
/// are none). Before the first try the data's age decides, so a restart does
/// not collect again data that is still fresh; after it, the last try does,
/// so a machine with no agent history is not retried every minute.
pub fn is_due(
    since_attempt: Option<Duration>,
    data_age: Option<Duration>,
    every: Duration,
) -> bool {
    match since_attempt {
        Some(elapsed) => elapsed >= every,
        None => data_age.is_none_or(|age| age >= every),
    }
}

/// How old the figures in memory are.
fn data_age(app: &AppHandle) -> Option<Duration> {
    let state = app.state::<AppState>();
    let usage = state.usage.lock().expect("usage lock");
    let generated = chrono::DateTime::parse_from_rfc3339(&usage.as_ref()?.generated_at).ok()?;
    (chrono::Utc::now() - generated.with_timezone(&chrono::Utc))
        .to_std()
        .ok()
}

/// Run until the process exits.
pub async fn run(app: AppHandle) {
    tokio::time::sleep(STARTUP_DELAY).await;
    let mut last_attempt: Option<std::time::Instant> = None;
    loop {
        let settings = app.state::<AppState>().settings_snapshot();
        if let Some(every) = interval(&settings) {
            let enabled = veronica_core::extensions::entry("usage")
                .is_some_and(|entry| settings.extension_enabled(entry));
            let since_attempt = last_attempt.map(|at| at.elapsed());
            if enabled && is_due(since_attempt, data_age(&app), every) {
                last_attempt = Some(std::time::Instant::now());
                // A refresh the user started is already doing this one's work.
                if let Err(error) = crate::commands::usage_refresh(app.clone()).await {
                    tracing::info!(target: "veronica", "scheduled usage collection: {error}");
                }
            }
        }
        tokio::time::sleep(CHECK_EVERY).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MINUTE: Duration = Duration::from_secs(60);

    #[test]
    fn the_interval_defaults_clamps_and_can_be_switched_off() {
        let mut settings = veronica_core::Settings::default();
        assert_eq!(interval(&settings), Some(15 * MINUTE));
        settings.set(KEY, json!(1));
        assert_eq!(interval(&settings), Some(5 * MINUTE));
        settings.set(KEY, json!(100_000));
        assert_eq!(interval(&settings), Some(24 * 60 * MINUTE));
        settings.set(KEY, json!(0));
        assert_eq!(interval(&settings), None);
    }

    #[test]
    fn at_launch_only_stale_or_missing_data_is_collected() {
        let every = 15 * MINUTE;
        assert!(is_due(None, None, every), "nothing collected yet");
        assert!(
            is_due(None, Some(66 * 24 * 60 * MINUTE), every),
            "weeks old"
        );
        assert!(!is_due(None, Some(3 * MINUTE), every), "still fresh");
    }

    #[test]
    fn after_a_try_the_schedule_follows_the_try_not_the_data() {
        let every = 15 * MINUTE;
        // No history to collect leaves the data missing; that must not mean
        // trying again every minute.
        assert!(!is_due(Some(MINUTE), None, every));
        assert!(is_due(Some(15 * MINUTE), None, every));
    }
}
