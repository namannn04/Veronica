//! `vr tools` — does this computer have the programs Veronica shells out to?
//!
//! Edith's `ed tools ls`, over Veronica's catalogue. It looks for each tool,
//! asks it for its version, and says which extension wanted it. Nothing here
//! writes a setting, needs Veronica to be running, or can remove a tool:
//! uninstalling stays with apt, npm or `rm`.
//!
//! `vr tools` on its own runs `ls`, and `list` is the same command.
//!
//! Every tool is listed on every run, whether or not the extension that wants
//! it is switched on, because "why is Herdr empty" is asked by people who have
//! not thought about extensions at all.

use anyhow::Result;
use serde::Serialize;
use veronica_core::extensions::ENTRIES;
use veronica_core::tools::ToolSpec;
use veronica_system::tools::Readiness;

use crate::format::{self, Output};

#[derive(clap::Subcommand)]
pub enum ToolCommand {
    /// Every tool, with whether it is installed, its version, and why.
    #[command(alias = "list")]
    Ls,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolReport {
    id: &'static str,
    display_name: &'static str,
    why: &'static str,
    /// The extensions that named this tool, so a missing one points somewhere.
    wanted_by: Vec<&'static str>,
    instruction: &'static str,
    #[serde(flatten)]
    readiness: Readiness,
}

impl ToolReport {
    /// What to do about a tool that is not ready.
    ///
    /// A tool that is present but will not run needs the diagnosis, not the
    /// install line: telling someone to install what they already have is how
    /// a broken symlink turns into an afternoon.
    fn note(&self) -> String {
        match &self.readiness {
            Readiness::Error { detail } => detail.clone(),
            _ => format!("{} {}", self.why, self.instruction),
        }
    }
}

/// The extensions that declare `tool`, by title, in catalogue order.
fn wanted_by(tool: &ToolSpec) -> Vec<&'static str> {
    ENTRIES
        .iter()
        .filter(|entry| entry.required_tools.contains(&tool.id))
        .map(|entry| entry.title)
        .collect()
}

pub async fn run(command: &ToolCommand, output: Output) -> Result<()> {
    match command {
        ToolCommand::Ls => {
            let reports: Vec<ToolReport> = veronica_system::tools::catalogue()
                .await
                .into_iter()
                .map(|(tool, readiness)| ToolReport {
                    id: tool.id,
                    display_name: tool.display_name,
                    why: tool.why,
                    wanted_by: wanted_by(tool),
                    instruction: tool.instruction,
                    readiness,
                })
                .collect();

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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tool exists because something asked for it. One that nothing
    /// declares is a row the user can do nothing useful with.
    #[test]
    fn every_tool_in_the_catalogue_is_wanted_by_an_extension() {
        for tool in veronica_core::tools::CATALOG {
            assert!(
                !wanted_by(tool).is_empty(),
                "no extension declares '{}'",
                tool.id
            );
        }
    }

    #[test]
    fn a_tool_two_extensions_want_names_both() {
        let ssh = veronica_core::tools::spec("ssh").unwrap();
        assert!(wanted_by(ssh).contains(&"Machines"));
    }

    fn report(readiness: Readiness) -> ToolReport {
        let tool = veronica_core::tools::spec("codex").unwrap();
        ToolReport {
            id: tool.id,
            display_name: tool.display_name,
            why: tool.why,
            wanted_by: wanted_by(tool),
            instruction: tool.instruction,
            readiness,
        }
    }

    #[test]
    fn a_missing_tool_is_told_how_to_be_installed() {
        assert!(report(Readiness::Uninstalled)
            .note()
            .contains("npm install -g @openai/codex"));
    }

    /// The install line is wrong for a tool that is already on disk, so the
    /// diagnosis replaces it rather than following it.
    #[test]
    fn a_broken_tool_is_diagnosed_rather_than_reinstalled() {
        let note = report(Readiness::Error {
            detail: "/usr/bin/codex is there, but it exited 127.".to_string(),
        })
        .note();
        assert_eq!(note, "/usr/bin/codex is there, but it exited 127.");
        assert!(!note.contains("npm install"));
    }
}
