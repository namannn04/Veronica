//! Shared application state.

use std::path::PathBuf;
use std::process::Child;
use std::sync::Mutex;
use std::time::Instant;

use anyhow::Result;
use veronica_core::{AppDirectories, DesktopSession, Settings};
use veronica_system::MetricsSampler;
use veronica_usage::UsageDocument;

/// Everything the Tauri commands need. Each field is independently locked so a
/// slow collector run cannot block a metrics tick.
pub struct AppState {
    pub directories: AppDirectories,
    pub session: Mutex<DesktopSession>,
    pub settings: Mutex<Settings>,
    /// A logind descriptor held while Lid Awake is enabled. Dropping it
    /// immediately restores the normal lid and idle behaviour.
    pub lid_awake: Mutex<Option<veronica_system::power::InhibitorLock>>,
    /// Separate from Lid Awake: this one only blocks the idle sleep timer.
    pub prevent_sleep: Mutex<Option<veronica_system::power::InhibitorLock>>,
    /// The last collected usage document, so the UI can render immediately on
    /// launch instead of waiting for a refresh.
    pub usage: Mutex<Option<UsageDocument>>,
    /// Held across ticks because CPU percentages need two samples to exist.
    pub sampler: Mutex<MetricsSampler>,
    /// True while the collector is running, so overlapping refreshes are
    /// rejected rather than corrupting the output directory.
    pub refreshing: Mutex<bool>,
    /// Notifications seen on the bus, newest first and bounded.
    pub notifications: Mutex<Vec<veronica_system::Notification>>,
    /// Native PipeWire recorder held between the Start and Stop IPC calls.
    pub companion_recording: Mutex<Option<CompanionRecording>>,
    /// The emoji catalogue, parsed once. It is two thousand entries with a
    /// precomputed search index, and the picker opens on a hotkey, so parsing
    /// it per call would be felt.
    pub emoji: veronica_core::EmojiCatalog,
}

pub struct CompanionRecording {
    pub child: Child,
    pub path: PathBuf,
    pub started: Instant,
}

impl AppState {
    pub fn new(directories: AppDirectories, session: DesktopSession) -> Result<Self> {
        let settings = Settings::load(&directories.settings_file())?;
        // A document from a previous run is not required; a first launch simply
        // has none until the user refreshes.
        let usage = match veronica_usage::collector::read_document(&directories.usage_file()) {
            Ok(document) => document,
            Err(error) => {
                tracing::warn!("ignoring unreadable usage document: {error:#}");
                None
            }
        };

        Ok(Self {
            emoji: veronica_core::EmojiCatalog::bundled(),
            directories,
            session: Mutex::new(session),
            settings: Mutex::new(settings),
            lid_awake: Mutex::new(None),
            prevent_sleep: Mutex::new(None),
            usage: Mutex::new(usage),
            sampler: Mutex::new(MetricsSampler::new()),
            refreshing: Mutex::new(false),
            notifications: Mutex::new(Vec::new()),
            companion_recording: Mutex::new(None),
        })
    }

    /// Record a notification, newest first, dropping the oldest past the limit.
    pub fn push_notification(&self, notification: veronica_system::Notification) {
        let mut list = self.notifications.lock().expect("notifications lock");
        list.insert(0, notification);
        list.truncate(veronica_system::notifications::HISTORY_LIMIT);
    }

    /// The configured skin tone, so the picker and `vr emoji` copy the same
    /// character.
    pub fn emoji_tone(&self) -> veronica_core::SkinTone {
        veronica_core::SkinTone::parse(
            self.settings_snapshot()
                .string("emojiSkinTone")
                .unwrap_or("standard"),
        )
    }

    pub fn settings_snapshot(&self) -> Settings {
        self.settings.lock().expect("settings lock").clone()
    }

    /// Write one setting through to disk, so the CLI and a restart both see it.
    pub fn set_setting(&self, key: &str, value: serde_json::Value) -> Result<()> {
        self.update_settings(|settings| {
            settings.set(key, value);
            Ok(())
        })
    }

    /// Change the settings file and the cached copy together.
    ///
    /// The change is applied to the file as it is now, not to the cached copy:
    /// the notch and `vr` write the same file, and the cache is up to two
    /// seconds behind them. Saving the cache put back whatever they had just
    /// changed, so a Keep Awake switched on from the notch reverted the moment
    /// the app saved any other setting.
    pub fn update_settings<T>(&self, change: impl FnOnce(&mut Settings) -> Result<T>) -> Result<T> {
        let mut cached = self.settings.lock().expect("settings lock");
        let path = self.directories.settings_file();
        let mut settings = match Settings::load(&path) {
            Ok(current) => current,
            // Unreadable rather than corrupt (a corrupt file loads as the
            // defaults): the cache is the best copy there is.
            Err(error) => {
                tracing::warn!("cannot re-read {}: {error:#}", path.display());
                cached.clone()
            }
        };
        let result = change(&mut settings)?;
        settings.save(&path)?;
        *cached = settings;
        Ok(result)
    }

    /// Refresh process-local caches after a backup import replaced files.
    pub fn reload_persistent_data(&self) -> Result<()> {
        let settings = Settings::load(&self.directories.settings_file())?;
        *self.settings.lock().expect("settings lock") = settings;
        let usage = veronica_usage::collector::read_document(&self.directories.usage_file())?;
        *self.usage.lock().expect("usage lock") = usage;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_in(root: &std::path::Path) -> AppState {
        let directories = AppDirectories {
            configuration: root.join("config"),
            data: root.join("data"),
            cache: root.join("cache"),
            runtime: root.join("runtime"),
            state: root.join("state"),
        };
        AppState::new(directories, veronica_core::DesktopSession::detect()).unwrap()
    }

    #[test]
    fn a_write_from_the_app_keeps_what_another_process_wrote() {
        let root = tempfile::tempdir().unwrap();
        let state = state_in(root.path());
        state
            .set_setting("appearance", serde_json::json!("dark"))
            .unwrap();

        // The notch, through `vr config set`, while the app's copy is stale.
        let path = state.directories.settings_file();
        let mut outside = Settings::load(&path).unwrap();
        outside.set("preventSleep", serde_json::json!(true));
        outside.save(&path).unwrap();

        state
            .set_setting("appearance", serde_json::json!("light"))
            .unwrap();

        let stored = Settings::load(&path).unwrap();
        assert_eq!(stored.get("preventSleep"), Some(&serde_json::json!(true)));
        assert_eq!(stored.string("appearance"), Some("light"));
        // And the cache now matches the file.
        assert_eq!(state.settings_snapshot(), stored);
    }

    #[test]
    fn a_change_that_fails_writes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let state = state_in(root.path());
        state
            .set_setting("appearance", serde_json::json!("dark"))
            .unwrap();
        let refused: Result<()> = state.update_settings(|settings| {
            settings.set("appearance", serde_json::json!("light"));
            anyhow::bail!("refused")
        });
        assert!(refused.is_err());
        let stored = Settings::load(&state.directories.settings_file()).unwrap();
        assert_eq!(stored.string("appearance"), Some("dark"));
    }
}
