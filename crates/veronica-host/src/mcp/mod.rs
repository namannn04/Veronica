pub mod clients;
mod file;

use std::path::PathBuf;

use serde::Serialize;

use crate::launcher::Launch;
use clients::{Client, Homes, Registration, Selection};
use file::Edit;

pub const SERVER_NAME: &str = "veronica";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerEntry {
    pub command: String,
    pub args: Vec<String>,
}

impl ServerEntry {
    pub fn from_launch(launch: &Launch) -> Self {
        let mut args = launch.args.clone();
        args.push("mcp".to_string());
        Self {
            command: launch.command.clone(),
            args,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Created,
    Updated,
    Unchanged,
    Skipped,
    NotFound,
    Removed,
    Refused,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientReport {
    pub client: Client,
    pub path: PathBuf,
    pub action: Action,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerEntry>,
    pub clients: Vec<ClientReport>,
}

impl Report {
    pub fn succeeded(&self) -> bool {
        self.clients.iter().all(|client| {
            !matches!(
                client.action,
                Action::Refused | Action::Failed | Action::NotFound
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatus {
    pub client: Client,
    pub path: PathBuf,
    pub found: bool,
    pub present: bool,
    pub matches: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusReport {
    pub server: ServerEntry,
    pub clients: Vec<ClientStatus>,
}

pub fn install(homes: &Homes, selection: Selection, server: &ServerEntry, dry_run: bool) -> Report {
    let clients = run(
        homes,
        selection,
        dry_run,
        |client, text| client.format().install(text, server),
        |replaced| {
            if replaced {
                Action::Updated
            } else {
                Action::Created
            }
        },
    )
    .into_iter()
    .map(|(client, mut report)| {
        if dry_run || report.action == Action::Refused {
            report.snippet = Some(client.format().snippet(server));
        }
        report
    })
    .collect();
    Report {
        dry_run,
        server: Some(server.clone()),
        clients,
    }
}

pub fn uninstall(homes: &Homes, selection: Selection, dry_run: bool) -> Report {
    let clients = run(
        homes,
        selection,
        dry_run,
        |client, text| client.format().uninstall(text),
        |_| Action::Removed,
    )
    .into_iter()
    .map(|(_, report)| report)
    .collect();
    Report {
        dry_run,
        server: None,
        clients,
    }
}

pub fn status(homes: &Homes, selection: Selection, server: &ServerEntry) -> StatusReport {
    let clients = selection
        .clients()
        .into_iter()
        .map(|client| {
            let path = client.config_file(homes);
            let mut status = ClientStatus {
                client,
                path: path.clone(),
                found: client.is_installed(homes),
                present: false,
                matches: false,
                reason: None,
            };
            let registration = file::read_optional(&path)
                .map_err(|error| format!("{error:#}"))
                .and_then(|text| client.format().registration(text.as_deref(), server));
            match registration {
                Ok(registration) => {
                    status.present = registration != Registration::Absent;
                    status.matches = registration == Registration::Matches;
                }
                Err(reason) => status.reason = Some(reason),
            }
            status
        })
        .collect();
    StatusReport {
        server: server.clone(),
        clients,
    }
}

fn run(
    homes: &Homes,
    selection: Selection,
    dry_run: bool,
    edit: impl Fn(Client, Option<&str>) -> Edit,
    written: impl Fn(bool) -> Action,
) -> Vec<(Client, ClientReport)> {
    selection
        .clients()
        .into_iter()
        .map(|client| {
            let path = client.config_file(homes);
            let mut report = ClientReport {
                client,
                path: path.clone(),
                action: Action::Unchanged,
                backup: None,
                reason: None,
                snippet: None,
            };
            if !client.is_installed(homes) {
                report.action = match selection {
                    Selection::All => Action::Skipped,
                    Selection::One(_) => Action::NotFound,
                };
                return (client, report);
            }
            match file::apply(&path, dry_run, &|text| edit(client, text)) {
                Ok(applied) => {
                    report.backup = applied.backup;
                    report.action = match applied.edit {
                        Edit::Keep => Action::Unchanged,
                        Edit::Refuse(reason) => {
                            report.reason = Some(reason);
                            Action::Refused
                        }
                        Edit::Write { replaced, .. } => written(replaced),
                    };
                }
                Err(error) => {
                    report.action = Action::Failed;
                    report.reason = Some(format!("{error:#}"));
                }
            }
            (client, report)
        })
        .collect()
}
