//! `vr maintenance` — App Maintenance.
//!
//! Edith's `ed maintenance`, over apt, snap and flatpak instead of Homebrew,
//! the Mac App Store and Sparkle. The rules are the same: discovery never
//! installs anything, and `update` and `remove` print their plan and stop
//! unless `--yes` is given.
//!
//! Nothing here is wrapped in `sudo` on the user's behalf. A write that needs
//! root goes through `pkexec`, which shows the desktop's own authentication
//! dialog — the user authenticates, Veronica does not escalate.

use anyhow::{Context, Result};
use serde_json::json;
use veronica_system::packages::{self, Source};

use crate::format::{self, Output};

#[derive(clap::Subcommand)]
pub enum MaintenanceCommand {
    /// What is installed, from every source on this computer.
    #[command(alias = "ls")]
    Inventory {
        /// Restrict to one source: apt, snap or flatpak.
        #[arg(long)]
        source: Option<String>,
    },
    /// What has a newer version available. Installs nothing.
    Updates {
        #[arg(long)]
        source: Option<String>,
    },
    /// Apply updates.
    ///
    /// With no names, everything from that source. Prints the plan and stops
    /// unless `--yes` is given.
    Update {
        /// Which source to update. Required, because the three take different
        /// commands and "everything everywhere" is rarely what is meant.
        source: String,
        /// Specific packages. Omit for every update from that source.
        names: Vec<String>,
        #[arg(long)]
        yes: bool,
    },
    /// What removing a package would take with it.
    ///
    /// Prints the plan and stops unless `--yes` is given. apt only: snap and
    /// flatpak have no equivalent simulation to review first.
    Remove {
        package: String,
        #[arg(long)]
        yes: bool,
    },
    /// Which package sources this computer has.
    Sources,
}

fn source(raw: &str) -> Result<Source> {
    Source::parse(raw).with_context(|| {
        let known: Vec<&str> = Source::ALL.iter().map(|s| s.title()).collect();
        format!("unknown source '{raw}'; try one of {}", known.join(", "))
    })
}

pub async fn run(command: &MaintenanceCommand, output: Output) -> Result<()> {
    match command {
        MaintenanceCommand::Sources => {
            let mut rows = Vec::new();
            let mut document = Vec::new();
            for entry in Source::ALL {
                let present = packages::available(entry).await;
                document.push(json!({
                    "source": entry.title(),
                    "available": present,
                    "needsRoot": entry.needs_root(),
                }));
                rows.push(vec![
                    entry.title().to_string(),
                    if present {
                        "installed"
                    } else {
                        "not installed"
                    }
                    .to_string(),
                    if entry.needs_root() {
                        "changes need authentication"
                    } else {
                        "changes need nothing"
                    }
                    .to_string(),
                ]);
            }
            output.emit(&document, || {
                format::table(&["source", "state", "privilege"], &rows)
            })
        }

        MaintenanceCommand::Inventory { source: filter } => {
            let wanted = filter.as_deref().map(source).transpose()?;
            let installed: Vec<_> = packages::inventory()
                .await
                .into_iter()
                .filter(|entry| wanted.is_none_or(|source| entry.source == source))
                .collect();
            output.emit(&installed, || {
                if installed.is_empty() {
                    return "nothing installed from these sources".to_string();
                }
                let rows: Vec<Vec<String>> = installed
                    .iter()
                    .map(|entry| {
                        vec![
                            entry.source.title().to_string(),
                            entry.name.clone(),
                            entry.version.clone(),
                            entry
                                .size_bytes
                                .map(veronica_system::metrics::human_bytes)
                                .unwrap_or_else(|| "—".to_string()),
                        ]
                    })
                    .collect();
                format!(
                    "{}\n\n{} package{}",
                    format::table(&["source", "name", "version", "size"], &rows),
                    installed.len(),
                    if installed.len() == 1 { "" } else { "s" }
                )
            })
        }

        MaintenanceCommand::Updates { source: filter } => {
            let wanted = filter.as_deref().map(source).transpose()?;
            let updates: Vec<_> = packages::updates()
                .await
                .into_iter()
                .filter(|entry| wanted.is_none_or(|source| entry.source == source))
                .collect();
            output.emit(&updates, || {
                if updates.is_empty() {
                    return "everything is up to date".to_string();
                }
                let rows: Vec<Vec<String>> = updates
                    .iter()
                    .map(|entry| {
                        vec![
                            entry.source.title().to_string(),
                            entry.name.clone(),
                            entry
                                .installed_version
                                .clone()
                                .unwrap_or_else(|| "—".to_string()),
                            entry.available_version.clone(),
                        ]
                    })
                    .collect();
                format!(
                    "{}\n\n{} update{} available. Nothing was installed.",
                    format::table(&["source", "name", "installed", "available"], &rows),
                    updates.len(),
                    if updates.len() == 1 { "" } else { "s" }
                )
            })
        }

        MaintenanceCommand::Update {
            source: raw,
            names,
            yes,
        } => {
            let source = source(raw)?;
            if !packages::available(source).await {
                anyhow::bail!("{} is not installed on this computer", source.title());
            }
            let argv =
                packages::privileged_command(source, &packages::update_command(source, names)?);
            let command = argv.join(" ");

            if !yes {
                // The plan is what the update would touch, so it is discovered
                // rather than assumed: naming three packages that are already
                // current should say so, not print a command anyway.
                let pending: Vec<_> = packages::updates()
                    .await
                    .into_iter()
                    .filter(|entry| entry.source == source)
                    .filter(|entry| names.is_empty() || names.contains(&entry.name))
                    .collect();
                return output.emit(
                    &json!({ "command": command, "wouldUpdate": pending }),
                    || {
                        if pending.is_empty() {
                            return format!(
                                "nothing to update from {}. Nothing was run.",
                                source.title()
                            );
                        }
                        let rows: Vec<Vec<String>> = pending
                            .iter()
                            .map(|entry| {
                                vec![
                                    entry.name.clone(),
                                    entry
                                        .installed_version
                                        .clone()
                                        .unwrap_or_else(|| "—".to_string()),
                                    entry.available_version.clone(),
                                ]
                            })
                            .collect();
                        format!(
                            "{}\n\nNothing was run. Rerun with --yes, or run it yourself:\n  {command}",
                            format::table(&["name", "installed", "available"], &rows)
                        )
                    },
                );
            }

            let result = packages::apply_update(source, names).await?;
            if !result.succeeded {
                anyhow::bail!("{} failed:\n{}", result.command, result.output);
            }
            output.emit(&result, || {
                format!("{}\n\n{}", result.command, result.output.trim_end())
            })
        }

        MaintenanceCommand::Remove { package, yes } => {
            let plan = packages::removal_plan(package).await?;
            if !yes {
                return output.emit(&plan, || {
                    use std::fmt::Write;
                    let mut out = format!("Removing {} would remove:\n", plan.package);
                    for name in &plan.removed {
                        let _ = writeln!(out, "  {name}");
                    }
                    if plan.removes_more_than_asked {
                        let _ = writeln!(out, "\nThat is more than the package you named.");
                    }
                    if !plan.now_unused.is_empty() {
                        let _ = writeln!(out, "\nLeft installed but no longer needed by anything:");
                        for name in &plan.now_unused {
                            let _ = writeln!(out, "  {name}");
                        }
                    }
                    let _ = write!(
                        out,
                        "\nNothing was removed. Rerun with --yes, or run it yourself:\n  {}",
                        plan.command
                    );
                    out
                });
            }

            let argv = packages::removal_command(package, true)?;
            let status = tokio::process::Command::new(&argv[0])
                .args(&argv[1..])
                .env("DEBIAN_FRONTEND", "noninteractive")
                .status()
                .await
                .with_context(|| format!("cannot run {}", argv.join(" ")))?;
            if !status.success() {
                anyhow::bail!("{} exited with {status}", argv.join(" "));
            }
            output.emit(&json!({ "removed": plan.removed }), || {
                format!("removed {}", plan.removed.join(", "))
            })
        }
    }
}
