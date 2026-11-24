use std::fmt::Write as _;

use anyhow::Result;
use serde_json::Value;
use veronica_host::launcher::{self, LaunchEnvironment};
use veronica_host::mcp::clients::{Client, Homes, Selection};
use veronica_host::mcp::{self, Action, Report, ServerEntry, StatusReport};
use veronica_protocol::ExitCode;

use crate::format::{self, Failure, Output};

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum ClientArg {
    Claude,
    Codex,
    Cursor,
    Vscode,
    All,
}

impl ClientArg {
    fn selection(self) -> Selection {
        match self {
            Self::Claude => Selection::One(Client::Claude),
            Self::Codex => Selection::One(Client::Codex),
            Self::Cursor => Selection::One(Client::Cursor),
            Self::Vscode => Selection::One(Client::Vscode),
            Self::All => Selection::All,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Change {
    Install,
    Uninstall,
}

fn homes() -> Result<Homes> {
    Homes::current().ok_or_else(|| {
        Failure::new(
            ExitCode::Unavailable,
            "HOME must be set to an absolute path to find the MCP clients' settings",
        )
        .into()
    })
}

fn server() -> Result<ServerEntry> {
    launcher::resolve(&LaunchEnvironment::current())
        .map(|launch| ServerEntry::from_launch(&launch))
        .map_err(|unsupported| Failure::new(ExitCode::Unavailable, unsupported.reason).into())
}

fn action_label(action: Action) -> &'static str {
    match action {
        Action::Created => "created",
        Action::Updated => "updated",
        Action::Unchanged => "unchanged",
        Action::Skipped => "skipped: not installed",
        Action::NotFound => "not installed",
        Action::Removed => "removed",
        Action::Refused => "refused",
        Action::Failed => "failed",
    }
}

fn render(report: &Report) -> String {
    let rows: Vec<Vec<String>> = report
        .clients
        .iter()
        .map(|client| {
            let note = client
                .reason
                .clone()
                .or_else(|| {
                    client
                        .backup
                        .as_ref()
                        .map(|backup| format!("backup {}", backup.display()))
                })
                .unwrap_or_default();
            vec![
                client.client.to_string(),
                action_label(client.action).to_string(),
                client.path.display().to_string(),
                note,
            ]
        })
        .collect();
    let noted = rows.iter().any(|row| !row[3].is_empty());
    let rows: Vec<Vec<String>> = rows
        .into_iter()
        .map(|mut row| {
            if !noted {
                row.truncate(3);
            }
            row
        })
        .collect();
    let headers: &[&str] = if noted {
        &["client", "action", "file", "note"]
    } else {
        &["client", "action", "file"]
    };
    let mut text = format::table(headers, &rows);
    for client in &report.clients {
        if client.action == Action::Refused {
            if let Some(snippet) = &client.snippet {
                let _ = write!(
                    text,
                    "\n\nAdd this to {} yourself:\n{snippet}",
                    client.path.display()
                );
            }
        }
    }
    text
}

fn outcome(report: &Report) -> Option<Failure> {
    let failed = |action: Action| report.clients.iter().any(|client| client.action == action);
    if failed(Action::Refused) || failed(Action::Failed) {
        Some(Failure::new(
            ExitCode::Failed,
            "some MCP clients could not be changed; the report says why",
        ))
    } else if failed(Action::NotFound) {
        Some(Failure::new(
            ExitCode::NotFound,
            "that MCP client is not installed on this computer",
        ))
    } else {
        None
    }
}

fn finish(report: &Report, output: Output) -> Result<()> {
    let Some(failure) = outcome(report) else {
        return output.emit(report, || render(report));
    };
    match output {
        Output::Json => {
            let mut document = serde_json::to_value(report)?;
            if let Value::Object(fields) = &mut document {
                fields.insert(
                    "error".to_string(),
                    format::error_document(failure.code, &failure.message)["error"].take(),
                );
            }
            format::document(&document);
        }
        Output::Text => println!("{}", render(report)),
    }
    Err(failure.into())
}

pub(crate) fn change(
    change: Change,
    client: ClientArg,
    dry_run: bool,
    output: Output,
    yes: bool,
) -> Result<()> {
    let homes = homes()?;
    let selection = client.selection();
    let preview = dry_run || !yes;
    let report = match change {
        Change::Install => mcp::install(&homes, selection, &server()?, preview),
        Change::Uninstall => mcp::uninstall(&homes, selection, preview),
    };
    if !dry_run && !yes {
        return Err(format::confirmation(output, &report, || render(&report)));
    }
    finish(&report, output)
}

fn render_status(report: &StatusReport) -> String {
    let rows: Vec<Vec<String>> = report
        .clients
        .iter()
        .map(|client| {
            let state = match (client.found, client.present, client.matches) {
                (_, true, true) => "registered".to_string(),
                (_, true, false) => "registered with another command".to_string(),
                (true, false, _) => "not registered".to_string(),
                (false, false, _) => "not installed".to_string(),
            };
            vec![
                client.client.to_string(),
                client.reason.clone().unwrap_or(state),
                client.path.display().to_string(),
            ]
        })
        .collect();
    let command = std::iter::once(report.server.command.as_str())
        .chain(report.server.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "Server  {command}\n\n{}",
        format::table(&["client", "state", "file"], &rows)
    )
}

pub(crate) fn status(client: ClientArg, output: Output) -> Result<()> {
    let report = mcp::status(&homes()?, client.selection(), &server()?);
    output.emit(&report, || render_status(&report))
}
