//! The extension catalogue.
//!
//! Ported from Edith's `ExtensionRegistry`: same ids, same groups, same defaults
//! keys. The user-visible wording changes where Edith's copy says "Mac" and
//! "menu bar" and Ubuntu says "computer" and "top bar".
//!
//! One entry's capabilities differ deliberately. Edith's Music plays a local
//! folder, so it *requires* local playback; on Ubuntu the same page drives every
//! player through MPRIS, so that is what it requires, with local playback
//! optional. Requiring a capability Veronica has not built would report the
//! working feature as unavailable, which is the opposite of honest.
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
    Degraded {
        missing: Vec<Capability>,
    },
    /// A required capability is missing, so the extension cannot run.
    Unavailable {
        missing: Vec<Capability>,
    },
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
        id: "attention",
        title: "Attention",
        subtitle: "Understand where your time goes and protect focused work!",
        icon: "hourglass",
        group: G::Utilities,
        featured: true,
        defaults_key: "tabAttentionEnabled",
        // Edith requires running applications, which macOS hands any app. On
        // Wayland only the compositor knows which window has focus, so the
        // shell extension is what makes the timeline real; without it the focus
        // timer and the history still work, which is why it is optional rather
        // than required.
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
        id: "seoAudit",
        title: "Site Audit",
        subtitle: "Crawl sitemaps, inspect page metadata, and keep every run local.",
        icon: "file-search",
        group: G::Utilities,
        featured: false,
        defaults_key: "tabSEOAuditEnabled",
        required_capabilities: &[SiteAuditing],
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
        // Edith's Music plays a local folder; on Ubuntu the same page drives
        // every player through MPRIS, which is the thing that actually exists
        // here. Local playback is listed as optional rather than required, so
        // the extension reports as working-but-partial instead of unavailable.
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
        // The picker itself only needs a clipboard, which always works, so
        // requiring insertion would report a working feature as unavailable.
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

/// Search and category filter, matching Edith's marketplace behaviour: an
/// empty query matches everything and matching is case-insensitive across
/// title and subtitle.
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

    /// Edith's catalogue, id for id. Listing them rather than counting them
    /// means a port that lands shows up here as a line, and the entries Veronica
    /// has not ported stay visible as the gap they are.
    #[test]
    fn the_catalogue_carries_ediths_ids() {
        let ported = [
            "attention",
            "usage",
            "herdr",
            "quinjet",
            "seoAudit",
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
        assert_eq!(ids, ported, "the catalogue drifted from Edith's order");
    }

    /// Every entry in Edith's catalogue now has an Ubuntu counterpart. This
    /// test is what would catch one being dropped rather than ported.
    #[test]
    fn nothing_edith_lists_is_missing_here() {
        assert_eq!(
            ENTRIES.len(),
            22,
            "Edith's catalogue has 22 entries and so does this one"
        );
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
        // The dimming is drawn inside GNOME Shell, so it is available on Wayland
        // — where no client could do it — and unavailable on a non-GNOME
        // desktop, whichever protocol that desktop runs.
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
