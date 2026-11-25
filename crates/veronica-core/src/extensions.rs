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
    CalendarEvents, ClipboardHistory, CompanionService, DatabaseBroker, EmojiInsertion,
    ExternalMediaControl, GlobalPaste, GlobalShortcuts, HerdrSessions, InputSuppression,
    KeystrokeObservation, LocalMusicPlayback, LocalTerminal, MachineManagement, MediaControls,
    MicrophoneControl, Notifications, PackageManagement, PreventSleep, RunningApplications,
    ScreenColorSampling, ScreenShareDetection, ShellIntegration, SystemMetrics, UsageCollection,
    WindowDimming,
};
use ExtensionGroup as G;

pub const ENTRIES: &[ExtensionEntry] = &[
    ExtensionEntry {
        id: "attention",
        title: "Attention",
        subtitle: "Understand where your time goes and protect focused work!",
        icon: "hourglass",
        group: G::Utilities,
        featured: true,
        defaults_key: "tabAttentionEnabled",
        required_capabilities: &[RunningApplications],
        optional_capabilities: &[ShellIntegration],
        required_tools: &[],
    },
    ExtensionEntry {
        id: "usage",
        title: "Agent Usage",
        subtitle: "Claude and Codex limits, usage stats, and alerts.",
        icon: "bar-chart-3",
        group: G::Agent,
        featured: true,
        defaults_key: "tabUsageEnabled",
        required_capabilities: &[UsageCollection],
        optional_capabilities: &[Notifications],
        required_tools: &["claude", "codex"],
    },
    ExtensionEntry {
        id: "herdr",
        title: "Herdr",
        subtitle: "Live Herdr sessions on this computer and your SSH machines.",
        icon: "columns-3",
        group: G::Agent,
        featured: true,
        defaults_key: "tabHerdrEnabled",
        required_capabilities: &[HerdrSessions],
        optional_capabilities: &[],
        required_tools: &["herdr"],
    },
    ExtensionEntry {
        id: "quinjet",
        title: "Quinjet",
        subtitle: "Review pull requests and live workspace changes in a native terminal.",
        icon: "git-branch",
        group: G::Agent,
        featured: true,
        defaults_key: "tabQuinjetEnabled",
        required_capabilities: &[LocalTerminal],
        optional_capabilities: &[MachineManagement],
        required_tools: &["quinjet"],
    },
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
        id: "companion",
        title: "Companion",
        subtitle: "Your notes, voice memos and activity, remembered and searchable.",
        icon: "brain",
        group: G::Agent,
        featured: false,
        defaults_key: "tabCompanionEnabled",
        required_capabilities: &[CompanionService],
        optional_capabilities: &[],
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
        id: "music",
        title: "Music",
        subtitle: "Controls every player through MPRIS: Spotify, a browser tab, VLC.",
        icon: "music",
        group: G::Media,
        featured: false,
        defaults_key: "tabMusicEnabled",
        required_capabilities: &[ExternalMediaControl],
        optional_capabilities: &[LocalMusicPlayback, MediaControls],
        required_tools: &[],
    },
    ExtensionEntry {
        id: "calendar",
        title: "Calendar",
        subtitle: "Shows your schedule in the panel and the app.",
        icon: "calendar",
        group: G::Media,
        featured: false,
        defaults_key: "tabCalendarEnabled",
        required_capabilities: &[CalendarEvents],
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
        id: "clipboard",
        title: "Clipboard",
        subtitle: "Clipboard history, searchable, with one-click copy back.",
        icon: "clipboard-list",
        group: G::Utilities,
        featured: true,
        defaults_key: "clipboardEnabled",
        required_capabilities: &[ClipboardHistory],
        optional_capabilities: &[GlobalPaste, GlobalShortcuts, ShellIntegration],
        required_tools: &[],
    },
    ExtensionEntry {
        id: "keystrokeHighlight",
        title: "Keystroke Highlight",
        subtitle: "Show each key press on screen for polished demos.",
        icon: "keyboard",
        group: G::Utilities,
        featured: false,
        defaults_key: "keystrokeHighlightEnabled",
        required_capabilities: &[KeystrokeObservation],
        optional_capabilities: &[GlobalShortcuts],
        required_tools: &[],
    },
    ExtensionEntry {
        id: "focusDim",
        title: "Focus Dim",
        subtitle: "Dims everything behind the window you are working in.",
        icon: "contrast",
        group: G::Utilities,
        featured: false,
        defaults_key: "focusDimEnabled",
        required_capabilities: &[WindowDimming],
        optional_capabilities: &[],
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
    ExtensionEntry {
        id: "emoji",
        title: "Emoji Picker",
        subtitle: "Every emoji on a hotkey, straight into the app you are typing in.",
        icon: "smile",
        group: G::Utilities,
        featured: false,
        defaults_key: "emojiEnabled",
        required_capabilities: &[],
        optional_capabilities: &[EmojiInsertion, GlobalShortcuts],
        required_tools: &[],
    },
    ExtensionEntry {
        id: "colorPicker",
        title: "Color Picker",
        subtitle: "System loupe on a hotkey, sampled color to your clipboard.",
        icon: "pipette",
        group: G::Utilities,
        featured: false,
        defaults_key: "colorPickerEnabled",
        required_capabilities: &[ScreenColorSampling],
        optional_capabilities: &[GlobalShortcuts],
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

pub const LEGACY_TOGGLES: &[LegacyToggle] = &[LegacyToggle {
    id: "seoAudit",
    extension: "audit",
    defaults_key: "tabSEOAuditEnabled",
    featured: false,
}];

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

    fn caps(kind: SessionKind) -> Capabilities {
        gnome_caps(kind, false)
    }

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
            "attention",
            "usage",
            "herdr",
            "quinjet",
            "system",
            "appMaintenance",
            "machines",
            "database",
            "companion",
            "systemStats",
            "micMute",
            "lidAwake",
            "music",
            "calendar",
            "notchShelf",
            "clipboard",
            "keystrokeHighlight",
            "focusDim",
            "presenter",
            "emoji",
            "colorPicker",
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
        assert_eq!(successor("seoAudit"), Some("audit"));
        assert_eq!(successor("audit"), Some("audit"));
        assert_eq!(successor("usage"), None);
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
    fn usage_is_available_on_wayland_because_it_only_reads_files() {
        let entry = entry("usage").unwrap();
        assert_eq!(
            entry.availability(&caps(SessionKind::Wayland)),
            ExtensionAvailability::Available
        );
    }

    #[test]
    fn focus_dim_follows_the_shell_rather_than_the_display_protocol() {
        let entry = entry("focusDim").unwrap();
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
    fn music_is_available_with_mpris_and_local_playback() {
        let entry = entry("music").unwrap();
        assert_eq!(
            entry.availability(&caps(SessionKind::Wayland)),
            ExtensionAvailability::Available
        );
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
        let hits = filter("SSH", None);
        let ids: Vec<_> = hits.iter().map(|e| e.id).collect();
        assert!(ids.contains(&"machines"));
        assert!(ids.contains(&"herdr"));
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
