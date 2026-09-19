//! `vr quinjet` — discover and open Quinjet review workspaces.
//!
//! Edith's `ed quinjet`, over the same Quinjet JSON contract. Discovery runs
//! against this computer or one configured SSH machine, exactly as Edith's
//! does.
//!
//! `open` prints the exact command and starts nothing; `launch` runs it. The
//! split is Edith's, and it is what makes this usable over SSH and inside a
//! terminal the user already has open.
//!
//! Edith's native session commands — its tabs, its switcher, its restart — have
//! no counterpart here, because Veronica has no embedded terminal to host them
//! in. Quinjet opens in the installed terminal instead.

use anyhow::{Context, Result};
use serde_json::json;
use veronica_core::{AppDirectories, Settings};
use veronica_machines::host::{self, Machine};
use veronica_system::quinjet::{self, LaunchOptions};

use crate::format::{self, Output};

const MACHINES_KEY: &str = "machines";

#[derive(clap::Subcommand)]
pub enum QuinjetCommand {
    /// Recent projects and their worktrees.
    Projects {
        /// Which machine to look on. Defaults to this computer.
        #[arg(long, default_value = "local")]
        machine: String,
    },
    /// Every worktree of one project.
    Worktrees {
        /// A project path, or a path inside one.
        path: String,
        #[arg(long, default_value = "local")]
        machine: String,
    },
    /// Print the command that opens a worktree, without running it.
    Open {
        /// A worktree, or a project — its current worktree is chosen.
        path: String,
        #[arg(long)]
        theme: Option<String>,
        /// light or dark.
        #[arg(long)]
        appearance: Option<String>,
        #[arg(long, default_value = "local")]
        machine: String,
    },
    /// Open a worktree in the installed terminal.
    Launch {
        path: String,
        #[arg(long)]
        theme: Option<String>,
        #[arg(long)]
        appearance: Option<String>,
    },
}

fn machines(settings: &Settings) -> Vec<Machine> {
    settings
        .get(MACHINES_KEY)
        .and_then(|value| serde_json::from_value::<Vec<Machine>>(value.clone()).ok())
        .unwrap_or_default()
}

fn find(settings: &Settings, id: &str) -> Result<Machine> {
    host::fleet(machines(settings))
        .into_iter()
        .find(|machine| machine.id == id)
        .with_context(|| format!("no machine called {id}; run `vr machines ls` for the list"))
}

/// Run a Quinjet command, here or on a machine.
///
/// The remote path runs `quinjet` over the same SSH transport the fleet probe
/// uses, so the user's own config, keys and jump hosts apply and Veronica never
/// handles a credential.
async fn discover(machine: &Machine, args: &[String]) -> Result<Vec<u8>> {
    if machine.is_local() {
        let executable = quinjet::executable().context(
            "Quinjet is not installed, or is not on PATH or in ~/.local/bin. \
             Nothing else in Veronica needs it.",
        )?;
        let output = tokio::process::Command::new(&executable)
            .args(args)
            .output()
            .await
            .with_context(|| format!("cannot run {}", executable.display()))?;
        if !output.status.success() {
            anyhow::bail!(
                "quinjet {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        return Ok(output.stdout);
    }

    let script = format!(
        "command -v quinjet >/dev/null 2>&1 || {{ echo 'quinjet is not installed there' >&2; exit 127; }}\nquinjet {}\n",
        args.iter()
            .map(|arg| quinjet::shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let output = veronica_machines::transport::run_script(
        machine,
        &script,
        veronica_machines::DEFAULT_TIMEOUT,
    )
    .await
    .with_context(|| format!("cannot reach {}", machine.name))?;
    Ok(output.into_bytes())
}

pub async fn run(
    directories: &AppDirectories,
    command: &QuinjetCommand,
    output: Output,
) -> Result<()> {
    let settings = Settings::load(&directories.settings_file())?;

    match command {
        QuinjetCommand::Projects { machine } => {
            let machine = find(&settings, machine)?;
            let projects =
                quinjet::parse_projects(&discover(&machine, &quinjet::projects_args()).await?)?;
            output.emit(&projects, || {
                if projects.is_empty() {
                    return "Quinjet has no recent projects".to_string();
                }
                let rows: Vec<Vec<String>> = projects
                    .iter()
                    .map(|project| {
                        vec![
                            project.name.clone(),
                            project
                                .default_worktree()
                                .map(|worktree| worktree.label())
                                .unwrap_or_else(|| "—".to_string()),
                            project.available_worktrees().len().to_string(),
                            project
                                .default_worktree()
                                .map(|worktree| worktree.path.clone())
                                .unwrap_or_else(|| project.common_dir.clone()),
                        ]
                    })
                    .collect();
                format::table(&["project", "on", "worktrees", "path"], &rows)
            })
        }

        QuinjetCommand::Worktrees { path, machine } => {
            let machine = find(&settings, machine)?;
            let worktrees = quinjet::parse_worktrees(
                &discover(&machine, &quinjet::worktrees_args(path)).await?,
            )?;
            output.emit(&worktrees, || {
                if worktrees.is_empty() {
                    return format!("no worktrees at {path}");
                }
                let rows: Vec<Vec<String>> = worktrees
                    .iter()
                    .map(|worktree| {
                        vec![
                            if worktree.current { "*" } else { "" }.to_string(),
                            worktree.label(),
                            worktree.path.clone(),
                            if worktree.can_open() {
                                String::new()
                            } else if worktree.bare {
                                "bare".to_string()
                            } else {
                                "prunable".to_string()
                            },
                        ]
                    })
                    .collect();
                format::table(&["", "branch", "path", ""], &rows)
            })
        }

        QuinjetCommand::Open {
            path,
            theme,
            appearance,
            machine,
        } => {
            let machine = find(&settings, machine)?;
            let options = LaunchOptions {
                theme: theme.clone(),
                appearance: appearance.clone(),
            };
            // Resolve the path first: naming a project should open the worktree
            // you are on, which is what Edith's `open` does.
            let worktree = resolve(&machine, path).await?;
            let executable = if machine.is_local() {
                quinjet::executable()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "quinjet".to_string())
            } else {
                "quinjet".to_string()
            };
            let command =
                quinjet::shell_command(&executable, &quinjet::launch_args(&worktree, &options));
            output.emit(
                &json!({ "worktree": worktree, "command": command, "machine": machine.id }),
                || command.clone(),
            )
        }

        QuinjetCommand::Launch {
            path,
            theme,
            appearance,
        } => {
            let local = Machine::local();
            let worktree = resolve(&local, path).await?;
            quinjet::open_terminal(
                &worktree,
                &LaunchOptions {
                    theme: theme.clone(),
                    appearance: appearance.clone(),
                },
            )?;
            output.emit(&json!({ "worktree": worktree }), || {
                format!("opened {worktree} in Quinjet")
            })
        }
    }
}

/// Turn whatever the user typed into a worktree Quinjet can open.
///
/// A worktree path is used as given; a project path resolves to the worktree
/// you are on. Asking Quinjet rather than guessing means the answer matches
/// what the tool itself would do.
async fn resolve(machine: &Machine, path: &str) -> Result<String> {
    let worktrees =
        quinjet::parse_worktrees(&discover(machine, &quinjet::worktrees_args(path)).await?)?;
    if worktrees.is_empty() {
        anyhow::bail!("{path} is not a git worktree Quinjet can open");
    }
    // Exactly what `Project::default_worktree` does, over a bare list.
    let openable: Vec<_> = worktrees.iter().filter(|entry| entry.can_open()).collect();
    let chosen = openable
        .iter()
        .find(|entry| entry.current)
        .or(openable.first())
        .with_context(|| format!("every worktree at {path} is bare or prunable"))?;
    Ok(chosen.path.clone())
}
