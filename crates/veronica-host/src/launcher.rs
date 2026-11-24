use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Serialize;
use veronica_core::packaging::{Packaging, FLATPAK_APP_ID};

pub const SNAP_VR: &str = "/snap/bin/veronica.vr";

const VR: &str = "vr";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchEnvironment {
    pub packaging: Packaging,
    pub appimage: Option<PathBuf>,
    pub appdir: Option<PathBuf>,
    pub snap: Option<PathBuf>,
    pub current_exe: Option<PathBuf>,
    pub path: Option<OsString>,
}

impl LaunchEnvironment {
    pub fn current() -> Self {
        Self::from_env(
            veronica_core::host::is_sandboxed(),
            std::env::current_exe().ok(),
            |name| std::env::var_os(name),
        )
    }

    pub fn from_env(
        sandboxed: bool,
        current_exe: Option<PathBuf>,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Self {
        Self {
            packaging: Packaging::from_env(sandboxed, &var),
            appimage: var("APPIMAGE").map(PathBuf::from),
            appdir: var("APPDIR").map(PathBuf::from),
            snap: var("SNAP").map(PathBuf::from),
            current_exe,
            path: var("PATH"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Launch {
    pub packaging: Packaging,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{reason}")]
pub struct Unsupported {
    pub packaging: Packaging,
    pub reason: String,
}

pub fn resolve(environment: &LaunchEnvironment) -> Result<Launch, Unsupported> {
    let packaging = environment.packaging;
    let (command, args) = match packaging {
        Packaging::Flatpak => (
            "flatpak".to_string(),
            vec![
                "run".to_string(),
                "--command=vr".to_string(),
                FLATPAK_APP_ID.to_string(),
            ],
        ),
        Packaging::Snap => (SNAP_VR.to_string(), Vec::new()),
        Packaging::AppImage => {
            let image = environment.appimage.as_deref().map_or_else(
                || "the AppImage".to_string(),
                |path| path.display().to_string(),
            );
            return Err(Unsupported {
                packaging,
                reason: format!(
                    "{image} does not ship a vr entry point that runs outside the image; install the Debian package, Flatpak or Snap to register the MCP server"
                ),
            });
        }
        Packaging::Deb => {
            let binary = installed_vr(environment).ok_or_else(|| Unsupported {
                packaging,
                reason: "cannot find an installed vr binary next to this program or on PATH"
                    .to_string(),
            })?;
            (binary.display().to_string(), Vec::new())
        }
    };
    Ok(Launch {
        packaging,
        command,
        args,
    })
}

fn installed_vr(environment: &LaunchEnvironment) -> Option<PathBuf> {
    let current = environment.current_exe.as_deref();
    let running_vr = current.filter(|path| path.file_name().is_some_and(|name| name == VR));
    let sibling = current.and_then(Path::parent).map(|parent| parent.join(VR));
    let on_path = environment
        .path
        .iter()
        .flat_map(std::env::split_paths)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(VR));
    running_vr
        .map(Path::to_path_buf)
        .into_iter()
        .chain(sibling)
        .chain(on_path)
        .find_map(|candidate| executable(&candidate))
}

fn executable(candidate: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let real = std::fs::canonicalize(candidate).ok()?;
    let metadata = std::fs::metadata(&real).ok()?;
    (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then_some(real)
}
