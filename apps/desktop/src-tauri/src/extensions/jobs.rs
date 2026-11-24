use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use veronica_core::fsutil::write_atomic;
use veronica_host::bundled::{install_preferred, Bundled};
use veronica_host::catalog::CatalogStore;
use veronica_host::connection::Launch;
use veronica_host::fetch::HttpTransport;
use veronica_host::update::{plan, AutoUpdate, Decision, Skip, Trigger};
use veronica_host::Installer;
use veronica_protocol::manifest::ActivationMode;
use veronica_protocol::rpc::RunMode;
use veronica_protocol::Manifest;

use super::host::Host;

pub(crate) const JOB: &str = "extension-job";
pub(crate) const CHANGED: &str = "extensions-changed";
const HISTORY: usize = 50;
const INTERRUPTED: &str = "the app closed before this finished";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Kind {
    Install,
    Sideload,
    Update,
    Enable,
    Disable,
    Remove,
    Purge,
    Rollback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Phase {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Job {
    pub id: u64,
    pub kind: Kind,
    pub extension: String,
    pub phase: Phase,
    pub message: Option<String>,
    pub error: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Action {
    pub kind: Kind,
    pub extension: String,
    pub source: Option<PathBuf>,
}

impl Action {
    pub(crate) fn on(kind: Kind, extension: String) -> Self {
        Self {
            kind,
            extension,
            source: None,
        }
    }

    pub(crate) fn sideload(source: PathBuf) -> Result<Self> {
        let path = source.join("manifest.json");
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let manifest =
            Manifest::parse(&raw).with_context(|| format!("{} is invalid", path.display()))?;
        Ok(Self {
            kind: Kind::Sideload,
            extension: manifest.id,
            source: Some(source),
        })
    }
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[derive(Debug)]
pub(crate) struct Jobs {
    file: PathBuf,
    list: Mutex<Vec<Job>>,
    turn: tokio::sync::Mutex<()>,
}

impl Jobs {
    pub(crate) fn load(file: PathBuf) -> Self {
        let mut list: Vec<Job> = std::fs::read(&file)
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        for job in &mut list {
            if matches!(job.phase, Phase::Queued | Phase::Running) {
                job.phase = Phase::Failed;
                job.error = Some(INTERRUPTED.to_string());
                job.finished_at = Some(now());
            }
        }
        Self {
            file,
            list: Mutex::new(list),
            turn: tokio::sync::Mutex::new(()),
        }
    }

    pub(crate) fn list(&self) -> Vec<Job> {
        self.list
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn is_busy(&self, extension: &str) -> bool {
        self.list().iter().any(|job| {
            job.extension == extension && matches!(job.phase, Phase::Queued | Phase::Running)
        })
    }

    fn change(&self, change: impl FnOnce(&mut Vec<Job>) -> Job) -> Job {
        let mut list = self.list.lock().unwrap_or_else(PoisonError::into_inner);
        let job = change(&mut list);
        let excess = list.len().saturating_sub(HISTORY);
        list.drain(..excess);
        match serde_json::to_vec_pretty(&*list) {
            Ok(body) => {
                if let Err(error) = write_atomic(&self.file, &body) {
                    tracing::warn!(target: "veronica", "cannot save extension jobs: {error:#}");
                }
            }
            Err(error) => {
                tracing::warn!(target: "veronica", "cannot encode extension jobs: {error}");
            }
        }
        job
    }

    fn create(&self, action: &Action) -> Job {
        self.change(|list| {
            let job = Job {
                id: list.iter().map(|job| job.id).max().unwrap_or(0) + 1,
                kind: action.kind,
                extension: action.extension.clone(),
                phase: Phase::Queued,
                message: None,
                error: None,
                started_at: now(),
                finished_at: None,
            };
            list.push(job.clone());
            job
        })
    }

    fn update(&self, id: u64, apply: impl FnOnce(&mut Job)) -> Job {
        self.change(|list| {
            let position = list.iter().position(|job| job.id == id);
            let job = position.map(|position| &mut list[position]);
            match job {
                Some(job) => {
                    apply(job);
                    job.clone()
                }
                None => unreachable!("jobs are only dropped once they are finished"),
            }
        })
    }
}

fn report(host: &Host, id: u64, apply: impl FnOnce(&mut Job)) {
    host.emit(JOB, json!(host.jobs().update(id, apply)));
}

pub(crate) fn submit(host: &Arc<Host>, action: Action) -> Job {
    let job = host.jobs().create(&action);
    host.emit(JOB, json!(job));
    let host = Arc::clone(host);
    let id = job.id;
    tokio::spawn(async move {
        let _turn = host.jobs().turn.lock().await;
        report(&host, id, |job| {
            job.phase = Phase::Running;
            job.message = Some(describe(action.kind).to_string());
        });
        let outcome = run(&host, &action).await;
        report(&host, id, |job| {
            job.finished_at = Some(now());
            job.message = None;
            match outcome {
                Ok(()) => job.phase = Phase::Succeeded,
                Err(error) => {
                    job.phase = Phase::Failed;
                    job.error = Some(format!("{error:#}"));
                }
            }
        });
        host.emit(CHANGED, json!({ "extension": action.extension }));
    });
    job
}

fn describe(kind: Kind) -> &'static str {
    match kind {
        Kind::Install => "Downloading and verifying",
        Kind::Sideload => "Installing from a folder",
        Kind::Update => "Downloading the update",
        Kind::Enable => "Turning on",
        Kind::Disable => "Turning off",
        Kind::Remove | Kind::Purge => "Removing",
        Kind::Rollback => "Restoring the previous version",
    }
}

fn skipped(skip: Skip) -> &'static str {
    match skip {
        Skip::NotFromCatalog => "it was not installed from the catalog",
        Skip::AutoUpdateOff => "automatic updates are off",
        Skip::NotInCatalog => "it is no longer in the catalog",
        Skip::NoCompatibleVersion => "no published version fits this app",
        Skip::UpToDate => "it is already up to date",
    }
}

pub(crate) fn fetch(installer: &Installer, id: &str, update: bool) -> Result<()> {
    let catalog = CatalogStore::new(installer.layout().clone())
        .load(Utc::now())?
        .catalog;
    if update {
        let decisions = plan(
            &installer.registry()?,
            &catalog,
            installer.host(),
            &AutoUpdate::default(),
            Trigger::Manual,
        );
        match decisions.get(id) {
            Some(Decision::Update { .. }) => {}
            Some(Decision::Skip(skip)) => bail!("{id} was not updated: {}", skipped(*skip)),
            None => bail!("{id} is not installed"),
        }
    }
    let bundled = Bundled::current()?;
    install_preferred(
        installer,
        &catalog,
        bundled.as_ref(),
        &HttpTransport::new()?,
        id,
    )?;
    Ok(())
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .context("the job stopped unexpectedly")?
}

async fn restart(host: &Host, id: &str) {
    host.supervisor().stop(id).await;
    host.forget(id);
    let background = Launch::prepare(host.layout(), id, RunMode::Service).is_ok_and(|launch| {
        launch.enabled && launch.manifest.activation.mode == ActivationMode::Background
    });
    if background {
        if let Err(error) = host.supervisor().restart(id).await {
            tracing::warn!(target: "veronica", "cannot start the {id} extension: {error}");
        }
    }
}

async fn run(host: &Host, action: &Action) -> Result<()> {
    let installer = host.installer();
    let id = action.extension.clone();
    let source = action.source.clone();
    let kind = action.kind;
    if matches!(kind, Kind::Disable | Kind::Remove | Kind::Purge) {
        host.supervisor().stop(&id).await;
        host.forget(&id);
    }
    blocking(move || match kind {
        Kind::Install => fetch(&installer, &id, false),
        Kind::Update => fetch(&installer, &id, true),
        Kind::Sideload => {
            let source = source.context("a folder is required")?;
            installer.install_sideload(&source).map(drop)
        }
        Kind::Enable => installer.set_enabled(&id, true).map(drop),
        Kind::Disable => installer.set_enabled(&id, false).map(drop),
        Kind::Remove => installer.remove(&id, false),
        Kind::Purge => installer.remove(&id, true),
        Kind::Rollback => installer.rollback(&id).map(drop),
    })
    .await?;
    if !matches!(kind, Kind::Disable | Kind::Remove | Kind::Purge) {
        restart(host, &action.extension).await;
    }
    Ok(())
}
