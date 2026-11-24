use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    Wayland,
    X11,
    Headless,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSession {
    pub kind: SessionKind,
    pub desktop: String,
    pub is_gnome: bool,
    pub shell_version: Option<String>,
    pub has_global_shortcuts_portal: bool,
    pub has_container_runtime: bool,
    pub has_pipewire: bool,
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
            has_global_shortcuts_portal: false,
            has_container_runtime: which("docker").is_some() || which("podman").is_some(),
            has_pipewire: which("pw-cli").is_some() || which("wpctl").is_some(),
            toolkit_backend: std::env::var("GDK_BACKEND").unwrap_or_default(),
        }
    }

    pub fn classify(session_type: &str, wayland_display: &str, display: &str) -> SessionKind {
        match session_type.to_ascii_lowercase().as_str() {
            "wayland" => SessionKind::Wayland,
            "x11" => SessionKind::X11,
            _ if !wayland_display.is_empty() => SessionKind::Wayland,
            _ if !display.is_empty() => SessionKind::X11,
            _ => SessionKind::Headless,
        }
    }

    pub fn primary_desktop(raw: &str) -> String {
        raw.split(':')
            .map(str::trim)
            .rfind(|part| !part.is_empty())
            .unwrap_or("unknown")
            .to_string()
    }

    pub fn is_graphical(&self) -> bool {
        self.kind != SessionKind::Headless
    }
}

pub fn which(binary: &str) -> Option<std::path::PathBuf> {
    crate::host::which(binary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_type_wins_over_the_display_variables() {
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
