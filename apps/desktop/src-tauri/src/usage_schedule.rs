use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::state::AppState;

pub(crate) const KEY: &str = "usageRefreshMinutes";
pub(crate) const DEFAULT_MINUTES: u64 = 15;
pub(crate) const MIN_MINUTES: u64 = 5;
pub(crate) const MAX_MINUTES: u64 = 24 * 60;

const CHECK_EVERY: Duration = Duration::from_mins(1);
const STARTUP_DELAY: Duration = Duration::from_secs(20);

pub(crate) fn interval(settings: &veronica_core::Settings) -> Option<Duration> {
    let minutes = settings
        .get(KEY)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(DEFAULT_MINUTES);
    (minutes > 0).then(|| Duration::from_secs(minutes.clamp(MIN_MINUTES, MAX_MINUTES) * 60))
}

pub(crate) fn is_due(
    since_attempt: Option<Duration>,
    data_age: Option<Duration>,
    every: Duration,
) -> bool {
    match since_attempt {
        Some(elapsed) => elapsed >= every,
        None => data_age.is_none_or(|age| age >= every),
    }
}

fn data_age(app: &AppHandle) -> Option<Duration> {
    let state = app.state::<AppState>();
    let usage = state.usage.lock().expect("usage lock");
    let generated = chrono::DateTime::parse_from_rfc3339(&usage.as_ref()?.generated_at).ok()?;
    (chrono::Utc::now() - generated.with_timezone(&chrono::Utc))
        .to_std()
        .ok()
}

pub(crate) async fn run(app: AppHandle) {
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

    const MINUTE: Duration = Duration::from_mins(1);

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
        assert!(!is_due(Some(MINUTE), None, every));
        assert!(is_due(Some(15 * MINUTE), None, every));
    }
}
