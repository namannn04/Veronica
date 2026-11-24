use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::session::{DesktopSession, SessionKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    ApplicationAudio,
    BluetoothMonitoring,
    CalendarEvents,
    CameraPreview,
    ClipboardHistory,
    CompanionService,
    DatabaseBroker,
    EmojiInsertion,
    ExternalMediaControl,
    FileShelf,
    GlobalPaste,
    HerdrSessions,
    GlobalShortcuts,
    InputSuppression,
    KeystrokeObservation,
    LocalMusicPlayback,
    LocalTerminal,
    MachineManagement,
    MediaControls,
    MicrophoneControl,
    Notifications,
    PackageManagement,
    PreventSleep,
    RunningApplications,
    ScreenColorSampling,
    ScreenShareDetection,
    ShellIntegration,
    SiteAuditing,
    SystemMetrics,
    UsageCollection,
    WindowDimming,
}

impl Capability {
    pub const ALL: [Self; 31] = [
        Self::ApplicationAudio,
        Self::BluetoothMonitoring,
        Self::CalendarEvents,
        Self::CameraPreview,
        Self::ClipboardHistory,
        Self::CompanionService,
        Self::DatabaseBroker,
        Self::EmojiInsertion,
        Self::ExternalMediaControl,
        Self::FileShelf,
        Self::GlobalPaste,
        Self::HerdrSessions,
        Self::GlobalShortcuts,
        Self::InputSuppression,
        Self::KeystrokeObservation,
        Self::LocalMusicPlayback,
        Self::LocalTerminal,
        Self::MachineManagement,
        Self::MediaControls,
        Self::MicrophoneControl,
        Self::Notifications,
        Self::PackageManagement,
        Self::PreventSleep,
        Self::RunningApplications,
        Self::ScreenColorSampling,
        Self::ScreenShareDetection,
        Self::ShellIntegration,
        Self::SiteAuditing,
        Self::SystemMetrics,
        Self::UsageCollection,
        Self::WindowDimming,
    ];

    pub fn title(self) -> &'static str {
        use Capability::{
            ApplicationAudio, BluetoothMonitoring, CalendarEvents, CameraPreview, ClipboardHistory,
            CompanionService, DatabaseBroker, EmojiInsertion, ExternalMediaControl, FileShelf,
            GlobalPaste, GlobalShortcuts, HerdrSessions, InputSuppression, KeystrokeObservation,
            LocalMusicPlayback, LocalTerminal, MachineManagement, MediaControls, MicrophoneControl,
            Notifications, PackageManagement, PreventSleep, RunningApplications,
            ScreenColorSampling, ScreenShareDetection, ShellIntegration, SiteAuditing,
            SystemMetrics, UsageCollection, WindowDimming,
        };
        match self {
            ApplicationAudio => "Per-app audio",
            BluetoothMonitoring => "Bluetooth monitoring",
            CalendarEvents => "Calendar events",
            CameraPreview => "Camera preview",
            ClipboardHistory => "Clipboard history",
            CompanionService => "Companion service",
            DatabaseBroker => "Database access",
            EmojiInsertion => "Emoji insertion",
            ExternalMediaControl => "External media control",
            FileShelf => "File shelf",
            GlobalPaste => "Paste in place",
            HerdrSessions => "Herdr sessions",
            GlobalShortcuts => "Global shortcuts",
            InputSuppression => "Input suppression",
            KeystrokeObservation => "Keystroke observation",
            LocalMusicPlayback => "Local music playback",
            LocalTerminal => "Local terminal",
            MachineManagement => "Machine management",
            MediaControls => "Media keys",
            MicrophoneControl => "Microphone control",
            Notifications => "Notifications",
            PackageManagement => "Package management",
            PreventSleep => "Prevent sleep",
            RunningApplications => "Running applications",
            ScreenColorSampling => "Screen colour sampling",
            ScreenShareDetection => "Screen share detection",
            ShellIntegration => "Top bar integration",
            SiteAuditing => "Site auditing",
            SystemMetrics => "System metrics",
            UsageCollection => "Usage collection",
            WindowDimming => "Window dimming",
        }
    }

    pub fn backend(self) -> &'static str {
        use Capability::{
            ApplicationAudio, BluetoothMonitoring, CalendarEvents, CameraPreview, ClipboardHistory,
            CompanionService, DatabaseBroker, EmojiInsertion, ExternalMediaControl, FileShelf,
            GlobalPaste, GlobalShortcuts, HerdrSessions, InputSuppression, KeystrokeObservation,
            LocalMusicPlayback, LocalTerminal, MachineManagement, MediaControls, MicrophoneControl,
            Notifications, PackageManagement, PreventSleep, RunningApplications,
            ScreenColorSampling, ScreenShareDetection, ShellIntegration, SiteAuditing,
            SystemMetrics, UsageCollection, WindowDimming,
        };
        match self {
            ApplicationAudio | MicrophoneControl => "PipeWire",
            BluetoothMonitoring => "BlueZ (D-Bus)",
            CalendarEvents => "Evolution Data Server (D-Bus)",
            CameraPreview => "xdg-desktop-portal Camera",
            ClipboardHistory => "Wayland data-control / GNOME Shell",
            CompanionService => "Local XDG data + PipeWire",
            DatabaseBroker => "Secret Service + SQLite",
            EmojiInsertion => "Clipboard + GNOME Shell extension",
            ExternalMediaControl | MediaControls => "MPRIS2 (D-Bus)",
            FileShelf => "In-process",
            GlobalPaste | KeystrokeObservation | ShellIntegration => "GNOME Shell extension",
            HerdrSessions | UsageCollection => "Filesystem",
            GlobalShortcuts => "xdg-desktop-portal GlobalShortcuts",
            InputSuppression => "libinput / evdev",
            LocalMusicPlayback => "GStreamer",
            LocalTerminal => "GNOME Console / x-terminal-emulator",
            MachineManagement => "OpenSSH",
            Notifications => "org.freedesktop.Notifications",
            PackageManagement => "apt / snap / flatpak",
            PreventSleep => "systemd-logind inhibitor",
            RunningApplications => "procfs + desktop entries",
            ScreenColorSampling => "xdg-desktop-portal Screenshot.PickColor",
            ScreenShareDetection => "xdg-desktop-portal ScreenCast",
            SiteAuditing => "HTTPS",
            SystemMetrics => "procfs / sysfs",
            WindowDimming => "Compositor",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum CapabilityState {
    Available,
    PermissionRequired { reason: String },
    IntegrationRequired { reason: String },
    Unsupported { reason: String },
}

impl CapabilityState {
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Available | Self::PermissionRequired { .. })
    }

    fn permission(reason: &str) -> Self {
        Self::PermissionRequired {
            reason: reason.to_string(),
        }
    }

    fn integration(reason: &str) -> Self {
        Self::IntegrationRequired {
            reason: reason.to_string(),
        }
    }

    fn unsupported(reason: &str) -> Self {
        Self::Unsupported {
            reason: reason.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub states: BTreeMap<Capability, CapabilityState>,
}

impl Capabilities {
    pub fn state(&self, capability: Capability) -> &CapabilityState {
        static UNKNOWN: LazyLock<CapabilityState> =
            LazyLock::new(|| CapabilityState::Unsupported {
                reason: "Capability has no platform implementation.".to_string(),
            });
        self.states.get(&capability).unwrap_or(&UNKNOWN)
    }

    pub fn is_supported(&self, capability: Capability) -> bool {
        self.states
            .get(&capability)
            .is_some_and(CapabilityState::is_supported)
    }

    pub fn resolve(session: &DesktopSession) -> Self {
        use Capability::{
            ApplicationAudio, BluetoothMonitoring, CalendarEvents, CameraPreview, ClipboardHistory,
            CompanionService, DatabaseBroker, EmojiInsertion, ExternalMediaControl, FileShelf,
            GlobalPaste, GlobalShortcuts, HerdrSessions, InputSuppression, KeystrokeObservation,
            LocalMusicPlayback, LocalTerminal, MachineManagement, MediaControls, MicrophoneControl,
            Notifications, PackageManagement, PreventSleep, RunningApplications,
            ScreenColorSampling, ScreenShareDetection, ShellIntegration, SiteAuditing,
            SystemMetrics, UsageCollection, WindowDimming,
        };
        let wayland = session.kind == SessionKind::Wayland;
        let mut states = BTreeMap::new();

        let mut set = |capability: Capability, state: CapabilityState| {
            states.insert(capability, state);
        };

        set(UsageCollection, CapabilityState::Available);
        set(HerdrSessions, CapabilityState::Available);
        set(FileShelf, CapabilityState::Available);
        set(SystemMetrics, CapabilityState::Available);
        set(MachineManagement, CapabilityState::Available);
        set(LocalTerminal, CapabilityState::Available);

        set(Notifications, CapabilityState::Available);
        set(ExternalMediaControl, CapabilityState::Available);
        set(MediaControls, CapabilityState::Available);
        set(LocalMusicPlayback, CapabilityState::Available);
        set(PreventSleep, CapabilityState::Available);
        set(ApplicationAudio, CapabilityState::Available);
        set(MicrophoneControl, CapabilityState::Available);
        set(BluetoothMonitoring, CapabilityState::Available);
        set(CalendarEvents, CapabilityState::Available);
        set(RunningApplications, CapabilityState::Available);
        set(ScreenShareDetection, CapabilityState::Available);
        set(SiteAuditing, CapabilityState::Available);

        set(CompanionService, CapabilityState::Available);

        set(
            DatabaseBroker,
            CapabilityState::permission(
                "Database passwords go to the desktop keyring, which asks you to unlock it \
                 on first use. Without a keyring they fall back to a 0600 file.",
            ),
        );

        set(
            PackageManagement,
            CapabilityState::permission(
                "Installing or removing a package asks for authentication through pkexec. \
                 Listing what is installed and what could be updated needs nothing.",
            ),
        );

        set(
            CameraPreview,
            CapabilityState::permission("Approve camera access when Veronica asks."),
        );
        set(
            ScreenColorSampling,
            CapabilityState::permission(
                "The colour picker asks the desktop portal for one screen sample per pick.",
            ),
        );
        set(GlobalShortcuts, CapabilityState::Available);
        set(
            GlobalPaste,
            if session.is_gnome {
                CapabilityState::permission(
                    "Enable Veronica's GNOME Shell extension, which synthesises the key \
                     press inside the compositor.",
                )
            } else {
                CapabilityState::integration(
                    "Synthesising a key press into a window Veronica does not own is the \
                     compositor's job, and doing it needs a shell extension. This desktop \
                     is not GNOME.",
                )
            },
        );

        set(
            ClipboardHistory,
            if !wayland {
                CapabilityState::Available
            } else if session.is_gnome {
                CapabilityState::permission(
                    "Enable the Veronica GNOME Shell extension, which captures copies \
                     from inside the compositor.",
                )
            } else {
                CapabilityState::integration(
                    "A Wayland compositor only hands the clipboard to the focused window, \
                     and capturing needs a shell extension. This desktop is not GNOME.",
                )
            },
        );
        set(
            WindowDimming,
            if session.is_gnome {
                CapabilityState::permission(
                    "Enable the Veronica GNOME Shell extension, which draws the dimming \
                     inside the compositor.",
                )
            } else {
                CapabilityState::integration(
                    "Dimming behind another app's window needs the compositor, and on this \
                     desktop nothing exposes that. GNOME can, through Veronica's shell \
                     extension.",
                )
            },
        );
        set(
            ShellIntegration,
            if session.is_gnome {
                CapabilityState::permission(
                    "Log out and back in once after installing, so GNOME Shell loads the \
                     extension.",
                )
            } else {
                CapabilityState::unsupported(
                    "Adding sections to the top bar needs a GNOME Shell extension; this \
                     desktop is not GNOME.",
                )
            },
        );

        set(
            EmojiInsertion,
            if session.is_gnome {
                CapabilityState::permission(
                    "Enable Veronica's GNOME Shell extension to type the emoji straight \
                     into the app you were in. Copying to the clipboard needs nothing.",
                )
            } else {
                CapabilityState::integration(
                    "Typing the emoji into the app you were in needs a shell extension; \
                     this desktop is not GNOME. The picker still copies to the clipboard.",
                )
            },
        );

        set(
            KeystrokeObservation,
            if session.is_gnome {
                CapabilityState::integration(
                    "GNOME hands an application's key presses straight to it, so the \
                     shell extension can only show keys typed into the shell itself, such \
                     as Activities search. Showing every key would need read access to the \
                     keyboard device (the input group), which Veronica does not take.",
                )
            } else {
                CapabilityState::integration(
                    "A Wayland compositor hands key presses only to the focused window, so \
                     showing them on screen needs a shell extension. This desktop is not \
                     GNOME.",
                )
            },
        );

        set(
            InputSuppression,
            if session.is_gnome {
                CapabilityState::permission(
                    "Enable Veronica's GNOME extension; Shell takes a compositor modal grab \
                     while keyboard cleaning mode is open.",
                )
            } else {
                CapabilityState::integration(
                    "Keyboard cleaning mode currently needs Veronica's GNOME Shell extension.",
                )
            },
        );

        if session.kind == SessionKind::Headless {
            for capability in [
                CameraPreview,
                ClipboardHistory,
                EmojiInsertion,
                KeystrokeObservation,
                GlobalPaste,
                GlobalShortcuts,
                RunningApplications,
                ScreenColorSampling,
                ScreenShareDetection,
                WindowDimming,
                InputSuppression,
                ShellIntegration,
            ] {
                set(
                    capability,
                    CapabilityState::unsupported(
                        "No graphical session; this needs a desktop to be logged in.",
                    ),
                );
            }
        }

        Self { states }
    }

    pub fn needing_attention(&self) -> Vec<(Capability, &CapabilityState)> {
        self.states
            .iter()
            .filter(|(_, state)| !matches!(state, CapabilityState::Available))
            .map(|(cap, state)| (*cap, state))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(kind: SessionKind) -> DesktopSession {
        DesktopSession {
            kind,
            desktop: "GNOME".into(),
            has_global_shortcuts_portal: true,
            has_container_runtime: true,
            ..DesktopSession::unknown()
        }
    }

    #[test]
    fn usage_collection_is_available_because_it_only_reads_files() {
        let caps = Capabilities::resolve(&session(SessionKind::Wayland));
        assert_eq!(
            caps.state(Capability::UsageCollection),
            &CapabilityState::Available
        );
    }

    #[test]
    fn window_dimming_follows_the_shell_not_the_display_protocol() {
        for kind in [SessionKind::Wayland, SessionKind::X11] {
            let mut gnome = session(kind);
            gnome.is_gnome = true;
            assert!(
                Capabilities::resolve(&gnome).is_supported(Capability::WindowDimming),
                "GNOME {kind:?} should support dimming through the extension"
            );

            let mut other = session(kind);
            other.is_gnome = false;
            assert!(matches!(
                Capabilities::resolve(&other).state(Capability::WindowDimming),
                CapabilityState::IntegrationRequired { .. }
            ));
        }
    }

    #[test]
    fn keystroke_highlight_is_not_offered_as_working_on_gnome() {
        for kind in [SessionKind::Wayland, SessionKind::X11] {
            let mut gnome = session(kind);
            gnome.is_gnome = true;
            assert!(matches!(
                Capabilities::resolve(&gnome).state(Capability::KeystrokeObservation),
                CapabilityState::IntegrationRequired { .. }
            ));
        }
    }

    #[test]
    fn companion_is_available_without_an_external_service() {
        let caps = Capabilities::resolve(&session(SessionKind::Wayland));
        assert_eq!(
            caps.state(Capability::CompanionService),
            &CapabilityState::Available
        );
    }

    #[test]
    fn clipboard_history_is_reachable_on_gnome_wayland_via_the_extension() {
        let mut gnome = session(SessionKind::Wayland);
        gnome.is_gnome = true;
        assert!(Capabilities::resolve(&gnome).is_supported(Capability::ClipboardHistory));

        let mut other = session(SessionKind::Wayland);
        other.is_gnome = false;
        assert!(!Capabilities::resolve(&other).is_supported(Capability::ClipboardHistory));

        assert_eq!(
            Capabilities::resolve(&session(SessionKind::X11)).state(Capability::ClipboardHistory),
            &CapabilityState::Available
        );
    }

    #[test]
    fn permission_gated_capabilities_still_count_as_supported() {
        let caps = Capabilities::resolve(&session(SessionKind::Wayland));
        assert!(caps.is_supported(Capability::ScreenColorSampling));
        assert!(caps.is_supported(Capability::GlobalShortcuts));
    }

    #[test]
    fn companion_does_not_depend_on_a_container_runtime() {
        let mut s = session(SessionKind::Wayland);
        s.has_container_runtime = false;
        let caps = Capabilities::resolve(&s);
        assert!(caps.is_supported(Capability::CompanionService));
        assert!(caps.is_supported(Capability::UsageCollection));
    }

    #[test]
    fn a_headless_session_reports_gui_capabilities_as_unsupported() {
        let caps = Capabilities::resolve(&session(SessionKind::Headless));
        for capability in [
            Capability::ScreenColorSampling,
            Capability::ClipboardHistory,
            Capability::WindowDimming,
            Capability::GlobalShortcuts,
        ] {
            assert!(
                matches!(caps.state(capability), CapabilityState::Unsupported { .. }),
                "{capability:?} should be unsupported without a display"
            );
            assert!(!caps.is_supported(capability));
        }
    }

    #[test]
    fn headless_still_collects_usage_because_it_only_reads_files() {
        let caps = Capabilities::resolve(&session(SessionKind::Headless));
        assert!(caps.is_supported(Capability::UsageCollection));
        assert!(caps.is_supported(Capability::SystemMetrics));
        assert!(caps.is_supported(Capability::MachineManagement));
    }

    #[test]
    fn an_unresolved_capability_still_returns_a_state() {
        let empty = Capabilities {
            states: BTreeMap::new(),
        };
        assert!(matches!(
            empty.state(Capability::UsageCollection),
            CapabilityState::Unsupported { .. }
        ));
    }

    #[test]
    fn top_bar_integration_needs_gnome() {
        let mut gnome = session(SessionKind::Wayland);
        gnome.is_gnome = true;
        assert!(Capabilities::resolve(&gnome).is_supported(Capability::ShellIntegration));

        let mut other = session(SessionKind::Wayland);
        other.is_gnome = false;
        assert!(matches!(
            Capabilities::resolve(&other).state(Capability::ShellIntegration),
            CapabilityState::Unsupported { .. }
        ));
    }

    #[test]
    fn every_capability_resolves_to_a_state() {
        let caps = Capabilities::resolve(&session(SessionKind::Wayland));
        for capability in Capability::ALL {
            assert!(
                caps.states.contains_key(&capability),
                "{capability:?} has no resolved state"
            );
        }
    }
}
