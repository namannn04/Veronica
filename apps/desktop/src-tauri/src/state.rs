use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use veronica_core::{AppDirectories, DesktopSession, Settings};
use veronica_system::MetricsSampler;

pub(crate) struct AppState {
    pub chat: veronica_system::companion_ai::ChatRuntime,
    pub agents: veronica_system::agents::AgentManager,
    pub directories: AppDirectories,
    pub session: Mutex<DesktopSession>,
    pub settings: Mutex<Settings>,
    pub lid_awake: Mutex<Option<veronica_system::power::InhibitorLock>>,
    pub prevent_sleep: Mutex<Option<veronica_system::power::InhibitorLock>>,
    pub sampler: Mutex<MetricsSampler>,
    pub notifications: Mutex<Vec<veronica_system::Notification>>,
    pub companion_recording: Mutex<Option<CompanionRecording>>,
    pub emoji: veronica_core::EmojiCatalog,
    pub extensions: Arc<crate::extensions::host::Host>,
}

pub(crate) struct CompanionRecording {
    pub child: Child,
    pub path: PathBuf,
    pub started: Instant,
}

impl AppState {
    pub(crate) fn new(directories: AppDirectories, session: DesktopSession) -> Result<Self> {
        let settings = Settings::load(&directories.settings_file())?;
        Ok(Self {
            chat: veronica_system::companion_ai::ChatRuntime::new(
                directories.data.join("companion/chat"),
            ),
            agents: veronica_system::agents::AgentManager::default(),
            emoji: veronica_core::EmojiCatalog::bundled(),
            extensions: Arc::new(crate::extensions::host::Host::new(&directories)),
            directories,
            session: Mutex::new(session),
            settings: Mutex::new(settings),
            lid_awake: Mutex::new(None),
            prevent_sleep: Mutex::new(None),
            sampler: Mutex::new(MetricsSampler::new()),
            notifications: Mutex::new(Vec::new()),
            companion_recording: Mutex::new(None),
        })
    }

    pub(crate) fn push_notification(&self, notification: veronica_system::Notification) {
        let mut list = self.notifications.lock().expect("notifications lock");
        list.insert(0, notification);
        list.truncate(veronica_system::notifications::HISTORY_LIMIT);
    }

    pub(crate) fn emoji_tone(&self) -> veronica_core::SkinTone {
        veronica_core::SkinTone::parse(
            self.settings_snapshot()
                .string("ext.emoji.skinTone")
                .unwrap_or("standard"),
        )
    }

    pub(crate) fn settings_snapshot(&self) -> Settings {
        self.settings.lock().expect("settings lock").clone()
    }

    pub(crate) fn set_setting(&self, key: &str, value: serde_json::Value) -> Result<()> {
        self.update_settings(|settings| {
            settings.set(key, value);
            Ok(())
        })
    }

    pub(crate) fn update_settings<T>(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<T>,
    ) -> Result<T> {
        let mut cached = self.settings.lock().expect("settings lock");
        let path = self.directories.settings_file();
        let mut settings = match Settings::load(&path) {
            Ok(current) => current,
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

    pub(crate) fn reload_persistent_data(&self) -> Result<()> {
        let settings = Settings::load(&self.directories.settings_file())?;
        *self.settings.lock().expect("settings lock") = settings;
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
        AppState::new(directories, DesktopSession::detect()).unwrap()
    }

    #[test]
    fn a_write_from_the_app_keeps_what_another_process_wrote() {
        let root = tempfile::tempdir().unwrap();
        let state = state_in(root.path());
        state
            .set_setting("appearance", serde_json::json!("dark"))
            .unwrap();

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
