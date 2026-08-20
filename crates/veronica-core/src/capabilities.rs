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
    ExternalMediaControl,
    FileShelf,
    GlobalPaste,
    HerdrSessions,
    GlobalShortcuts,
    InputSuppression,
    LocalMusicPlayback,
    MachineManagement,
    MediaControls,
    MicrophoneControl,
    Notifications,
    PreventSleep,
    RunningApplications,
    ScreenColorSampling,
    ScreenShareDetection,
    SystemMetrics,
    UsageCollection,
    WindowDimming,
}

impl Capability {
    pub const ALL: [Capability; 24] = [
        Capability::ApplicationAudio,
        Capability::BluetoothMonitoring,
        Capability::CalendarEvents,
        Capability::CameraPreview,
        Capability::ClipboardHistory,
        Capability::CompanionService,
        Capability::ExternalMediaControl,
        Capability::FileShelf,
        Capability::GlobalPaste,
        Capability::HerdrSessions,
        Capability::GlobalShortcuts,
        Capability::InputSuppression,
        Capability::LocalMusicPlayback,
        Capability::MachineManagement,
        Capability::MediaControls,
        Capability::MicrophoneControl,
        Capability::Notifications,
        Capability::PreventSleep,
        Capability::RunningApplications,
        Capability::ScreenColorSampling,
        Capability::ScreenShareDetection,
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
            ExternalMediaControl => "External media control",
            FileShelf => "File shelf",
            GlobalPaste => "Paste in place",
            HerdrSessions => "Herdr sessions",
            GlobalShortcuts => "Global shortcuts",
            InputSuppression => "Input suppression",
            LocalMusicPlayback => "Local music playback",
            MachineManagement => "Machine management",
            MediaControls => "Media keys",
            MicrophoneControl => "Microphone control",
            Notifications => "Notifications",
            PreventSleep => "Prevent sleep",
            RunningApplications => "Running applications",
            ScreenColorSampling => "Screen colour sampling",
            ScreenShareDetection => "Screen share detection",
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
            CompanionService => "Docker or Podman",
            ExternalMediaControl | MediaControls => "MPRIS2 (D-Bus)",
            FileShelf => "In-process",
            GlobalPaste => "xdg-desktop-portal RemoteDesktop",
            HerdrSessions | UsageCollection => "Filesystem",
            GlobalShortcuts => "xdg-desktop-portal GlobalShortcuts",
            InputSuppression => "libinput / evdev",
            LocalMusicPlayback => "GStreamer",
            MachineManagement => "OpenSSH",
            Notifications => "org.freedesktop.Notifications",
            PreventSleep => "systemd-logind inhibitor",
            RunningApplications => "procfs + desktop entries",
            ScreenColorSampling => "xdg-desktop-portal Screenshot.PickColor",
            ScreenShareDetection => "xdg-desktop-portal ScreenCast",
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
        static UNKNOWN: LazyLock<CapabilityState> = LazyLock::new(|| {
            CapabilityState::Unsupported {
                reason: "Capability has no platform implementation.".to_string(),
            }
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
        set(LocalMusicPlayback, CapabilityState::Available);

        // Standard freedesktop services, present on any modern desktop.
        set(Notifications, CapabilityState::Available);
        set(ExternalMediaControl, CapabilityState::Available);
        set(MediaControls, CapabilityState::Available);
        set(PreventSleep, CapabilityState::Available);
        set(ApplicationAudio, CapabilityState::Available);
        set(MicrophoneControl, CapabilityState::Available);
        set(BluetoothMonitoring, CapabilityState::Available);
        set(CalendarEvents, CapabilityState::Available);
        set(RunningApplications, CapabilityState::Available);
        set(ScreenShareDetection, CapabilityState::Available);

        set(
            CompanionService,
            if session.has_container_runtime {
                CapabilityState::Available
            } else {
                CapabilityState::integration(
                    "Install Docker or Podman to host the Companion backend.",
                )
            },
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
        set(
            GlobalShortcuts,
            if session.has_global_shortcuts_portal {
                CapabilityState::permission(
                    "Approve the shortcut list once in the desktop portal dialog.",
                )
            } else {
                CapabilityState::integration(
                    "This desktop has no GlobalShortcuts portal; bind the shortcut manually \
                     in Settings › Keyboard.",
                )
            },
        );
        set(
            GlobalPaste,
            CapabilityState::permission(
                "Pasting in place synthesises a key press through the RemoteDesktop portal, \
                 which needs approval each session.",
            ),
        );

        // Wayland deliberately restricts these.
        set(
            ClipboardHistory,
            if wayland {
                CapabilityState::integration(
                    "A Wayland compositor only hands the clipboard to the focused window. \
                     Veronica reads it while its panel is focused; continuous background \
                     history needs the GNOME Shell companion extension.",
                )
            } else {
                CapabilityState::Available
            },
        );
        set(
            WindowDimming,
            if wayland {
                CapabilityState::integration(
                    "Dimming other windows requires the compositor. Install the GNOME Shell \
                     companion extension, or use an X11 session.",
                )
            } else {
                CapabilityState::Available
            },
        );
        set(
            InputSuppression,
            CapabilityState::integration(
                "The keyboard-cleaning lock needs an exclusive evdev grab, which requires \
                 membership of the 'input' group.",
            ),
        );

        // Without a graphical session there is no compositor, portal or tray to
        // talk to. `vr` runs happily over SSH, so rather than reporting these as
        // available and failing later, mark them unsupported for this session.
        if session.kind == SessionKind::Headless {
            for capability in [
                CameraPreview,
                ClipboardHistory,
                GlobalPaste,
                GlobalShortcuts,
                RunningApplications,
                ScreenColorSampling,
                ScreenShareDetection,
                WindowDimming,
                InputSuppression,
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
    fn clipboard_and_dimming_need_help_on_wayland_but_not_on_x11() {
        let wayland = Capabilities::resolve(&session(SessionKind::Wayland));
        let x11 = Capabilities::resolve(&session(SessionKind::X11));

        for capability in [Capability::ClipboardHistory, Capability::WindowDimming] {
            assert!(
                matches!(
                    wayland.state(capability),
                    CapabilityState::IntegrationRequired { .. }
                ),
                "{capability:?} should need an integration on Wayland"
            );
            assert_eq!(x11.state(capability), &CapabilityState::Available);
        }
    }

    #[test]
    fn permission_gated_capabilities_still_count_as_supported() {
        let caps = Capabilities::resolve(&session(SessionKind::Wayland));
        assert!(caps.is_supported(Capability::ScreenColorSampling));
        assert!(caps.is_supported(Capability::GlobalShortcuts));
    }

    #[test]
    fn missing_container_runtime_degrades_companion_only() {
        let mut s = session(SessionKind::Wayland);
        s.has_container_runtime = false;
        let caps = Capabilities::resolve(&s);
        assert!(!caps.is_supported(Capability::CompanionService));
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
