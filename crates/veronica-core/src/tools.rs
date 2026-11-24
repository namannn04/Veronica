use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "route", rename_all = "camelCase")]
pub enum Install {
    Apt { package: &'static str },
    Npm { package: &'static str },
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    pub why: &'static str,
    pub executable: &'static str,
    pub version_args: &'static [&'static str],
    pub install: Install,
    pub instruction: &'static str,
}

impl ToolSpec {
    pub fn locate(&self) -> Option<PathBuf> {
        locate_in(&search_path(), self.executable)
    }

    pub fn present(&self) -> bool {
        self.locate().is_some()
    }
}

pub const CATALOG: &[ToolSpec] = &[
    ToolSpec {
        id: "claude",
        display_name: "Claude Code",
        why: "Agent Usage reads Claude's session and weekly rate limits from it.",
        executable: "claude",
        version_args: &["--version"],
        install: Install::Npm {
            package: "@anthropic-ai/claude-code",
        },
        instruction: "Install with `npm install -g @anthropic-ai/claude-code`.",
    },
    ToolSpec {
        id: "codex",
        display_name: "Codex",
        why: "Agent Usage reads Codex's rate-limit windows from its own server.",
        executable: "codex",
        version_args: &["--version"],
        install: Install::Npm {
            package: "@openai/codex",
        },
        instruction: "Install with `npm install -g @openai/codex`.",
    },
    ToolSpec {
        id: "herdr",
        display_name: "Herdr",
        why: "The agent board is Herdr's own JSON API; without it there is no board.",
        executable: "herdr",
        version_args: &["--version"],
        install: Install::Manual,
        instruction: "Install Herdr, and make sure `herdr` is on PATH or in ~/.local/bin.",
    },
    ToolSpec {
        id: "quinjet",
        display_name: "Quinjet",
        why: "Quinjet owns the review workspaces and the review itself.",
        executable: "quinjet",
        version_args: &["--version"],
        install: Install::Manual,
        instruction: "Install Quinjet, and make sure `quinjet` is on PATH or in ~/.local/bin.",
    },
    ToolSpec {
        id: "ssh",
        display_name: "OpenSSH client",
        why: "Every remote machine is reached by running `ssh`, with your own config and keys.",
        executable: "ssh",
        version_args: &["-V"],
        install: Install::Apt {
            package: "openssh-client",
        },
        instruction: "Install with `sudo apt install openssh-client`.",
    },
];

pub fn spec(id: &str) -> Option<&'static ToolSpec> {
    CATALOG.iter().find(|tool| tool.id == id)
}

pub fn wanted_by(id: &str) -> Vec<&'static str> {
    crate::extensions::ENTRIES
        .iter()
        .filter(|entry| entry.required_tools.contains(&id))
        .map(|entry| entry.title)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRule {
    All,
    Any,
}

pub fn rule(extension_id: &str) -> ToolRule {
    match extension_id {
        "usage" => ToolRule::Any,
        _ => ToolRule::All,
    }
}

pub fn required_by(entry: &crate::extensions::ExtensionEntry) -> Vec<&'static ToolSpec> {
    entry
        .required_tools
        .iter()
        .filter_map(|id| spec(id))
        .collect()
}

pub fn search_path() -> Vec<PathBuf> {
    let named = crate::host::path_directories();

    let fixed = ["/usr/local/bin", "/usr/bin", "/bin", "/snap/bin"]
        .into_iter()
        .map(PathBuf::from);

    let home = std::env::var_os("HOME").map(PathBuf::from);
    let personal = home
        .into_iter()
        .flat_map(|home| {
            [".local/bin", ".cargo/bin", ".bun/bin", ".npm-global/bin"]
                .into_iter()
                .map(move |suffix| home.join(suffix))
        })
        .collect::<Vec<_>>();

    let mut seen = Vec::new();
    for directory in named.into_iter().chain(fixed).chain(personal) {
        if !seen.contains(&directory) {
            seen.push(directory);
        }
    }
    seen
}

pub fn locate_in(directories: &[PathBuf], executable: &str) -> Option<PathBuf> {
    directories
        .iter()
        .map(|directory| directory.join(executable))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    crate::host::is_executable(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_tool(directory: &Path, name: &str, executable: bool) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir_all(directory).unwrap();
        let path = directory.join(name);
        fs::write(&path, "#!/bin/sh\n").unwrap();
        let mode = if executable { 0o755 } else { 0o644 };
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    #[test]
    fn every_declared_tool_is_in_the_catalogue() {
        for entry in crate::extensions::ENTRIES {
            for id in entry.required_tools {
                assert!(
                    spec(id).is_some(),
                    "extension {} needs tool '{id}', which is not in the catalogue",
                    entry.id
                );
            }
        }
    }

    #[test]
    fn ids_are_unique() {
        for (index, tool) in CATALOG.iter().enumerate() {
            assert!(
                CATALOG
                    .iter()
                    .skip(index + 1)
                    .all(|other| other.id != tool.id),
                "duplicate tool id '{}'",
                tool.id
            );
        }
    }

    #[test]
    fn a_missing_tool_is_not_located() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(locate_in(&[root.path().to_path_buf()], "quinjet"), None);
    }

    #[test]
    fn a_directory_that_does_not_exist_is_skipped_rather_than_failing() {
        let root = tempfile::tempdir().unwrap();
        let present = write_tool(&root.path().join("second"), "herdr", true);
        let directories = vec![root.path().join("missing"), root.path().join("second")];
        assert_eq!(locate_in(&directories, "herdr"), Some(present));
    }

    #[test]
    fn a_file_without_the_execute_bit_is_not_a_tool() {
        let root = tempfile::tempdir().unwrap();
        write_tool(root.path(), "codex", false);
        assert_eq!(locate_in(&[root.path().to_path_buf()], "codex"), None);
    }

    #[test]
    fn the_first_directory_holding_the_tool_wins() {
        let root = tempfile::tempdir().unwrap();
        let first = write_tool(&root.path().join("a"), "claude", true);
        write_tool(&root.path().join("b"), "claude", true);
        let directories = vec![root.path().join("a"), root.path().join("b")];
        assert_eq!(locate_in(&directories, "claude"), Some(first));
    }

    #[test]
    fn the_search_path_has_no_duplicates() {
        let path = search_path();
        for (index, directory) in path.iter().enumerate() {
            assert!(
                path.iter().skip(index + 1).all(|other| other != directory),
                "{} appears twice in the search path",
                directory.display()
            );
        }
    }

    #[test]
    fn the_search_path_covers_the_per_user_prefixes() {
        let path = search_path();
        assert!(path.contains(&PathBuf::from("/usr/bin")));
        if std::env::var_os("HOME").is_some() {
            assert!(
                path.iter()
                    .any(|directory| directory.ends_with(".local/bin")),
                "~/.local/bin is where these tools most often land"
            );
        }
    }
}
