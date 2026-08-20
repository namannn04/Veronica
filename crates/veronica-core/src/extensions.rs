//! The extension catalogue.
//!
//! Ported one-for-one from Edith's `ExtensionRegistry`: same ids, same groups,
//! same required and optional capabilities, same defaults keys. Only the user
//! visible wording changes, because Edith's copy says "Mac" and "menu bar"
//! where Ubuntu says "computer" and "top bar".
//!
//! Availability is derived, never stored: an extension is available when every
//! required capability is supported, degraded when only optional ones are
//! missing, and unavailable otherwise. That is what lets the same catalogue
//! describe both platforms honestly.

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
            ExtensionGroup::Agent => "Agent",
            ExtensionGroup::System => "System",
            ExtensionGroup::Media => "Media",
            ExtensionGroup::Utilities => "Utilities",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionEntry {
    pub id: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    /// Lucide icon name, the web equivalent of Edith's SF Symbol.
    pub icon: &'static str,
    pub group: ExtensionGroup,
    pub featured: bool,
    pub defaults_key: &'static str,
    pub required_capabilities: &'static [Capability],
    pub optional_capabilities: &'static [Capability],
    /// External binaries the extension needs; Veronica offers to install them.
    pub required_tools: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "availability", rename_all = "camelCase")]
pub enum ExtensionAvailability {
    Available,
    /// Works, but some optional capability is missing, so part of it is off.
    Degraded { missing: Vec<Capability> },
    /// A required capability is missing, so the extension cannot run.
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

use Capability::*;
use ExtensionGroup as G;

pub const ENTRIES: &[ExtensionEntry] = &[
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
        required_tools: &[],
    },
    ExtensionEntry {
        id: "system",
        title: "System",
        subtitle: "Running apps, prevent sleep, and the keyboard-cleaning lock.",
        icon: "toggle-right",
        group: G::System,
        featured: true,
        defaults_key: "tabSystemEnabled",
        required_capabilities: &[RunningApplications],
        optional_capabilities: &[PreventSleep, InputSuppression],
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
        id: "companion",
        title: "Companion",
        subtitle: "Your notes, voice memos and activity, remembered and searchable.",
        icon: "brain",
        group: G::Agent,
        featured: false,
        defaults_key: "tabCompanionEnabled",
        required_capabilities: &[CompanionService],
        optional_capabilities: &[],
        required_tools: &["docker"],
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
        subtitle: "Plays your local music folder, with media keys.",
        icon: "music",
        group: G::Media,
        featured: false,
        defaults_key: "tabMusicEnabled",
        required_capabilities: &[LocalMusicPlayback],
        optional_capabilities: &[MediaControls],
        required_tools: &["yt-dlp"],
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
        subtitle: "Clipboard history with instant paste.",
        icon: "clipboard-list",
        group: G::Utilities,
        featured: true,
        defaults_key: "clipboardEnabled",
        required_capabilities: &[ClipboardHistory],
        optional_capabilities: &[GlobalPaste, GlobalShortcuts],
        required_tools: &[],
    },
    ExtensionEntry {
        id: "focusDim",
        title: "Focus Dim",
        subtitle: "Dims everything behind your active app.",
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

/// Search and category filter, matching Edith's marketplace behaviour: an
/// empty query matches everything and matching is case-insensitive across
/// title and subtitle.
pub fn filter(
    query: &str,
    group: Option<ExtensionGroup>,
) -> Vec<&'static ExtensionEntry> {
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
        Capabilities::resolve(&DesktopSession {
            kind,
            has_global_shortcuts_portal: true,
            has_container_runtime: true,
            ..DesktopSession::unknown()
        })
    }

    #[test]
    fn catalogue_matches_ediths_fifteen_extensions() {
        assert_eq!(ENTRIES.len(), 15);
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
    fn focus_dim_is_unavailable_on_wayland_and_available_on_x11() {
        let entry = entry("focusDim").unwrap();
        assert!(matches!(
            entry.availability(&caps(SessionKind::Wayland)),
            ExtensionAvailability::Unavailable { .. }
        ));
        assert_eq!(
            entry.availability(&caps(SessionKind::X11)),
            ExtensionAvailability::Available
        );
    }

    #[test]
    fn system_is_degraded_not_unavailable_when_only_an_optional_capability_is_missing() {
        // InputSuppression is optional for `system` and needs the input group,
        // so the extension must still run with the lock switched off.
        let entry = entry("system").unwrap();
        match entry.availability(&caps(SessionKind::Wayland)) {
            ExtensionAvailability::Degraded { missing } => {
                assert_eq!(missing, vec![InputSuppression]);
            }
            other => panic!("expected degraded, got {other:?}"),
        }
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
