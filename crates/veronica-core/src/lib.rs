//! Portable core for Veronica.
//!
//! Everything here is free of GUI and toolkit dependencies so the desktop app,
//! the `vr` CLI and the tests can share one definition of paths, capabilities,
//! the extension catalogue and settings.

pub mod capabilities;
pub mod extensions;
pub mod paths;
pub mod session;
pub mod settings;

pub use capabilities::{Capabilities, Capability, CapabilityState};
pub use extensions::{ExtensionAvailability, ExtensionEntry, ExtensionGroup, ENTRIES};
pub use paths::{AppDirectories, APP_ID};
pub use session::{DesktopSession, SessionKind};
pub use settings::Settings;

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
