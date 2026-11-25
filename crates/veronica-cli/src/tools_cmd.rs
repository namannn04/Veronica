use std::fmt::Write as _;

use anyhow::{Context, Result};
use veronica_system::tools::Outcome;

use crate::format::{self, Output};

#[derive(clap::Subcommand)]
pub(crate) enum ToolCommand {
    #[command(
        alias = "list",
        about = "Every tool, with whether it is installed, its version, and why"
    )]
    Ls,
    #[command(about = "Fetch one tool, when Veronica has a route it can drive")]
    Install {
        #[arg(help = "A tool id, as `ls` prints it")]
        tool: String,
    },
}

pub(crate) async fn run(command: &ToolCommand, output: Output, yes: bool) -> Result<()> {
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
                for report in reports
                    .iter()
                    .filter(|report| !report.readiness.is_installed())
                {
                    let _ = write!(rendered, "\n\n{}  {}", report.id, report.note());
                }
                rendered
            })
        }

        ToolCommand::Install { tool } => {
            let spec = veronica_core::tools::spec(tool).with_context(|| {
                let known: Vec<&str> = veronica_core::tools::CATALOG
                    .iter()
                    .map(|tool| tool.id)
                    .collect();
                format!("no tool called '{tool}'; try one of {}", known.join(", "))
            })?;
            let outcome = veronica_system::tools::install(spec, yes).await?;
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

    fn packaged_tools() -> Vec<String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions");
        std::fs::read_dir(root)
            .unwrap()
            .filter_map(|entry| std::fs::read(entry.ok()?.path().join("manifest.json")).ok())
            .filter_map(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok())
            .flat_map(|manifest| {
                manifest["requires"]["tools"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|tool| tool.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn every_tool_in_the_catalogue_is_wanted_by_an_extension() {
        let packaged = packaged_tools();
        for tool in veronica_core::tools::CATALOG {
            assert!(
                !wanted_by(tool.id).is_empty() || packaged.iter().any(|id| id == tool.id),
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
