//! Desktop session detection.
//!
//! Which compositor the user logged into decides what several features can do,
//! so this is resolved once at launch and threaded through the capability map.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    Wayland,
    X11,
    /// No graphical session: a TTY, an SSH shell, or CI. The collector and the
    /// CLI still work here; the UI does not.
    Headless,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSession {
    pub kind: SessionKind,
    /// Primary entry of XDG_CURRENT_DESKTOP, e.g. "GNOME" or "KDE".
    pub desktop: String,
    /// True on GNOME, where the shell owns the top bar and window list.
    pub is_gnome: bool,
    pub shell_version: Option<String>,
    pub has_global_shortcuts_portal: bool,
    pub has_container_runtime: bool,
    pub has_pipewire: bool,
    /// Which GDK backend this process actually connected to.
    ///
    /// Distinct from `kind`: Veronica asks for X11 even inside a Wayland
    /// session, because the notch overlay needs to position and raise itself.
    /// Reporting only the session would make that behaviour look like a bug.
    pub toolkit_backend: String,
}

impl DesktopSession {
    pub fn unknown() -> Self {
        Self {
            kind: SessionKind::Headless,
            desktop: String::new(),
            is_gnome: false,
            shell_version: None,
            has_global_shortcuts_portal: false,
            has_container_runtime: false,
            has_pipewire: false,
            toolkit_backend: String::new(),
        }
    }

    pub fn detect() -> Self {
        let env = |key: &str| std::env::var(key).unwrap_or_default();
        let kind = Self::classify(
            &env("XDG_SESSION_TYPE"),
            &env("WAYLAND_DISPLAY"),
            &env("DISPLAY"),
        );
        let raw_desktop = env("XDG_CURRENT_DESKTOP");
        let desktop = Self::primary_desktop(&raw_desktop);

        Self {
            kind,
            is_gnome: raw_desktop.to_ascii_uppercase().contains("GNOME"),
            desktop,
            shell_version: None,
            // The portal advertises interfaces on the session bus; probing it
            // properly needs an async connection, so the caller refines this.
            has_global_shortcuts_portal: false,
            has_container_runtime: which("docker").is_some() || which("podman").is_some(),
            has_pipewire: which("pw-cli").is_some() || which("wpctl").is_some(),
            // Set by the process itself before GTK starts; absent for the CLI,
            // which has no toolkit at all.
            toolkit_backend: std::env::var("GDK_BACKEND").unwrap_or_default(),
        }
    }

    /// Session type from the three variables that describe it.
    ///
    /// `XDG_SESSION_TYPE` is authoritative when set, but a GNOME Wayland
    /// session also exports `DISPLAY` for Xwayland clients, so checking
    /// `DISPLAY` first would misreport Wayland as X11.
    pub fn classify(session_type: &str, wayland_display: &str, display: &str) -> SessionKind {
        match session_type.to_ascii_lowercase().as_str() {
            "wayland" => SessionKind::Wayland,
            "x11" => SessionKind::X11,
            _ if !wayland_display.is_empty() => SessionKind::Wayland,
            _ if !display.is_empty() => SessionKind::X11,
            _ => SessionKind::Headless,
        }
    }

    /// XDG_CURRENT_DESKTOP is a colon-separated preference list; Ubuntu sets
    /// "ubuntu:GNOME". The last entry is the generic one, which is the useful
    /// name for feature decisions.
    pub fn primary_desktop(raw: &str) -> String {
        raw.split(':')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .next_back()
            .unwrap_or("unknown")
            .to_string()
    }

    pub fn is_graphical(&self) -> bool {
        self.kind != SessionKind::Headless
    }
}

/// Minimal `which`, so the core crate stays dependency-free.
pub fn which(binary: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(binary))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_type_wins_over_the_display_variables() {
        // A GNOME Wayland session exports DISPLAY for Xwayland; that must not
        // be read as an X11 session.
        assert_eq!(
            DesktopSession::classify("wayland", "wayland-0", ":0"),
            SessionKind::Wayland
        );
    }

    #[test]
    fn falls_back_to_the_display_variables() {
        assert_eq!(
            DesktopSession::classify("", "wayland-0", ""),
            SessionKind::Wayland
        );
        assert_eq!(DesktopSession::classify("", "", ":0"), SessionKind::X11);
    }

    #[test]
    fn no_display_variables_means_headless() {
        assert_eq!(DesktopSession::classify("", "", ""), SessionKind::Headless);
    }

    #[test]
    fn ubuntu_desktop_list_resolves_to_gnome() {
        assert_eq!(DesktopSession::primary_desktop("ubuntu:GNOME"), "GNOME");
        assert_eq!(DesktopSession::primary_desktop("GNOME"), "GNOME");
        assert_eq!(DesktopSession::primary_desktop(""), "unknown");
    }
}
