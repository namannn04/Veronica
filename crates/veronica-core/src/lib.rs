//! Portable core for Veronica.
//!
//! Everything here is free of GUI and toolkit dependencies so the desktop app,
//! the `vr` CLI and the tests can share one definition of paths, capabilities,
//! the extension catalogue and settings.

pub mod attention;
pub mod backup;
pub mod capabilities;
pub mod clipboard;
pub mod companion;
pub mod extensions;
pub mod focus_dim;
pub mod paths;
pub mod presenter;
pub mod session;
pub mod settings;
pub mod swatches;

pub use attention::{
    AttentionCategory, AttentionEvent, AttentionFocusSession, AttentionOverview, AttentionPrivacy,
    AttentionRepository, AttentionSettings, AttentionStatus,
};
pub use backup::{BackupArchive, BackupManifest, ImportReport};
pub use capabilities::{Capabilities, Capability, CapabilityState};
pub use clipboard::{ClipEntry, ClipboardHistory};
pub use companion::{CompanionItem, CompanionKind, CompanionRepository};
pub use extensions::{ExtensionAvailability, ExtensionEntry, ExtensionGroup, ENTRIES};
pub use focus_dim::{DisplayMode, FocusDimSettings};
pub use paths::{AppDirectories, APP_ID};
pub use presenter::{BlurCategory, PresenterState};
pub use session::{DesktopSession, SessionKind};
pub use settings::Settings;
pub use swatches::{ColorProfile, CopyFormat, Swatch, SwatchHistory};

/// Version of the running build, from Cargo.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One resolved snapshot of the environment, built once at launch and handed to
/// the UI so every screen agrees on what the platform can do.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub version: &'static str,
    pub session: DesktopSession,
    pub directories: DirectoryReport,
    pub capabilities: Capabilities,
    pub extensions: Vec<ExtensionReport>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryReport {
    pub configuration: String,
    pub data: String,
    pub cache: String,
    pub state: String,
    pub runtime: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionReport {
    pub id: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub icon: &'static str,
    /// The settings key holding this extension's on/off state. Sent to the
    /// interface so the catalogue stays the single source of it.
    pub defaults_key: &'static str,
    pub group: ExtensionGroup,
    pub featured: bool,
    pub enabled: bool,
    #[serde(flatten)]
    pub availability: ExtensionAvailability,
}

impl Diagnostics {
    pub fn collect(
        directories: &AppDirectories,
        session: DesktopSession,
        settings: &Settings,
    ) -> Self {
        let capabilities = Capabilities::resolve(&session);
        let extensions = ENTRIES
            .iter()
            .map(|entry| ExtensionReport {
                id: entry.id,
                title: entry.title,
                subtitle: entry.subtitle,
                icon: entry.icon,
                defaults_key: entry.defaults_key,
                group: entry.group,
                featured: entry.featured,
                enabled: settings.extension_enabled(entry),
                availability: entry.availability(&capabilities),
            })
            .collect();

        Self {
            version: VERSION,
            session,
            directories: DirectoryReport {
                configuration: directories.configuration.display().to_string(),
                data: directories.data.display().to_string(),
                cache: directories.cache.display().to_string(),
                state: directories.state.display().to_string(),
                runtime: directories.runtime.display().to_string(),
            },
            capabilities,
            extensions,
        }
    }
}
