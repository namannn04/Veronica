//! Shared application state.

use std::sync::Mutex;

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
    /// The last collected usage document, so the UI can render immediately on
    /// launch instead of waiting for a refresh.
    pub usage: Mutex<Option<UsageDocument>>,
    /// Held across ticks because CPU percentages need two samples to exist.
    pub sampler: Mutex<MetricsSampler>,
    /// True while the collector is running, so overlapping refreshes are
    /// rejected rather than corrupting the output directory.
    pub refreshing: Mutex<bool>,
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
            directories,
            session: Mutex::new(session),
            settings: Mutex::new(settings),
            usage: Mutex::new(usage),
            sampler: Mutex::new(MetricsSampler::new()),
            refreshing: Mutex::new(false),
        })
    }

    pub fn settings_snapshot(&self) -> Settings {
        self.settings.lock().expect("settings lock").clone()
    }

    /// Write one setting through to disk, so the CLI and a restart both see it.
    pub fn set_setting(&self, key: &str, value: serde_json::Value) -> Result<()> {
        let mut settings = self.settings.lock().expect("settings lock");
        settings.set(key, value);
        settings.save(&self.directories.settings_file())?;
        Ok(())
    }
}
