use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::Value;
use veronica_core::swatches::{ColorProfile, CopyFormat, SwatchHistory, DEFAULT_HISTORY_SIZE};
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::settings::Settings;

pub(crate) const FORMAT_KEY: &str = "ext.color.copyFormat";
pub(crate) const PROFILE_KEY: &str = "ext.color.profile";
pub(crate) const HISTORY_KEY: &str = "ext.color.historySize";

pub(crate) struct Preferences {
    pub path: PathBuf,
    pub format: CopyFormat,
    pub profile: ColorProfile,
    pub limit: usize,
}

impl Preferences {
    pub(crate) fn from_settings(path: PathBuf, settings: &Settings) -> Self {
        Self {
            path,
            format: CopyFormat::parse(settings.string(FORMAT_KEY).unwrap_or("hex")),
            profile: ColorProfile::parse(settings.string(PROFILE_KEY).unwrap_or("srgb")),
            limit: SwatchHistory::clamp_limit(
                settings
                    .get(HISTORY_KEY)
                    .and_then(Value::as_u64)
                    .map_or(DEFAULT_HISTORY_SIZE, |size| {
                        usize::try_from(size).unwrap_or(usize::MAX)
                    }),
            ),
        }
    }

    pub(crate) fn load() -> Result<Self> {
        let directories = AppDirectories::current()?;
        let settings = Settings::load(&directories.settings_file())?;
        Ok(Self::from_settings(directories.swatches_file(), &settings))
    }

    pub(crate) fn history(path: &Path) -> Result<SwatchHistory> {
        SwatchHistory::load(path)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn preferences_live_in_the_extension_namespace() {
        let mut settings = Settings::default();
        let defaults = Preferences::from_settings(PathBuf::from("swatches.json"), &settings);
        assert_eq!(defaults.format, CopyFormat::Hex);
        assert_eq!(defaults.profile, ColorProfile::Srgb);
        assert_eq!(defaults.limit, DEFAULT_HISTORY_SIZE);
        settings.set(FORMAT_KEY, json!("rgb"));
        settings.set(PROFILE_KEY, json!("displayP3"));
        settings.set(HISTORY_KEY, json!(10));
        let chosen = Preferences::from_settings(PathBuf::from("swatches.json"), &settings);
        assert_eq!(chosen.format, CopyFormat::parse("rgb"));
        assert_eq!(chosen.profile, ColorProfile::DisplayP3);
        assert_eq!(chosen.limit, SwatchHistory::clamp_limit(10));
    }
}
