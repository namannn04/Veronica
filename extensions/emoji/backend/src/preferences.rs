use std::path::PathBuf;

use anyhow::Result;
use veronica_core::emoji::SkinTone;
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::settings::Settings;

pub(crate) const TONE_KEY: &str = "ext.emoji.skinTone";

pub(crate) struct Preferences {
    pub path: PathBuf,
    pub tone: SkinTone,
}

impl Preferences {
    pub(crate) fn from_settings(path: PathBuf, settings: &Settings) -> Self {
        Self {
            path,
            tone: SkinTone::parse(settings.string(TONE_KEY).unwrap_or("standard")),
        }
    }

    pub(crate) fn load() -> Result<Self> {
        let directories = AppDirectories::current()?;
        let settings = Settings::load(&directories.settings_file())?;
        Ok(Self::from_settings(
            directories.emoji_usage_file(),
            &settings,
        ))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_skin_tone_lives_in_the_extension_namespace() {
        let mut settings = Settings::default();
        let path = PathBuf::from("emoji-usage.json");
        assert_eq!(
            Preferences::from_settings(path.clone(), &settings).tone,
            SkinTone::Standard
        );
        settings.set(TONE_KEY, json!("dark"));
        assert_eq!(
            Preferences::from_settings(path, &settings).tone,
            SkinTone::parse("dark")
        );
    }
}
