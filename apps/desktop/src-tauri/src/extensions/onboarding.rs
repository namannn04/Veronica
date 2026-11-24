use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Serialize;

struct Tool {
    name: &'static str,
    binaries: &'static [&'static str],
    launchers: &'static [&'static str],
}

const EDITORS: &[Tool] = &[
    Tool {
        name: "VS Code",
        binaries: &["code"],
        launchers: &[
            "code.desktop",
            "code_code.desktop",
            "com.visualstudio.code.desktop",
        ],
    },
    Tool {
        name: "Cursor",
        binaries: &["cursor"],
        launchers: &["cursor.desktop", "cursor_cursor.desktop"],
    },
    Tool {
        name: "Zed",
        binaries: &["zed", "zeditor"],
        launchers: &["dev.zed.Zed.desktop", "zed.desktop"],
    },
    Tool {
        name: "JetBrains Toolbox",
        binaries: &["jetbrains-toolbox"],
        launchers: &["jetbrains-toolbox.desktop"],
    },
    Tool {
        name: "Ghostty",
        binaries: &["ghostty"],
        launchers: &["com.mitchellh.ghostty.desktop"],
    },
    Tool {
        name: "Kitty",
        binaries: &["kitty"],
        launchers: &["kitty.desktop"],
    },
    Tool {
        name: "Alacritty",
        binaries: &["alacritty"],
        launchers: &[
            "Alacritty.desktop",
            "alacritty.desktop",
            "org.alacritty.Alacritty.desktop",
        ],
    },
    Tool {
        name: "WezTerm",
        binaries: &["wezterm"],
        launchers: &["org.wezfurlong.wezterm.desktop"],
    },
];

const AGENTS: &[&str] = &["claude", "codex", "cursor-agent", "gemini", "opencode"];
const AGENT_HOME_DIRS: &[&str] = &[".local/bin", ".npm-global/bin", ".bun/bin", ".cargo/bin"];
const DEFAULT_DATA_DIRS: &str = "/usr/local/share:/usr/share";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Profile {
    Simple,
    Developer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Detection {
    suggested: Profile,
    editors: Vec<&'static str>,
    agents: Vec<&'static str>,
}

#[derive(Debug, Clone)]
pub(crate) struct Environment {
    home: PathBuf,
    path: Option<OsString>,
    data_home: Option<PathBuf>,
    data_dirs: Option<OsString>,
    extra_applications: Vec<PathBuf>,
}

fn variable(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}

impl Environment {
    pub(crate) fn current() -> Self {
        let home = veronica_core::paths::home_dir().unwrap_or_default();
        let extra_applications = vec![
            PathBuf::from("/var/lib/snapd/desktop/applications"),
            PathBuf::from("/var/lib/flatpak/exports/share/applications"),
            home.join(".local/share/flatpak/exports/share/applications"),
        ];
        Self {
            path: variable("PATH"),
            data_home: variable("XDG_DATA_HOME").map(PathBuf::from),
            data_dirs: variable("XDG_DATA_DIRS"),
            extra_applications,
            home,
        }
    }

    fn binary_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self
            .path
            .as_deref()
            .map(|path| {
                std::env::split_paths(path)
                    .filter(|dir| dir.is_absolute())
                    .collect()
            })
            .unwrap_or_default();
        dirs.extend(AGENT_HOME_DIRS.iter().map(|dir| self.home.join(dir)));
        dirs
    }

    fn application_dirs(&self) -> Vec<PathBuf> {
        let data_home = self
            .data_home
            .clone()
            .unwrap_or_else(|| self.home.join(".local/share"));
        let data_dirs = self
            .data_dirs
            .clone()
            .unwrap_or_else(|| OsString::from(DEFAULT_DATA_DIRS));
        let mut dirs = vec![data_home.join("applications")];
        dirs.extend(std::env::split_paths(&data_dirs).map(|dir| dir.join("applications")));
        dirs.extend(self.extra_applications.iter().cloned());
        dirs
    }
}

fn executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && (meta.permissions().mode() & 0o111) != 0)
}

fn on_path(dirs: &[PathBuf], name: &str) -> bool {
    dirs.iter().any(|dir| executable(&dir.join(name)))
}

fn launcher(dirs: &[PathBuf], name: &str) -> bool {
    dirs.iter().any(|dir| dir.join(name).is_file())
}

pub(crate) fn detect(environment: &Environment) -> Detection {
    let binaries = environment.binary_dirs();
    let applications = environment.application_dirs();
    let editors: Vec<&'static str> = EDITORS
        .iter()
        .filter(|tool| {
            tool.binaries.iter().any(|name| on_path(&binaries, name))
                || tool
                    .launchers
                    .iter()
                    .any(|name| launcher(&applications, name))
        })
        .map(|tool| tool.name)
        .collect();
    let agents: Vec<&'static str> = AGENTS
        .iter()
        .copied()
        .filter(|name| on_path(&binaries, name))
        .collect();
    let suggested = if editors.is_empty() && agents.is_empty() {
        Profile::Simple
    } else {
        Profile::Developer
    };
    Detection {
        suggested,
        editors,
        agents,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn isolated(home: &TempDir) -> Environment {
        let root = home.path();
        Environment {
            home: root.to_path_buf(),
            path: Some(root.join("path-bin").into_os_string()),
            data_home: None,
            data_dirs: Some(root.join("system-share").into_os_string()),
            extra_applications: vec![root.join("snap-applications")],
        }
    }

    fn program(path: &Path, mode: u32) {
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(path, "#!/bin/sh\n").expect("program");
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("mode");
    }

    fn desktop_file(path: &Path) {
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(path, "[Desktop Entry]\nType=Application\n").expect("launcher");
    }

    #[test]
    fn an_empty_home_suggests_the_simple_profile() {
        let home = TempDir::new().expect("home");
        let found = detect(&isolated(&home));
        assert_eq!(found.suggested, Profile::Simple);
        assert!(found.editors.is_empty());
        assert!(found.agents.is_empty());
    }

    #[test]
    fn agent_programs_in_known_folders_suggest_the_developer_profile() {
        let home = TempDir::new().expect("home");
        program(&home.path().join(".bun/bin/opencode"), 0o755);
        program(&home.path().join(".cargo/bin/codex"), 0o755);
        program(&home.path().join("path-bin/claude"), 0o755);
        let found = detect(&isolated(&home));
        assert_eq!(found.suggested, Profile::Developer);
        assert_eq!(found.agents, ["claude", "codex", "opencode"]);
    }

    #[test]
    fn editors_are_found_by_program_or_launcher() {
        let home = TempDir::new().expect("home");
        program(&home.path().join("path-bin/zeditor"), 0o755);
        desktop_file(&home.path().join(".local/share/applications/kitty.desktop"));
        desktop_file(&home.path().join("system-share/applications/code.desktop"));
        desktop_file(&home.path().join("snap-applications/cursor_cursor.desktop"));
        let found = detect(&isolated(&home));
        assert_eq!(found.suggested, Profile::Developer);
        assert_eq!(found.editors, ["VS Code", "Cursor", "Zed", "Kitty"]);
    }

    #[test]
    fn the_default_terminal_and_plain_files_do_not_count() {
        let home = TempDir::new().expect("home");
        desktop_file(
            &home
                .path()
                .join(".local/share/applications/org.gnome.Terminal.desktop"),
        );
        program(&home.path().join("path-bin/gnome-terminal"), 0o755);
        program(&home.path().join(".local/bin/claude"), 0o644);
        fs::create_dir_all(home.path().join(".local/bin/codex")).expect("folder");
        let found = detect(&isolated(&home));
        assert_eq!(found.suggested, Profile::Simple);
    }

    #[test]
    fn the_detection_reports_in_camel_case() {
        let home = TempDir::new().expect("home");
        program(&home.path().join(".npm-global/bin/gemini"), 0o755);
        let value = serde_json::to_value(detect(&isolated(&home))).expect("json");
        assert_eq!(
            value,
            serde_json::json!({ "suggested": "developer", "editors": [], "agents": ["gemini"] })
        );
    }
}
