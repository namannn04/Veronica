use std::time::Duration;

use veronica_sdk::paths::AppDirectories;
use veronica_sdk::settings::Settings;
use veronica_sdk::Context;

pub(crate) const KEY: &str = "usageRefreshMinutes";
pub(crate) const DEFAULT_MINUTES: u64 = 15;
pub(crate) const MIN_MINUTES: u64 = 5;
pub(crate) const MAX_MINUTES: u64 = 24 * 60;

const CHECK_EVERY: Duration = Duration::from_mins(1);
const STARTUP_DELAY: Duration = Duration::from_secs(20);

pub(crate) fn interval(settings: &Settings) -> Option<Duration> {
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

fn data_age(directories: &AppDirectories) -> Option<Duration> {
    let document = veronica_usage::collector::read_document(&directories.usage_file())
        .ok()
        .flatten()?;
    let generated = chrono::DateTime::parse_from_rfc3339(&document.generated_at).ok()?;
    (chrono::Utc::now() - generated.with_timezone(&chrono::Utc))
        .to_std()
        .ok()
}

pub(crate) async fn run(context: Context, directories: AppDirectories) {
    tokio::time::sleep(STARTUP_DELAY).await;
    let mut last_attempt: Option<std::time::Instant> = None;
    loop {
        let settings = Settings::load(&directories.settings_file()).unwrap_or_default();
        if let Some(every) = interval(&settings) {
            let since_attempt = last_attempt.map(|at| at.elapsed());
            if is_due(since_attempt, data_age(&directories), every) {
                last_attempt = Some(std::time::Instant::now());
                if let Err(error) = crate::refresh::run(&context, &directories).await {
                    tracing::info!("scheduled usage collection: {error}");
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
        let mut settings = Settings::default();
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
