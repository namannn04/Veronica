//! Platform capabilities.
//!
//! A direct port of Edith's `PlatformCapability` model. The variants are
//! identical so the extension catalogue stays shared; only the resolved state
//! differs, because each capability reaches a different Linux service.

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
    pub const ALL: [Capability; 31] = [
        Capability::ApplicationAudio,
        Capability::BluetoothMonitoring,
        Capability::CalendarEvents,
        Capability::CameraPreview,
        Capability::ClipboardHistory,
        Capability::CompanionService,
        Capability::DatabaseBroker,
        Capability::EmojiInsertion,
        Capability::ExternalMediaControl,
        Capability::FileShelf,
        Capability::GlobalPaste,
        Capability::HerdrSessions,
        Capability::GlobalShortcuts,
        Capability::InputSuppression,
        Capability::KeystrokeObservation,
        Capability::LocalMusicPlayback,
        Capability::LocalTerminal,
        Capability::MachineManagement,
        Capability::MediaControls,
        Capability::MicrophoneControl,
        Capability::Notifications,
        Capability::PackageManagement,
        Capability::PreventSleep,
        Capability::RunningApplications,
        Capability::ScreenColorSampling,
        Capability::ScreenShareDetection,
        Capability::ShellIntegration,
        Capability::SiteAuditing,
        Capability::SystemMetrics,
        Capability::UsageCollection,
        Capability::WindowDimming,
    ];

    /// Human label, matching the wording Edith uses in its settings pane.
    pub fn title(self) -> &'static str {
        use Capability::*;
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

    /// The Linux service this capability is implemented against. Shown on the
    /// diagnostics page so a failure points at something specific.
    pub fn backend(self) -> &'static str {
        use Capability::*;
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
            GlobalPaste => "GNOME Shell extension",
            HerdrSessions | UsageCollection => "Filesystem",
            GlobalShortcuts => "xdg-desktop-portal GlobalShortcuts",
            InputSuppression => "libinput / evdev",
            KeystrokeObservation => "GNOME Shell extension",
            LocalMusicPlayback => "GStreamer",
            LocalTerminal => "GNOME Console / x-terminal-emulator",
            MachineManagement => "OpenSSH",
            Notifications => "org.freedesktop.Notifications",
            PackageManagement => "apt / snap / flatpak",
            PreventSleep => "systemd-logind inhibitor",
            RunningApplications => "procfs + desktop entries",
            ScreenColorSampling => "xdg-desktop-portal Screenshot.PickColor",
            ScreenShareDetection => "xdg-desktop-portal ScreenCast",
            ShellIntegration => "GNOME Shell extension",
            SiteAuditing => "HTTPS",
            SystemMetrics => "procfs / sysfs",
            WindowDimming => "Compositor",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum CapabilityState {
    /// Works now, nothing for the user to do.
    Available,
    /// Works once the user grants a portal or system permission.
    PermissionRequired { reason: String },
    /// Reachable on this platform but Veronica has not wired it up yet.
    IntegrationRequired { reason: String },
    /// Cannot work in this session at all.
    Unsupported { reason: String },
}

impl CapabilityState {
    /// Edith treats permission-gated capabilities as supported, because the
    /// user can resolve them without a code change.
    pub fn is_supported(&self) -> bool {
        matches!(
            self,
            CapabilityState::Available | CapabilityState::PermissionRequired { .. }
        )
    }

    fn permission(reason: &str) -> Self {
        CapabilityState::PermissionRequired {
            reason: reason.to_string(),
        }
    }

    fn integration(reason: &str) -> Self {
        CapabilityState::IntegrationRequired {
            reason: reason.to_string(),
        }
    }

    fn unsupported(reason: &str) -> Self {
        CapabilityState::Unsupported {
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
        /// Returned for a capability the resolver did not cover, so callers
        /// always get a state rather than an option.
        static UNKNOWN: LazyLock<CapabilityState> =
            LazyLock::new(|| CapabilityState::Unsupported {
                reason: "Capability has no platform implementation.".to_string(),
            });
        self.states.get(&capability).unwrap_or(&UNKNOWN)
    }

    pub fn is_supported(&self, capability: Capability) -> bool {
        self.states
            .get(&capability)
            .map(CapabilityState::is_supported)
            .unwrap_or(false)
    }

    /// Resolve every capability for the running session.
    ///
    /// Several capabilities differ between Wayland and X11: a Wayland
    /// compositor deliberately withholds the global input, window list and
    /// clipboard access that these features need, so they resolve differently
    /// depending on which session the user logged into.
    pub fn resolve(session: &DesktopSession) -> Self {
        use Capability::*;
        let wayland = session.kind == SessionKind::Wayland;
        let mut states = BTreeMap::new();

        let mut set = |capability: Capability, state: CapabilityState| {
            states.insert(capability, state);
        };

        // Portable: these read files or spawn processes and behave the same as
        // they do on macOS.
        set(UsageCollection, CapabilityState::Available);
        set(HerdrSessions, CapabilityState::Available);
        set(FileShelf, CapabilityState::Available);
        set(SystemMetrics, CapabilityState::Available);
        set(MachineManagement, CapabilityState::Available);
        // Edith hosts a terminal inside itself, which is what lets it give
        // Quinjet native tabs. Veronica opens the installed terminal instead —
        // the same decision Herdr's port made — so this is available wherever
        // one is installed rather than requiring an embedded emulator.
        set(LocalTerminal, CapabilityState::Available);

        // Standard freedesktop services, present on any modern desktop.
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
        // Fetching the pages being audited is the only thing this needs, and
        // it is the only thing it does: no result leaves the computer, and no
        // third-party service is consulted.
        set(SiteAuditing, CapabilityState::Available);

        set(CompanionService, CapabilityState::Available);

        // Edith puts a broker process on this boundary because a sandboxed
        // macOS app cannot hold a database socket. Veronica has no sandbox to
        // cross, so it holds the socket itself; what it does need is somewhere
        // to keep credentials, and the desktop keyring is that. Without one —
        // over SSH — secrets fall back to a 0600 file, which works and is
        // worse, so this reports as needing permission rather than as ready.
        set(
            DatabaseBroker,
            CapabilityState::permission(
                "Database passwords go to the desktop keyring, which asks you to unlock it \
                 on first use. Without a keyring they fall back to a 0600 file.",
            ),
        );

        // Reading what is installed and what could be updated needs nothing.
        // Applying an apt or snap change needs root, which Veronica never takes
        // on its own: it runs the change through `pkexec`, so the desktop's own
        // dialog asks the user to authenticate.
        set(
            PackageManagement,
            CapabilityState::permission(
                "Installing or removing a package asks for authentication through pkexec. \
                 Listing what is installed and what could be updated needs nothing.",
            ),
        );

        // Portal-gated: the portal exists, the user approves on first use.
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

        // Wayland deliberately restricts these.
        set(
            ClipboardHistory,
            if !wayland {
                CapabilityState::Available
            } else if session.is_gnome {
                // The shell extension does the watching, since the compositor is
                // the only thing a Wayland session lets read the selection.
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
        // Dimming behind another application's window is the compositor's job:
        // no client can place something between two windows it does not own.
        // Veronica's GNOME Shell extension draws it, which is why this does not
        // depend on Wayland versus X11 but on whether the shell is GNOME.
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

        // The picker itself only needs a clipboard, which always works. Typing
        // the emoji straight into the app you were in synthesises a key press
        // through the RemoteDesktop portal, exactly as paste-in-place does, so
        // it carries the same approval.
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

        // Watching key presses is the compositor's job for the same reason
        // dimming is: on Wayland no client may observe input meant for another
        // window, and there is no permission that changes it.
        set(
            KeystrokeObservation,
            if session.is_gnome {
                CapabilityState::permission(
                    "Enable Veronica's GNOME Shell extension, which reads key presses \
                     inside the compositor and never consumes one.",
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

        // Without a graphical session there is no compositor, portal or tray to
        // talk to. `vr` runs happily over SSH, so rather than reporting these as
        // available and failing later, mark them unsupported for this session.
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

    /// Everything the user could act on, so the UI can surface one list.
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
        // The dimming is drawn by the GNOME Shell extension, so it works on
        // Wayland — where a client could never do it — and not on a non-GNOME
        // X11 desktop, where a client could but nothing has been written.
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
        // Permission-gated rather than unavailable: the user can act on it.
        assert!(Capabilities::resolve(&gnome).is_supported(Capability::ClipboardHistory));

        let mut other = session(SessionKind::Wayland);
        other.is_gnome = false;
        assert!(!Capabilities::resolve(&other).is_supported(Capability::ClipboardHistory));

        // X11 hands the clipboard to anyone who asks.
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
        // `vr` over SSH has no compositor, portal or tray.
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
