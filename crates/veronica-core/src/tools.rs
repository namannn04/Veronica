//! The command line programs Veronica's extensions shell out to.
//!
//! `extensions::ENTRIES` already names the tools each extension needs by id.
//! This is the table those ids resolve against: the readable name, why
//! Veronica wants it, which executable to look for, and how to get it.
//!
//! Ported from Edith's `CLIToolSpec` and `ToolProvisioning`, with two
//! differences that the platforms force.
//!
//! The first is the install route. Edith reaches for Homebrew and falls back to
//! npm. Ubuntu has apt for what the distribution ships and npm for what it does
//! not, and nothing here escalates: an apt install is an instruction Veronica
//! prints, never a `sudo` it runs on your behalf.
//!
//! The second is the catalogue itself. Edith lists `yt-dlp` for its download
//! queue, which Veronica has no equivalent of, and Homebrew, which Ubuntu has
//! no equivalent of; Veronica lists `herdr` and `ssh`, which Edith reaches
//! through paths of its own. What matches exactly is the rule that every tool
//! an extension declares has to be in here, which `every_declared_tool_is_in_the_catalogue`
//! pins.
//!
//! Finding the executable is deliberately not `which`. A desktop entry does not
//! inherit the login shell's PATH, so a tool installed into `~/.local/bin` or
//! `~/.cargo/bin` is invisible to the app while being perfectly visible in the
//! terminal the user installed it from. The search path below is PATH plus the
//! places those installers actually write to.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// How the tool is obtained on Ubuntu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "route", rename_all = "camelCase")]
pub enum Install {
    /// `apt install <package>`, which needs root and so is never run for you.
    Apt { package: &'static str },
    /// `npm install -g <package>`, which lands in the user's own prefix.
    Npm { package: &'static str },
    /// Nothing Veronica can drive; the instruction is the whole answer.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    /// The id extensions refer to, which is also the executable's usual name.
    pub id: &'static str,
    pub display_name: &'static str,
    /// What stops working without it, in the user's terms.
    pub why: &'static str,
    /// The file to look for on the search path.
    pub executable: &'static str,
    /// Arguments that make the tool print its version, for the probe.
    pub version_args: &'static [&'static str],
    pub install: Install,
    /// The exact line to run by hand, printed when an install is refused or
    /// fails. It stands alone: a user who never opens Veronica again can still
    /// act on it.
    pub instruction: &'static str,
}

impl ToolSpec {
    /// Where this tool is, or `None` when it is not installed.
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

/// The extensions that declare `id`, by title, in catalogue order.
///
/// This is the other direction of `ExtensionEntry::required_tools`, and it is
/// what turns "codex is missing" into "Agent Usage is missing something".
pub fn wanted_by(id: &str) -> Vec<&'static str> {
    crate::extensions::ENTRIES
        .iter()
        .filter(|entry| entry.required_tools.contains(&id))
        .map(|entry| entry.title)
        .collect()
}

/// Whether an extension needs every tool it declares, or any one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRule {
    All,
    Any,
}

/// Edith's policy table makes exactly one exception, and so does this.
///
/// Claude and Codex are two providers of the same numbers, so Agent Usage has
/// something to show with either one. Every other extension's tools each do a
/// job nothing else does, and missing one means the feature is missing.
pub fn rule(extension_id: &str) -> ToolRule {
    match extension_id {
        "usage" => ToolRule::Any,
        _ => ToolRule::All,
    }
}

/// The tools one extension needs, in the order it declares them.
///
/// An id with no spec is skipped rather than faked, and
/// `every_declared_tool_is_in_the_catalogue` is what stops that being silent.
pub fn required_by(entry: &crate::extensions::ExtensionEntry) -> Vec<&'static ToolSpec> {
    entry
        .required_tools
        .iter()
        .filter_map(|id| spec(id))
        .collect()
}

/// The directories a tool is looked for in, best first.
///
/// PATH comes first, so a user who has deliberately put one version ahead of
/// another gets the one they chose. The rest are the locations installers write
/// to that a desktop launch would otherwise miss: Ubuntu's snap bin, the two
/// system prefixes, and the per-user prefixes npm, cargo, bun and pip use.
pub fn search_path() -> Vec<PathBuf> {
    let named = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();

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

/// The first executable called `executable` in `directories`.
///
/// Taking the directories as an argument rather than reading the environment is
/// what makes this testable: a test can hand it a temporary tree instead of
/// mutating PATH, which two tests running at once cannot safely share.
pub fn locate_in(directories: &[PathBuf], executable: &str) -> Option<PathBuf> {
    directories
        .iter()
        .map(|directory| directory.join(executable))
        .find(|candidate| is_executable(candidate))
}

/// A regular file the current user may run. A directory named `ssh` is not a
/// tool, and neither is a file without the execute bit.
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
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

    /// The catalogue is only real if every id an extension declares resolves.
    /// Without this, adding a tool to an entry is a silent no-op.
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

    /// A file without the execute bit is a file, not a tool. Treating it as one
    /// would report the extension ready and then fail at the first run.
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

    /// PATH is honoured first, and the GUI-safe fallbacks are appended rather
    /// than replacing it: a desktop launch has to see both.
    #[test]
    fn the_search_path_covers_the_per_user_prefixes() {
        let path = search_path();
        assert!(path.contains(&PathBuf::from("/usr/bin")));
        // A build environment without HOME has no per-user prefix to cover,
        // and failing there would be reporting the harness, not the code.
        if std::env::var_os("HOME").is_some() {
            assert!(
                path.iter()
                    .any(|directory| directory.ends_with(".local/bin")),
                "~/.local/bin is where these tools most often land"
            );
        }
    }
}
