use serde::{Deserialize, Serialize};

use crate::capabilities::{Capabilities, Capability};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionGroup {
    Agent,
    System,
    Media,
    Utilities,
}

impl ExtensionGroup {
    pub fn title(self) -> &'static str {
        match self {
            Self::Agent => "Agent",
            Self::System => "System",
            Self::Media => "Media",
            Self::Utilities => "Utilities",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionEntry {
    pub id: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub icon: &'static str,
    pub group: ExtensionGroup,
    pub featured: bool,
    pub defaults_key: &'static str,
    pub required_capabilities: &'static [Capability],
    pub optional_capabilities: &'static [Capability],
    pub required_tools: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "availability", rename_all = "camelCase")]
pub enum ExtensionAvailability {
    Available,
    Degraded { missing: Vec<Capability> },
    Unavailable { missing: Vec<Capability> },
}

impl ExtensionEntry {
    pub fn availability(&self, capabilities: &Capabilities) -> ExtensionAvailability {
        let missing_required: Vec<Capability> = self
            .required_capabilities
            .iter()
            .copied()
            .filter(|cap| !capabilities.is_supported(*cap))
            .collect();

        if !missing_required.is_empty() {
            return ExtensionAvailability::Unavailable {
                missing: missing_required,
            };
        }

        let missing_optional: Vec<Capability> = self
            .optional_capabilities
            .iter()
            .copied()
            .filter(|cap| !capabilities.is_supported(*cap))
            .collect();

        if missing_optional.is_empty() {
            ExtensionAvailability::Available
        } else {
            ExtensionAvailability::Degraded {
                missing: missing_optional,
            }
        }
    }
}

use Capability::{
    DatabaseBroker, ExternalMediaControl, GlobalShortcuts, InputSuppression, MachineManagement,
    MicrophoneControl, Notifications, PackageManagement, PreventSleep, RunningApplications,
    ScreenShareDetection, ShellIntegration, SystemMetrics,
};
use ExtensionGroup as G;

pub const ENTRIES: &[ExtensionEntry] = &[
    ExtensionEntry {
        id: "system",
        title: "System",
        subtitle: "Running apps, listening ports, Docker, sleep controls and keyboard cleaning.",
        icon: "toggle-right",
        group: G::System,
        featured: true,
        defaults_key: "tabSystemEnabled",
        required_capabilities: &[RunningApplications],
        optional_capabilities: &[PreventSleep, InputSuppression],
        required_tools: &[],
    },
    ExtensionEntry {
        id: "appMaintenance",
        title: "App Maintenance",
        subtitle: "Packages, updates across apt, snap and flatpak, and review-first removal.",
        icon: "package",
        group: G::System,
        featured: false,
        defaults_key: "tabAppMaintenanceEnabled",
        required_capabilities: &[PackageManagement],
        optional_capabilities: &[Notifications],
        required_tools: &[],
    },
    ExtensionEntry {
        id: "machines",
        title: "Machines",
        subtitle: "Your other computers over SSH: stats, files, Docker, and a terminal.",
        icon: "server",
        group: G::System,
        featured: true,
        defaults_key: "tabMachinesEnabled",
        required_capabilities: &[MachineManagement],
        optional_capabilities: &[Notifications],
        required_tools: &["ssh"],
    },
    ExtensionEntry {
        id: "database",
        title: "Database",
        subtitle: "Explore databases and run guarded production mutations.",
        icon: "database",
        group: G::System,
        featured: false,
        defaults_key: "tabDatabaseEnabled",
        required_capabilities: &[DatabaseBroker],
        optional_capabilities: &[MachineManagement],
        required_tools: &[],
    },
    ExtensionEntry {
        id: "systemStats",
        title: "CPU & Memory in the top bar",
        subtitle: "Live CPU and memory readout as a tray indicator.",
        icon: "gauge",
        group: G::System,
        featured: false,
        defaults_key: "menuBarSystemStats",
        required_capabilities: &[SystemMetrics],
        optional_capabilities: &[],
        required_tools: &[],
    },
    ExtensionEntry {
        id: "micMute",
        title: "Mic Mute",
        subtitle: "Mute every microphone system-wide from the tray or a shortcut.",
        icon: "mic-off",
        group: G::System,
        featured: false,
        defaults_key: "micMuteEnabled",
        required_capabilities: &[MicrophoneControl],
        optional_capabilities: &[GlobalShortcuts],
        required_tools: &[],
    },
    ExtensionEntry {
        id: "lidAwake",
        title: "Lid Awake",
        subtitle: "Keeps this computer running with the lid shut, on battery and unplugged.",
        icon: "laptop",
        group: G::System,
        featured: false,
        defaults_key: "lidAwakeEnabled",
        required_capabilities: &[PreventSleep],
        optional_capabilities: &[],
        required_tools: &[],
    },
    ExtensionEntry {
        id: "notchShelf",
        title: "Top bar",
        subtitle: "Agent usage and machine load inside the top bar's own clock dropdown.",
        icon: "inbox",
        group: G::Media,
        featured: true,
        defaults_key: "notchShelfEnabled",
        required_capabilities: &[ShellIntegration],
        optional_capabilities: &[ExternalMediaControl],
        required_tools: &[],
    },
    ExtensionEntry {
        id: "presenter",
        title: "Presenter",
        subtitle: "Blurs sensitive numbers while sharing your screen.",
        icon: "theater",
        group: G::Utilities,
        featured: false,
        defaults_key: "presenterEnabled",
        required_capabilities: &[ScreenShareDetection],
        optional_capabilities: &[],
        required_tools: &[],
    },
];

pub fn entry(id: &str) -> Option<&'static ExtensionEntry> {
    ENTRIES.iter().find(|e| e.id == id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyToggle {
    pub id: &'static str,
    pub extension: &'static str,
    pub defaults_key: &'static str,
    pub featured: bool,
}

pub const LEGACY_TOGGLES: &[LegacyToggle] = &[
    LegacyToggle {
        id: "focusDim",
        extension: "focus-dim",
        defaults_key: "focusDimEnabled",
        featured: false,
    },
    LegacyToggle {
        id: "companion",
        extension: "companion",
        defaults_key: "tabCompanionEnabled",
        featured: false,
    },
    LegacyToggle {
        id: "keystrokeHighlight",
        extension: "keystroke-highlight",
        defaults_key: "keystrokeHighlightEnabled",
        featured: false,
    },
    LegacyToggle {
        id: "clipboard",
        extension: "clipboard",
        defaults_key: "clipboardEnabled",
        featured: true,
    },
    LegacyToggle {
        id: "attention",
        extension: "attention",
        defaults_key: "tabAttentionEnabled",
        featured: true,
    },
    LegacyToggle {
        id: "herdr",
        extension: "herdr",
        defaults_key: "tabHerdrEnabled",
        featured: true,
    },
    LegacyToggle {
        id: "quinjet",
        extension: "quinjet",
        defaults_key: "tabQuinjetEnabled",
        featured: true,
    },
    LegacyToggle {
        id: "usage",
        extension: "usage",
        defaults_key: "tabUsageEnabled",
        featured: true,
    },
    LegacyToggle {
        id: "seoAudit",
        extension: "audit",
        defaults_key: "tabSEOAuditEnabled",
        featured: false,
    },
    LegacyToggle {
        id: "calendar",
        extension: "calendar",
        defaults_key: "tabCalendarEnabled",
        featured: false,
    },
    LegacyToggle {
        id: "music",
        extension: "media",
        defaults_key: "tabMusicEnabled",
        featured: false,
    },
    LegacyToggle {
        id: "colorPicker",
        extension: "color",
        defaults_key: "colorPickerEnabled",
        featured: false,
    },
    LegacyToggle {
        id: "emoji",
        extension: "emoji",
        defaults_key: "emojiEnabled",
        featured: false,
    },
];

pub fn successor(id: &str) -> Option<&'static str> {
    LEGACY_TOGGLES
        .iter()
        .find(|toggle| toggle.id == id || toggle.extension == id)
        .map(|toggle| toggle.extension)
}

pub fn filter(query: &str, group: Option<ExtensionGroup>) -> Vec<&'static ExtensionEntry> {
    let needle = query.trim().to_lowercase();
    ENTRIES
        .iter()
        .filter(|entry| group.is_none_or(|g| entry.group == g))
        .filter(|entry| {
            needle.is_empty()
                || entry.title.to_lowercase().contains(&needle)
                || entry.subtitle.to_lowercase().contains(&needle)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{DesktopSession, SessionKind};

    fn gnome_caps(kind: SessionKind, is_gnome: bool) -> Capabilities {
        Capabilities::resolve(&DesktopSession {
            kind,
            is_gnome,
            has_global_shortcuts_portal: true,
            has_container_runtime: true,
            ..DesktopSession::unknown()
        })
    }

    #[test]
    fn the_catalogue_keeps_its_order() {
        let ported = [
            "system",
            "appMaintenance",
            "machines",
            "database",
            "systemStats",
            "micMute",
            "lidAwake",
            "notchShelf",
            "presenter",
        ];
        let ids: Vec<&str> = ENTRIES.iter().map(|entry| entry.id).collect();
        assert_eq!(ids, ported, "the catalogue order drifted");
    }

    #[test]
    fn migrated_features_leave_the_built_in_catalogue() {
        for toggle in LEGACY_TOGGLES {
            assert!(
                entry(toggle.id).is_none(),
                "{} is still built in",
                toggle.id
            );
            assert!(
                ENTRIES
                    .iter()
                    .all(|entry| entry.defaults_key != toggle.defaults_key),
                "{} reuses a built-in toggle",
                toggle.id
            );
        }
        for toggle in LEGACY_TOGGLES {
            assert_eq!(successor(toggle.id), Some(toggle.extension));
            assert_eq!(successor(toggle.extension), Some(toggle.extension));
        }
        for entry in ENTRIES {
            assert_eq!(successor(entry.id), None, "{}", entry.id);
        }
    }

    #[test]
    fn ids_and_defaults_keys_are_unique() {
        for field in [
            ENTRIES.iter().map(|e| e.id).collect::<Vec<_>>(),
            ENTRIES.iter().map(|e| e.defaults_key).collect::<Vec<_>>(),
        ] {
            let mut sorted = field.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), field.len(), "duplicate entry in {field:?}");
        }
    }

    #[test]
    fn focus_dim_follows_the_shell_rather_than_the_display_protocol() {
        let entry = ExtensionEntry {
            id: "dimmer",
            title: "Dimmer",
            subtitle: "Needs window dimming.",
            icon: "contrast",
            group: G::Utilities,
            featured: false,
            defaults_key: "dimmerEnabled",
            required_capabilities: &[Capability::WindowDimming],
            optional_capabilities: &[],
            required_tools: &[],
        };
        for kind in [SessionKind::Wayland, SessionKind::X11] {
            assert_eq!(
                entry.availability(&gnome_caps(kind, true)),
                ExtensionAvailability::Available,
                "GNOME {kind:?}"
            );
            assert!(
                matches!(
                    entry.availability(&gnome_caps(kind, false)),
                    ExtensionAvailability::Unavailable { .. }
                ),
                "non-GNOME {kind:?}"
            );
        }
    }

    #[test]
    fn system_is_available_on_gnome_with_compositor_input_suppression() {
        let entry = entry("system").unwrap();
        assert_eq!(
            entry.availability(&gnome_caps(SessionKind::Wayland, true)),
            ExtensionAvailability::Available
        );
    }

    #[test]
    fn filter_matches_subtitle_case_insensitively() {
        let first = ENTRIES.first().unwrap();
        let word = first
            .subtitle
            .split_whitespace()
            .max_by_key(|word| word.len())
            .unwrap()
            .trim_matches(|character: char| !character.is_alphanumeric());
        let needle = word.to_lowercase();
        let hits = filter(&word.to_uppercase(), None);
        assert!(hits.iter().any(|entry| entry.id == first.id));
        assert!(hits.iter().all(|entry| {
            entry.title.to_lowercase().contains(&needle)
                || entry.subtitle.to_lowercase().contains(&needle)
        }));
    }

    #[test]
    fn empty_query_returns_the_whole_group() {
        assert_eq!(
            filter("", Some(G::Utilities)).len(),
            ENTRIES.iter().filter(|e| e.group == G::Utilities).count()
        );
        assert_eq!(filter("   ", None).len(), ENTRIES.len());
    }
}
