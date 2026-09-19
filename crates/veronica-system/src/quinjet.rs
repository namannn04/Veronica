//! Quinjet: reviewing pull requests and live workspace changes.
//!
//! Ported from Edith's `QuinjetClient` and `QuinjetOperations`. Quinjet is its
//! own tool with its own CLI; Edith discovers projects and worktrees through
//! it, then launches its TUI. Veronica does the same, over the same JSON
//! contract — `project list --json` and `worktree list --json` — so a machine
//! with Quinjet installed behaves identically on both platforms.
//!
//! What differs is where the TUI ends up. Edith hosts it in an embedded
//! terminal, so it has native tabs, sessions and a switcher. Veronica has no
//! embedded terminal and does not want one: the same decision Herdr's port
//! made, for the same reason. It opens Quinjet in the installed terminal, or
//! prints the exact command for a terminal you already have open. That covers
//! discovery and launch, which is what the tool is for; it does not reproduce
//! Edith's tab management, and nothing here pretends to.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Identifies Veronica to Quinjet, the way Edith identifies itself. Quinjet
/// uses it to decide how much chrome to draw when something else owns the
/// window.
pub const CLIENT: &str = "veronica";

/// One git worktree Quinjet can review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    pub head: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub current: bool,
    #[serde(default)]
    pub bare: bool,
    #[serde(default)]
    pub detached: bool,
    #[serde(default)]
    pub locked: Option<String>,
    #[serde(default)]
    pub prunable: Option<String>,
}

impl Worktree {
    /// Edith's rule: a bare repository has no working tree to review, and a
    /// prunable one points at a directory that is already gone.
    pub fn can_open(&self) -> bool {
        !self.bare && self.prunable.is_none()
    }

    /// What to call it in a list.
    pub fn label(&self) -> String {
        match &self.branch {
            Some(branch) if !branch.is_empty() => branch.clone(),
            _ if self.detached => format!("detached at {}", short_head(&self.head)),
            _ => self.path.clone(),
        }
    }
}

fn short_head(head: &str) -> String {
    head.chars().take(8).collect()
}

/// One project, which is a repository and every worktree attached to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    pub common_dir: String,
    #[serde(default)]
    pub worktrees: Vec<Worktree>,
}

impl Project {
    pub fn available_worktrees(&self) -> Vec<&Worktree> {
        self.worktrees
            .iter()
            .filter(|worktree| worktree.can_open())
            .collect()
    }

    /// Which worktree to open when the user named a project rather than one.
    /// The one they are on, or the first that can be opened.
    pub fn default_worktree(&self) -> Option<&Worktree> {
        let available = self.available_worktrees();
        available
            .iter()
            .find(|worktree| worktree.current)
            .or(available.first())
            .copied()
    }
}

/// Resolve Quinjet from the GUI-safe locations as well as PATH, for the same
/// reason Herdr is: a desktop entry does not inherit the login PATH, and these
/// tools commonly live in `~/.local/bin`.
pub fn executable() -> Option<PathBuf> {
    let named = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .map(|directory| directory.join("quinjet"));
    let fixed = [
        PathBuf::from("/usr/bin/quinjet"),
        PathBuf::from("/usr/local/bin/quinjet"),
    ];
    let local = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/bin/quinjet"));
    named.chain(fixed).chain(local).find(|path| path.is_file())
}

pub fn installed() -> bool {
    executable().is_some()
}

/// The arguments that list recent projects.
pub fn projects_args() -> Vec<String> {
    vec!["project".into(), "list".into(), "--json".into()]
}

/// The arguments that list one project's worktrees.
pub fn worktrees_args(path: &str) -> Vec<String> {
    vec![
        "-C".into(),
        path.to_string(),
        "worktree".into(),
        "list".into(),
        "--json".into(),
    ]
}

/// How the TUI should look. Edith's options, with the same argument names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    /// A named Quinjet theme.
    pub theme: Option<String>,
    /// `light` or `dark`.
    pub appearance: Option<String>,
}

/// The arguments that open one worktree in Quinjet's TUI.
///
/// Built rather than run, so the caller can print it — `open` prints, `launch`
/// runs — and so the shape is testable without Quinjet installed.
pub fn launch_args(worktree: &str, options: &LaunchOptions) -> Vec<String> {
    let mut args = vec![
        "--client".to_string(),
        CLIENT.to_string(),
        "-C".to_string(),
        worktree.to_string(),
        "tui".to_string(),
    ];
    if let Some(theme) = &options.theme {
        args.push("--theme".to_string());
        args.push(theme.clone());
    }
    if let Some(appearance) = &options.appearance {
        args.push("--appearance".to_string());
        args.push(appearance.clone());
    }
    args
}

/// Quote one argument for a shell, so a printed command can be pasted.
pub fn shell_quote(argument: &str) -> String {
    if !argument.is_empty()
        && argument
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@,+".contains(c))
    {
        return argument.to_string();
    }
    format!("'{}'", argument.replace('\'', r"'\''"))
}

/// A pasteable command line.
pub fn shell_command(executable: &str, args: &[String]) -> String {
    std::iter::once(shell_quote(executable))
        .chain(args.iter().map(|arg| shell_quote(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn run(args: &[String]) -> Result<Vec<u8>> {
    let executable = executable().context(
        "Quinjet is not installed, or is not on PATH or in ~/.local/bin. \
         Nothing else in Veronica needs it.",
    )?;
    let output = Command::new(&executable)
        .args(args)
        .output()
        .with_context(|| format!("cannot run {}", executable.display()))?;
    if !output.status.success() {
        bail!(
            "quinjet {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

/// Parse `quinjet project list --json`.
pub fn parse_projects(json: &[u8]) -> Result<Vec<Project>> {
    serde_json::from_slice(json).context("Quinjet returned a project list Veronica cannot read")
}

/// Parse `quinjet worktree list --json`.
pub fn parse_worktrees(json: &[u8]) -> Result<Vec<Worktree>> {
    serde_json::from_slice(json).context("Quinjet returned a worktree list Veronica cannot read")
}

/// The recent projects on this computer.
pub fn projects() -> Result<Vec<Project>> {
    parse_projects(&run(&projects_args())?)
}

/// The worktrees of one project.
pub fn worktrees(path: &str) -> Result<Vec<Worktree>> {
    parse_worktrees(&run(&worktrees_args(path))?)
}

/// Open a worktree in the installed terminal.
///
/// The same three terminals Herdr's launcher tries, in the same order, so both
/// features land in whatever the user actually has.
pub fn open_terminal(worktree: &str, options: &LaunchOptions) -> Result<()> {
    let executable = executable().context("Quinjet is not installed")?;
    let executable = executable.display().to_string();
    let args = launch_args(worktree, options);

    let terminals: [(&str, Vec<String>); 3] = [
        (
            "kgx",
            std::iter::once("--".to_string())
                .chain(std::iter::once(executable.clone()))
                .chain(args.clone())
                .collect(),
        ),
        (
            "gnome-terminal",
            std::iter::once("--".to_string())
                .chain(std::iter::once(executable.clone()))
                .chain(args.clone())
                .collect(),
        ),
        (
            "x-terminal-emulator",
            std::iter::once("-e".to_string())
                .chain(std::iter::once(executable))
                .chain(args)
                .collect(),
        ),
    ];
    for (terminal, arguments) in terminals {
        if Command::new(terminal)
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
        {
            return Ok(());
        }
    }
    bail!("no supported terminal application is installed")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECTS: &[u8] = br#"[
      {"name": "veronica", "commonDir": "/home/me/code/veronica/.git",
       "worktrees": [
         {"path": "/home/me/code/veronica", "head": "abc1234567", "branch": "main",
          "current": true, "bare": false, "detached": false, "locked": null, "prunable": null},
         {"path": "/home/me/code/veronica-wt", "head": "def4567890", "branch": "feature",
          "current": false, "bare": false, "detached": false, "locked": null, "prunable": null}
       ]},
      {"name": "bare-thing", "commonDir": "/srv/bare.git",
       "worktrees": [
         {"path": "/srv/bare.git", "head": "0000000", "branch": null,
          "current": false, "bare": true, "detached": false, "locked": null, "prunable": null}
       ]}
    ]"#;

    #[test]
    fn quinjets_project_list_decodes_field_for_field() {
        let projects = parse_projects(PROJECTS).unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "veronica");
        assert_eq!(projects[0].common_dir, "/home/me/code/veronica/.git");
        assert_eq!(projects[0].worktrees.len(), 2);
    }

    #[test]
    fn a_bare_or_prunable_worktree_cannot_be_opened() {
        // There is no working tree to review in one, and the other points at a
        // directory that is already gone.
        let projects = parse_projects(PROJECTS).unwrap();
        assert!(projects[1].available_worktrees().is_empty());
        assert!(projects[1].default_worktree().is_none());

        let prunable = Worktree {
            prunable: Some("gitdir file points to non-existent location".into()),
            ..projects[0].worktrees[0].clone()
        };
        assert!(!prunable.can_open());
    }

    #[test]
    fn the_default_worktree_is_the_one_you_are_on() {
        let projects = parse_projects(PROJECTS).unwrap();
        assert_eq!(
            projects[0].default_worktree().unwrap().path,
            "/home/me/code/veronica"
        );
    }

    #[test]
    fn without_a_current_worktree_the_first_openable_one_is_used() {
        let mut projects = parse_projects(PROJECTS).unwrap();
        projects[0].worktrees[0].current = false;
        projects[0].worktrees[0].bare = true;
        assert_eq!(
            projects[0].default_worktree().unwrap().path,
            "/home/me/code/veronica-wt"
        );
    }

    #[test]
    fn a_worktree_is_labelled_by_its_branch_then_its_head_then_its_path() {
        let projects = parse_projects(PROJECTS).unwrap();
        assert_eq!(projects[0].worktrees[0].label(), "main");

        let detached = Worktree {
            branch: None,
            detached: true,
            head: "abcdef1234567890".into(),
            ..projects[0].worktrees[0].clone()
        };
        assert_eq!(detached.label(), "detached at abcdef12");

        let anonymous = Worktree {
            branch: None,
            detached: false,
            ..projects[0].worktrees[0].clone()
        };
        assert_eq!(anonymous.label(), "/home/me/code/veronica");
    }

    #[test]
    fn a_list_missing_optional_fields_still_decodes() {
        // Quinjet omits nulls in some versions; a stricter decode would break
        // on a tool Veronica does not control.
        let worktrees = parse_worktrees(br#"[{"path": "/x", "head": "abc"}]"#).unwrap();
        assert_eq!(worktrees.len(), 1);
        assert!(worktrees[0].can_open());
        assert_eq!(worktrees[0].branch, None);
    }

    #[test]
    fn something_that_is_not_quinjets_json_is_an_error_not_an_empty_list() {
        assert!(parse_projects(b"not json").is_err());
        assert!(parse_worktrees(b"{}").is_err());
    }

    #[test]
    fn the_discovery_arguments_are_the_ones_quinjet_documents() {
        assert_eq!(projects_args(), ["project", "list", "--json"]);
        assert_eq!(
            worktrees_args("/srv/app"),
            ["-C", "/srv/app", "worktree", "list", "--json"]
        );
    }

    #[test]
    fn launching_identifies_veronica_and_names_the_worktree() {
        let args = launch_args("/home/me/code/veronica", &LaunchOptions::default());
        assert_eq!(
            args,
            ["--client", CLIENT, "-C", "/home/me/code/veronica", "tui"]
        );
    }

    #[test]
    fn theme_and_appearance_are_passed_through_only_when_chosen() {
        let args = launch_args(
            "/x",
            &LaunchOptions {
                theme: Some("gruvbox".into()),
                appearance: Some("light".into()),
            },
        );
        assert_eq!(
            args,
            [
                "--client",
                CLIENT,
                "-C",
                "/x",
                "tui",
                "--theme",
                "gruvbox",
                "--appearance",
                "light"
            ]
        );
    }

    #[test]
    fn a_printed_command_can_be_pasted_into_a_shell() {
        // A path with a space in it is the whole reason this quotes at all.
        let command = shell_command(
            "/usr/bin/quinjet",
            &launch_args("/home/me/My Code/app", &LaunchOptions::default()),
        );
        assert_eq!(
            command,
            "/usr/bin/quinjet --client veronica -C '/home/me/My Code/app' tui"
        );
    }

    #[test]
    fn a_quote_inside_a_path_is_escaped_rather_than_ending_the_quoting() {
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote("plain"), "plain");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("a b"), "'a b'");
    }
}
