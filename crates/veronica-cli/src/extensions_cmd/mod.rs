mod builtin;
mod checks;
mod packages;

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use chrono::Utc;
use serde::Serialize;
use serde_json::json;
use veronica_core::{AppDirectories, Capabilities, ExtensionAvailability, Settings};
use veronica_host::archive::ArchiveError;
use veronica_host::catalog::{CatalogStore, LoadedCatalog, Provenance, RollbackError};
use veronica_host::connection::BackendError;
use veronica_host::fetch::FetchError;
use veronica_host::registry::{self, Origin, Registry};
use veronica_host::trust::TrustError;
use veronica_host::update::AutoUpdate;
use veronica_host::Layout;
use veronica_protocol::{ExitCode, Manifest};

use crate::format::{self, Failure, Output};
use crate::routes;

#[derive(clap::Subcommand)]
pub(crate) enum ExtensionCommand {
    #[command(
        alias = "list",
        about = "Every extension: built in, installed from the catalog and sideloaded"
    )]
    Ls,
    #[command(about = "Installed versions, available updates, the catalog and automatic updates")]
    Status,
    #[command(about = "Search the extension catalog by id, title, summary or category")]
    Search { query: String },
    #[command(about = "Describe one extension and everything it needs")]
    Info { id: String },
    #[command(about = "Install an extension from the catalog, or a folder for development")]
    Install {
        #[arg(help = "A catalog id, or a folder such as ./my-extension")]
        target: String,
    },
    #[command(about = "Update one extension, or every extension that has an update")]
    Update {
        id: Option<String>,
        #[arg(long, conflicts_with = "id", help = "Update everything (the default)")]
        all: bool,
    },
    #[command(about = "Turn one extension on, even when setup still remains")]
    Enable { id: String },
    #[command(about = "Turn one extension off")]
    Disable { id: String },
    #[command(about = "Remove an installed extension; its data stays unless --purge")]
    Remove {
        id: String,
        #[arg(long, help = "Also delete the extension's data")]
        purge: bool,
    },
    #[command(about = "Install the starter extensions of a profile")]
    Setup {
        #[arg(long, value_enum, default_value = "simple")]
        profile: ProfileArg,
        #[arg(long, help = "Show what would be installed and change nothing")]
        dry_run: bool,
    },
    #[command(about = "Re-hash installed extensions against their file lists and the catalog")]
    Verify { id: Option<String> },
    #[command(
        about = "Check that extensions can run here: exec permission, backends, tools, capabilities, catalog and leftovers"
    )]
    Doctor,
    #[command(
        name = "auto-update",
        about = "Show or change automatic extension updates"
    )]
    AutoUpdate {
        #[command(subcommand)]
        change: Option<AutoUpdateChange>,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum ProfileArg {
    Simple,
    Developer,
}

#[derive(clap::Subcommand)]
pub(crate) enum AutoUpdateChange {
    #[command(about = "Update extensions automatically")]
    On,
    #[command(about = "Stop updating extensions automatically")]
    Off,
    #[command(about = "Check for updates every N hours")]
    Hours {
        #[arg(value_parser = clap::value_parser!(u32).range(1..=168))]
        hours: u32,
    },
    #[command(about = "Never update one extension automatically")]
    Exclude { id: String },
    #[command(about = "Update one extension automatically again")]
    Include { id: String },
}

pub(crate) struct Env {
    directories: AppDirectories,
    layout: Layout,
    output: Output,
    yes: bool,
}

impl Env {
    fn settings(&self) -> Result<Settings> {
        Settings::load(&self.directories.settings_file())
    }

    fn registry(&self) -> Result<Registry> {
        Registry::load(&self.layout.installed_file())
    }

    fn view(&self) -> Result<Registry> {
        registry::view(&self.layout.installed_file(), &self.settings()?)
    }

    fn catalog(&self) -> Result<LoadedCatalog> {
        CatalogStore::new(self.layout.clone()).load(Utc::now())
    }

    fn manifest(&self, id: &str) -> Result<Manifest> {
        routes::read_manifest(&self.layout, id)
    }

    fn confirm<T: Serialize>(&self, preview: &T, text: impl FnOnce() -> String) -> Result<()> {
        if self.yes {
            return Ok(());
        }
        Err(format::confirmation(self.output, preview, text))
    }
}

pub(crate) async fn run(
    directories: AppDirectories,
    command: &ExtensionCommand,
    output: Output,
    yes: bool,
) -> Result<()> {
    let env = Env {
        layout: Layout::new(&directories),
        directories,
        output,
        yes,
    };
    let outcome = match command {
        ExtensionCommand::Ls => list(&env).await,
        ExtensionCommand::Status => packages::status(&env),
        ExtensionCommand::Search { query } => packages::search(&env, query),
        ExtensionCommand::Info { id } => info(&env, id).await,
        ExtensionCommand::Install { target } => packages::install(&env, target).await,
        ExtensionCommand::Update { id, .. } => packages::update(&env, id.as_deref()).await,
        ExtensionCommand::Enable { id } => switch(&env, id, true).await,
        ExtensionCommand::Disable { id } => switch(&env, id, false).await,
        ExtensionCommand::Remove { id, purge } => packages::remove(&env, id, *purge),
        ExtensionCommand::Setup { profile, dry_run } => {
            packages::setup(&env, *profile, *dry_run).await
        }
        ExtensionCommand::Verify { id } => checks::verify(&env, id.as_deref()),
        ExtensionCommand::Doctor => checks::doctor(&env).await,
        ExtensionCommand::AutoUpdate { change } => auto_update(&env, change.as_ref()),
    };
    outcome.map_err(classify)
}

fn classify(error: anyhow::Error) -> anyhow::Error {
    if error.downcast_ref::<Failure>().is_some() {
        return error;
    }
    let code = error
        .chain()
        .find_map(|cause| {
            if cause.is::<reqwest::Error>() {
                Some(ExitCode::Unreachable)
            } else if let Some(fetch) = cause.downcast_ref::<FetchError>() {
                Some(match fetch {
                    FetchError::UnexpectedNotModified(_) => ExitCode::Unreachable,
                    _ => ExitCode::VerificationFailed,
                })
            } else if cause.is::<ArchiveError>()
                || cause.is::<TrustError>()
                || cause.is::<RollbackError>()
            {
                Some(ExitCode::VerificationFailed)
            } else {
                cause
                    .downcast_ref::<BackendError>()
                    .map(BackendError::exit_code)
            }
        })
        .unwrap_or(ExitCode::Failed);
    Failure::new(code, format!("{error:#}")).into()
}

fn not_found(id: &str) -> anyhow::Error {
    Failure::new(
        ExitCode::NotFound,
        format!("no extension called '{id}'; `vr extensions ls` lists what is here and `vr extensions search` what can be installed"),
    )
    .into()
}

async fn capabilities() -> Capabilities {
    Capabilities::resolve(&veronica_system::detect_session().await)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: String,
    title: String,
    summary: String,
    origin: Origin,
    version: String,
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    availability: Option<ExtensionAvailability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    problem: Option<String>,
}

impl Row {
    fn state(&self) -> String {
        match (&self.problem, &self.availability, self.origin) {
            (Some(problem), _, _) => format!("broken: {problem}"),
            (None, Some(availability), _) => crate::availability_label(availability),
            (None, None, Origin::Sideload) => "development".to_string(),
            (None, None, _) => "installed".to_string(),
        }
    }
}

async fn list(env: &Env) -> Result<()> {
    let capabilities = capabilities().await;
    let rows: Vec<Row> = env
        .view()?
        .extensions
        .into_iter()
        .map(|(id, installed)| {
            if installed.origin == Origin::Builtin {
                let entry = veronica_core::extensions::entry(&id)
                    .context("built-in entries come from the catalogue")?;
                return Ok(Row {
                    title: entry.title.to_string(),
                    summary: entry.subtitle.to_string(),
                    origin: Origin::Builtin,
                    version: installed.version.to_string(),
                    enabled: installed.enabled,
                    availability: Some(entry.availability(&capabilities)),
                    problem: None,
                    id,
                });
            }
            let manifest = env.manifest(&id);
            Ok(Row {
                title: manifest
                    .as_ref()
                    .map_or_else(|_| id.clone(), |manifest| manifest.title.clone()),
                summary: manifest
                    .as_ref()
                    .map(|manifest| manifest.summary.clone())
                    .unwrap_or_default(),
                origin: installed.origin,
                version: installed.version.to_string(),
                enabled: installed.enabled,
                availability: None,
                problem: manifest.err().map(|error| format!("{error:#}")),
                id,
            })
        })
        .collect::<Result<_>>()?;
    env.output.emit(&rows, || {
        let table: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                vec![
                    row.id.clone(),
                    row.title.clone(),
                    origin_label(row.origin).to_string(),
                    row.version.clone(),
                    if row.enabled { "on" } else { "off" }.to_string(),
                    row.state(),
                ]
            })
            .collect();
        format::table(
            &["id", "title", "origin", "version", "enabled", "state"],
            &table,
        )
    })
}

pub(crate) fn origin_label(origin: Origin) -> &'static str {
    match origin {
        Origin::Builtin => "built in",
        Origin::Catalog => "catalog",
        Origin::Sideload => "sideload",
    }
}

pub(crate) fn provenance_label(provenance: Provenance) -> &'static str {
    match provenance {
        Provenance::Seed => "seed",
        Provenance::Cache => "cache",
        Provenance::Network => "network",
    }
}

async fn info(env: &Env, id: &str) -> Result<()> {
    let installed = env.registry()?;
    if installed.get(id).is_none() {
        if let Some(entry) = veronica_core::extensions::entry(id) {
            let capabilities = capabilities().await;
            return builtin::info(&env.settings()?, &capabilities, entry, env.output).await;
        }
    }
    packages::info(env, &installed, id)
}

async fn switch(env: &Env, id: &str, enabled: bool) -> Result<()> {
    if env.registry()?.get(id).is_some() {
        let entry = packages::installer(env, false).set_enabled(id, enabled)?;
        let report = json!({ "id": id, "origin": entry.origin, "enabled": entry.enabled });
        return env.output.emit(&report, || {
            format!("{} {id}", if enabled { "enabled" } else { "disabled" })
        });
    }
    let entry = veronica_core::extensions::entry(id).ok_or_else(|| not_found(id))?;
    let capabilities = capabilities().await;
    builtin::switch(
        &env.directories.settings_file(),
        &capabilities,
        entry,
        enabled,
        env.output,
    )
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AutoUpdateReport {
    enabled: bool,
    interval_hours: u32,
    excluded: Vec<String>,
}

impl From<&AutoUpdate> for AutoUpdateReport {
    fn from(settings: &AutoUpdate) -> Self {
        Self {
            enabled: settings.enabled,
            interval_hours: settings.interval_hours,
            excluded: settings.disabled.iter().cloned().collect(),
        }
    }
}

fn auto_update(env: &Env, change: Option<&AutoUpdateChange>) -> Result<()> {
    let path = env.directories.settings_file();
    let mut settings = env.settings()?;
    let mut preferences = AutoUpdate::from_settings(&settings);
    if let Some(change) = change {
        match change {
            AutoUpdateChange::On => preferences.enabled = true,
            AutoUpdateChange::Off => preferences.enabled = false,
            AutoUpdateChange::Hours { hours } => preferences.interval_hours = *hours,
            AutoUpdateChange::Exclude { id } => {
                preferences.disabled.insert(id.clone());
            }
            AutoUpdateChange::Include { id } => {
                preferences.disabled.remove(id);
            }
        }
        preferences.write_settings(&mut settings);
        settings.save(&path)?;
    }
    let report = AutoUpdateReport::from(&preferences);
    env.output.emit(&report, || {
        let mut text = format!(
            "automatic updates  {}\nchecked every      {}h",
            if report.enabled { "on" } else { "off" },
            report.interval_hours
        );
        if !report.excluded.is_empty() {
            text.push_str("\nexcluded           ");
            text.push_str(&report.excluded.join(", "));
        }
        text
    })
}

pub(crate) fn source_path(target: &str) -> Option<PathBuf> {
    (target.contains('/') || target.starts_with('.')).then(|| PathBuf::from(target))
}

#[cfg(test)]
mod tests {
    use super::{classify, source_path};
    use crate::format::Failure;
    use veronica_host::fetch::FetchError;
    use veronica_protocol::ExitCode;

    #[test]
    fn folders_are_told_apart_from_catalog_ids() {
        assert!(source_path("./probe").is_some());
        assert!(source_path("../probe").is_some());
        assert!(source_path("/tmp/probe").is_some());
        assert!(source_path("media").is_none());
    }

    #[test]
    fn failures_map_onto_the_exit_code_table() {
        let code = |error: anyhow::Error| classify(error).downcast::<Failure>().unwrap().code;
        let mismatch = FetchError::SizeMismatch {
            expected: 1,
            actual: 2,
        };
        assert_eq!(
            code(anyhow::Error::from(mismatch).context("installing")),
            ExitCode::VerificationFailed
        );
        assert_eq!(code(anyhow::anyhow!("plain")), ExitCode::Failed);
        assert_eq!(
            code(Failure::new(ExitCode::NotFound, "gone").into()),
            ExitCode::NotFound
        );
    }
}
