use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use veronica_host::catalog::{CatalogStore, LoadedCatalog, Provenance};
use veronica_host::fetch::HttpTransport;
use veronica_host::registry::Origin;
use veronica_host::supervisor::Status;
use veronica_host::update::{plan, resolve, AutoUpdate, Decision, Trigger};
use veronica_host::HostIdentity;
use veronica_protocol::catalog::{CatalogExtension, CatalogProfile, Category};
use veronica_protocol::Manifest;

use super::host::Host;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Row {
    pub id: String,
    pub version: String,
    pub origin: Origin,
    pub enabled: bool,
    pub installed_at: String,
    pub previous: Option<String>,
    pub state: &'static str,
    pub pid: Option<u32>,
    pub update: Option<String>,
    pub open_pages: Vec<String>,
    pub surfaces: Vec<veronica_host::surfaces::Surface>,
    pub manifest: Option<Manifest>,
}

pub(crate) async fn rows(host: &Host) -> Result<Vec<Row>> {
    let registry = host.registry()?;
    let catalog = CatalogStore::new(host.layout().clone())
        .load(Utc::now())?
        .catalog;
    let decisions = plan(
        &registry,
        &catalog,
        &HostIdentity::current(),
        &AutoUpdate::default(),
        Trigger::Manual,
    );
    let mut rows = Vec::new();
    for (id, entry) in &registry.extensions {
        let (state, pid) = match host.supervisor().status(id).await {
            Status::Running { pid } => ("running", pid),
            Status::Stopped => ("stopped", None),
            Status::CrashLooping => ("crashLooping", None),
        };
        rows.push(Row {
            id: id.clone(),
            version: entry.version.to_string(),
            origin: entry.origin,
            enabled: entry.enabled,
            installed_at: entry.installed_at.clone(),
            previous: entry.previous.map(|version| version.to_string()),
            state,
            pid,
            update: match decisions.get(id) {
                Some(Decision::Update { to, .. }) => Some(to.to_string()),
                _ => None,
            },
            open_pages: host.open_pages(id),
            surfaces: host.surfaces(id),
            manifest: host.manifest(id).ok(),
        });
    }
    Ok(rows)
}

pub(crate) async fn row(host: &Host, id: &str) -> Result<Row> {
    rows(host)
        .await?
        .into_iter()
        .find(|row| row.id == id)
        .with_context(|| format!("{id} is not installed"))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Offer {
    #[serde(flatten)]
    pub entry: CatalogExtension,
    pub installed: Option<String>,
    pub available: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Shelf {
    pub revision: u64,
    pub generated_at: String,
    pub stale: bool,
    pub provenance: &'static str,
    pub categories: Vec<Category>,
    pub profiles: Vec<CatalogProfile>,
    pub extensions: Vec<Offer>,
}

fn matches(entry: &CatalogExtension, query: &str, category: Option<&str>) -> bool {
    let wanted = query.trim().to_lowercase();
    let found = [&entry.id, &entry.title, &entry.summary, &entry.category]
        .iter()
        .any(|field| field.to_lowercase().contains(&wanted));
    found && category.is_none_or(|category| entry.category == category)
}

fn shelf(host: &Host, loaded: LoadedCatalog, query: &str, category: Option<&str>) -> Result<Shelf> {
    let registry = host.registry()?;
    let identity = HostIdentity::current();
    let catalog = loaded.catalog;
    let extensions = catalog
        .extensions
        .iter()
        .filter(|entry| matches(entry, query, category))
        .map(|entry| {
            let installed = registry.get(&entry.id);
            let quarantined = installed
                .map(|installed| installed.quarantined.clone())
                .unwrap_or_default();
            Offer {
                entry: entry.clone(),
                installed: installed.map(|installed| installed.version.to_string()),
                available: resolve(&catalog, &entry.id, &identity, &quarantined)
                    .map(|(version, _)| version.version.to_string()),
            }
        })
        .collect();
    Ok(Shelf {
        revision: catalog.revision,
        generated_at: catalog.generated_at,
        stale: loaded.stale,
        provenance: match loaded.provenance {
            Provenance::Seed => "seed",
            Provenance::Cache => "cache",
            Provenance::Network => "network",
        },
        categories: catalog.categories,
        profiles: catalog.profiles,
        extensions,
    })
}

pub(crate) fn search(host: &Host, query: &str, category: Option<&str>) -> Result<Shelf> {
    let loaded = CatalogStore::new(host.layout().clone()).load(Utc::now())?;
    shelf(host, loaded, query, category)
}

pub(crate) fn refresh(host: &Host) -> Result<Shelf> {
    let store = CatalogStore::new(host.layout().clone());
    let loaded = store.refresh(&HttpTransport::new()?, Utc::now())?;
    shelf(host, loaded, "", None)
}

pub(crate) fn info(host: &Host, id: &str) -> Result<Offer> {
    search(host, "", None)?
        .extensions
        .into_iter()
        .find(|offer| offer.entry.id == id)
        .with_context(|| format!("{id} is not in the catalog"))
}
