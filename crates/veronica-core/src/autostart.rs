//! Per-user XDG autostart integration.
//!
//! The desktop app stays alive for alerts, shortcuts and tray actions, so it
//! needs an explicit login entry rather than merely remembering a UI switch.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::APP_ID;

/// Prefer the stable AppImage path over its temporary mount executable.
pub fn preferred_executable(appimage: Option<&OsStr>, current_exe: &Path) -> PathBuf {
    appimage
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| current_exe.to_path_buf())
}

/// The complete entry GNOME starts after login.
pub fn desktop_entry(executable: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Veronica\nComment=Start Veronica's alerts, shortcuts and tray\nExec={} --background\nIcon=veronica\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        quote_exec(executable)
    )
}

/// True only when the installed entry matches this executable.
///
/// An AppImage can move. Treating its old path as enabled would leave a switch
/// that says On while the desktop silently fails to launch anything.
pub fn is_enabled(path: &Path, executable: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|entry| entry == desktop_entry(executable))
}

/// Install or remove Veronica's one per-user autostart entry.
pub fn set_enabled(path: &Path, executable: &Path, enabled: bool) -> Result<()> {
    if !enabled {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| format!("cannot remove {}", path.display()))
            }
        }
    }

    let parent = path
        .parent()
        .context("the autostart entry has no parent directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("cannot create {}", parent.display()))?;
    let temporary = path.with_extension(format!("desktop.tmp-{}", std::process::id()));
    std::fs::write(&temporary, desktop_entry(executable))
        .with_context(|| format!("cannot write {}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .with_context(|| format!("cannot replace {}", path.display()))?;
    Ok(())
}

pub fn filename() -> String {
    format!("{APP_ID}.desktop")
}

fn quote_exec(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let escaped = raw
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_appimage_uses_its_stable_file_instead_of_the_mount() {
        assert_eq!(
            preferred_executable(
                Some(OsStr::new("/home/me/Apps/Veronica.AppImage")),
                Path::new("/tmp/.mount_Veroni/usr/bin/veronica"),
            ),
            PathBuf::from("/home/me/Apps/Veronica.AppImage")
        );
    }

    #[test]
    fn exec_paths_are_quoted_for_spaces_and_expansion_characters() {
        let entry = desktop_entry(Path::new("/home/me/My $Apps/Veronica.AppImage"));
        assert!(entry.contains("Exec=\"/home/me/My \\$Apps/Veronica.AppImage\" --background"));
    }

    #[test]
    fn enabling_and_disabling_changes_the_real_entry() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(filename());
        let executable = Path::new("/usr/bin/veronica");

        assert!(!is_enabled(&path, executable));
        set_enabled(&path, executable, true).unwrap();
        assert!(is_enabled(&path, executable));
        set_enabled(&path, executable, false).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn a_moved_appimage_is_not_reported_as_enabled() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(filename());
        set_enabled(&path, Path::new("/old/Veronica.AppImage"), true).unwrap();
        assert!(!is_enabled(&path, Path::new("/new/Veronica.AppImage")));
    }
}
