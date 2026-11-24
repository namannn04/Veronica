use std::ffi::OsString;

use serde::Serialize;

pub const FLATPAK_APP_ID: &str = "io.github.namannn04.Veronica";

pub const SNAP_NAME: &str = "veronica";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Packaging {
    Deb,
    Flatpak,
    Snap,
    AppImage,
}

impl Packaging {
    pub fn detect() -> Self {
        Self::from_env(crate::host::is_sandboxed(), |name| std::env::var_os(name))
    }

    pub fn from_env(sandboxed: bool, var: impl Fn(&str) -> Option<OsString>) -> Self {
        if sandboxed {
            Self::Flatpak
        } else if var("SNAP_NAME").is_some_and(|name| name == SNAP_NAME) {
            Self::Snap
        } else if var("APPIMAGE").is_some() {
            Self::AppImage
        } else {
            Self::Deb
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deb => "deb",
            Self::Flatpak => "flatpak",
            Self::Snap => "snap",
            Self::AppImage => "appimage",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(sandboxed: bool, vars: &[(&str, &str)]) -> Packaging {
        Packaging::from_env(sandboxed, |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        })
    }

    #[test]
    fn sandbox_wins_over_every_variable() {
        assert_eq!(
            detect(true, &[("SNAP_NAME", "veronica"), ("APPIMAGE", "/x")]),
            Packaging::Flatpak
        );
    }

    #[test]
    fn only_the_veronica_snap_counts_as_snap() {
        assert_eq!(detect(false, &[("SNAP_NAME", "veronica")]), Packaging::Snap);
        assert_eq!(detect(false, &[("SNAP_NAME", "other")]), Packaging::Deb);
    }

    #[test]
    fn appimage_and_plain_installs() {
        assert_eq!(
            detect(false, &[("APPIMAGE", "/opt/Veronica.AppImage")]),
            Packaging::AppImage
        );
        assert_eq!(detect(false, &[]), Packaging::Deb);
        assert_eq!(Packaging::AppImage.as_str(), "appimage");
    }
}
