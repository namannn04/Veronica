//! `vr tools` — does this computer have the programs Veronica shells out to?
//!
//! Edith's `ed tools ls`, over Veronica's catalogue. It looks for each tool,
//! asks it for its version, and says which extension wanted it. Nothing here
//! writes a setting, needs Veronica to be running, or can remove a tool:
//! uninstalling stays with apt, npm or `rm`.
//!
//! `vr tools` on its own runs `ls`, and `list` is the same command.
//!
//! `install` reports a tool that is already there rather than reinstalling it,
//! and can never remove one: uninstalling stays with apt, npm or `rm`.
//!
//! Every tool is listed on every run, whether or not the extension that wants
//! it is switched on, because "why is Herdr empty" is asked by people who have
//! not thought about extensions at all.

use anyhow::{Context, Result};
use veronica_system::tools::Outcome;

use crate::format::{self, Output};

#[derive(clap::Subcommand)]
pub enum ToolCommand {
    /// Every tool, with whether it is installed, its version, and why.
    #[command(alias = "list")]
    Ls,
    /// Fetch one tool, when Veronica has a route it can drive.
    Install {
        /// A tool id, as `ls` prints it.
        tool: String,
        /// Authenticate through the desktop's own dialog for a route that
        /// needs root. Without it, that command is printed rather than run.
        #[arg(long)]
        yes: bool,
    },
}

pub async fn run(command: &ToolCommand, output: Output) -> Result<()> {
    match command {
        ToolCommand::Ls => {
            let reports = veronica_system::tools::catalogue().await;

            output.emit(&reports, || {
                let rows: Vec<Vec<String>> = reports
                    .iter()
                    .map(|report| {
                        vec![
                            report.id.to_string(),
                            report.readiness.summary(),
                            report.wanted_by.join(", "),
                        ]
                    })
                    .collect();
                let mut rendered = format::table(&["tool", "version", "wanted by"], &rows);
                // What to do about it goes under the table rather than in a
                // fourth column: an instruction is a sentence, and a column
                // wide enough for one makes the other three unreadable.
                for report in reports
                    .iter()
                    .filter(|report| !report.readiness.is_installed())
                {
                    rendered.push_str(&format!("\n\n{}  {}", report.id, report.note()));
                }
                rendered
            })
        }

        ToolCommand::Install { tool, yes } => {
            let spec = veronica_core::tools::spec(tool).with_context(|| {
                let known: Vec<&str> = veronica_core::tools::CATALOG
                    .iter()
                    .map(|tool| tool.id)
                    .collect();
                format!("no tool called '{tool}'; try one of {}", known.join(", "))
            })?;
            let outcome = veronica_system::tools::install(spec, *yes).await?;
            // A route Veronica cannot drive is not a failure of this command:
            // it ran, and the answer is the instruction. Scripts read the
            // outcome rather than the exit code to tell the two apart.
            output.emit(&outcome, || match &outcome {
                Outcome::AlreadyInstalled { path, version } => {
                    format!(
                        "{} {version} is already installed at {path}",
                        spec.display_name
                    )
                }
                Outcome::Installed { path, version } => {
                    format!("installed {} {version} at {path}", spec.display_name)
                }
                Outcome::NotRun {
                    command,
                    instruction,
                    reason,
                } => match command {
                    Some(command) => format!("{reason}\n\n{command}"),
                    None => format!("{reason}\n\n{instruction}"),
                },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use veronica_core::tools::wanted_by;

    /// Every tool exists because something asked for it. One that nothing
    /// declares is a row the user can do nothing useful with.
    #[test]
    fn every_tool_in_the_catalogue_is_wanted_by_an_extension() {
        for tool in veronica_core::tools::CATALOG {
            assert!(
                !wanted_by(tool.id).is_empty(),
                "no extension declares '{}'",
                tool.id
            );
        }
    }

    #[test]
    fn a_tool_names_the_extension_that_wanted_it() {
        assert!(wanted_by("ssh").contains(&"Machines"));
    }
}
