use std::path::Path;

use anyhow::Result;
use serde_json::json;
use veronica_core::{AttentionPrivacy, AttentionSettings};
use veronica_sdk::settings::Settings;

pub(crate) const TRACKING_KEY: &str = "attentionTrackingEnabled";
pub(crate) const PRIVACY_KEY: &str = "attentionPrivacy";
pub(crate) const IDLE_KEY: &str = "attentionIdleSeconds";

fn privacy(settings: &AttentionSettings) -> &'static str {
    if settings.privacy == AttentionPrivacy::Detailed {
        "detailed"
    } else {
        "applications"
    }
}

pub(crate) fn mirror(path: &Path, settings: &AttentionSettings) -> Result<()> {
    let mut shared = Settings::load(path)?;
    shared.set(TRACKING_KEY, json!(settings.enabled));
    shared.set(PRIVACY_KEY, json!(privacy(settings)));
    shared.set(IDLE_KEY, json!(settings.idle_threshold_seconds));
    shared.save(path)
}

pub(crate) fn mirror_once(path: &Path, settings: &AttentionSettings) -> Result<()> {
    if Settings::load(path)?.get(TRACKING_KEY).is_some() {
        return Ok(());
    }
    mirror(path, settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_reads_tracking_settings_from_the_shared_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut settings = AttentionSettings {
            enabled: false,
            ..AttentionSettings::default()
        };
        mirror_once(&path, &settings).unwrap();
        let stored = Settings::load(&path).unwrap();
        assert_eq!(stored.get(TRACKING_KEY), Some(&json!(false)));
        assert_eq!(stored.get(PRIVACY_KEY), Some(&json!("applications")));

        settings.enabled = true;
        mirror_once(&path, &settings).unwrap();
        assert_eq!(
            Settings::load(&path).unwrap().get(TRACKING_KEY),
            Some(&json!(false))
        );

        settings.privacy = AttentionPrivacy::Detailed;
        mirror(&path, &settings).unwrap();
        let stored = Settings::load(&path).unwrap();
        assert_eq!(stored.get(TRACKING_KEY), Some(&json!(true)));
        assert_eq!(stored.get(PRIVACY_KEY), Some(&json!("detailed")));
    }
}
