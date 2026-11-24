use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use chrono::Utc;
use serde::Serialize;
use veronica_host::bundled::{install_preferred, Bundled};
use veronica_host::catalog::CatalogStore;
use veronica_host::fetch::HttpTransport;
use veronica_host::host_api::HostServices;
use veronica_host::registry::{InstalledExtension, Origin, Registry};
use veronica_host::update::{plan, resolve, AutoUpdate, Decision, Skip, Trigger};
use veronica_host::{HostIdentity, Installer};
use veronica_protocol::catalog::Catalog;
use veronica_protocol::manifest::{Permissions, Profile, Requirements};
use veronica_protocol::{ExitCode, Manifest};

use super::{
    classify, not_found, origin_label, provenance_label, AutoUpdateReport, Env, ProfileArg,
};
use crate::format::{self, Failure};

pub(super) fn installer(env: &Env, health_check: bool) -> Installer {
    let installer = Installer::new(env.layout.clone(), HostIdentity::current());
    if !health_check {
        return installer;
    }
    let services = HostServices::new(env.layout.clone(), env.directories.settings_file());
    installer.with_health_check(Arc::new(services))
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .context("the work stopped unexpectedly")?
}

async fn catalog(env: &Env) -> Result<Catalog> {
    if !env.yes {
        return Ok(env.catalog()?.catalog);
    }
    let store = CatalogStore::new(env.layout.clone());
    let refreshed = blocking(move || store.refresh(&HttpTransport::new()?, Utc::now())).await;
    match refreshed {
        Ok(loaded) => Ok(loaded.catalog),
        Err(error) => {
            let loaded = env.catalog()?;
            eprintln!(
                "vr: cannot refresh the extension catalog, using the {} copy: {error:#}",
                provenance_label(loaded.provenance)
            );
            Ok(loaded.catalog)
        }
    }
}

fn bundled() -> Option<Bundled> {
    Bundled::current().unwrap_or_else(|error| {
        eprintln!("vr: ignoring the bundled extensions: {error:#}");
        None
    })
}

async fn install_bundled(env: &Env, bundled: Bundled, id: &str) -> Result<()> {
    let registry = env.registry()?;
    let package = bundled
        .get(id)
        .context("bundled packages come from the index")?;
    if let Some(entry) = registry
        .get(id)
        .filter(|entry| entry.origin != Origin::Builtin && entry.version >= package.version)
    {
        return report_installed(env, id, entry, false);
    }
    let preview = InstallPreview {
        action: "install",
        id: id.to_string(),
        title: id.to_string(),
        version: package.version.to_string(),
        origin: Origin::Bundled,
        source: Some(bundled.dir().join(package.file_name())),
        download_bytes: None,
        replaces: registry.get(id).map(|entry| entry.version.to_string()),
        permissions: Permissions::default(),
    };
    env.confirm(&preview, || preview.render())?;
    let installer = installer(env, true);
    let owned = id.to_string();
    let entry = blocking(move || bundled.install(&installer, &owned)).await?;
    report_installed(env, id, &entry, true)
}

fn built_in(id: &str, advice: &str) -> Option<anyhow::Error> {
    veronica_core::extensions::entry(id).map(|_| {
        Failure::new(
            ExitCode::Usage,
            format!("{id} is built into Veronica; {advice}"),
        )
        .into()
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallPreview {
    action: &'static str,
    id: String,
    title: String,
    version: String,
    origin: Origin,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    download_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    replaces: Option<String>,
    permissions: Permissions,
}

impl InstallPreview {
    fn render(&self) -> String {
        let mut text = format!(
            "would install {} {} ({})",
            self.title, self.version, self.id
        );
        if let Some(replaces) = &self.replaces {
            let _ = write!(text, ", replacing {replaces}");
        }
        match (&self.source, self.origin) {
            (Some(source), Origin::Bundled) => {
                let _ = write!(text, "\nfrom the copy bundled at {}", source.display());
            }
            (Some(source), _) => {
                let _ = write!(text, "\nfrom {} as a development install", source.display());
            }
            (None, _) => {}
        }
        text
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Installed {
    id: String,
    version: String,
    origin: Origin,
    changed: bool,
}

fn report_installed(env: &Env, id: &str, entry: &InstalledExtension, changed: bool) -> Result<()> {
    let report = Installed {
        id: id.to_string(),
        version: entry.version.to_string(),
        origin: entry.origin,
        changed,
    };
    env.output.emit(&report, || {
        if changed {
            format!("installed {id} {}", report.version)
        } else {
            format!("{id} {} is already installed", report.version)
        }
    })
}

pub(super) async fn install(env: &Env, target: &str) -> Result<()> {
    if let Some(source) = super::source_path(target) {
        return sideload(env, source).await;
    }
    let id = target;
    let registry = env.registry()?;
    let catalog = catalog(env).await?;
    let Some(listed) = catalog.extension(id) else {
        if let Some(bundled) = bundled().filter(|bundled| bundled.get(id).is_some()) {
            return install_bundled(env, bundled, id).await;
        }
        return Err(
            built_in(id, &format!("turn it on with `vr extensions enable {id}`"))
                .unwrap_or_else(|| not_found(id)),
        );
    };
    let current = registry.get(id);
    let quarantined = current
        .map(|entry| entry.quarantined.clone())
        .unwrap_or_default();
    let (version, artifact) = resolve(&catalog, id, &HostIdentity::current(), &quarantined)
        .ok_or_else(|| {
            Failure::new(
                ExitCode::Unavailable,
                format!("no published version of {id} fits this computer"),
            )
        })?;
    if let Some(entry) =
        current.filter(|entry| entry.origin == Origin::Catalog && entry.version >= version.version)
    {
        return report_installed(env, id, entry, false);
    }
    let preview = InstallPreview {
        action: "install",
        id: id.to_string(),
        title: listed.title.clone(),
        version: version.version.to_string(),
        origin: Origin::Catalog,
        source: None,
        download_bytes: Some(artifact.size),
        replaces: current.map(|entry| entry.version.to_string()),
        permissions: listed.permissions.clone(),
    };
    env.confirm(&preview, || preview.render())?;
    let installer = installer(env, true);
    let owned = id.to_string();
    let entry = blocking(move || {
        let bundled = bundled();
        install_preferred(
            &installer,
            &catalog,
            bundled.as_ref(),
            &HttpTransport::new()?,
            &owned,
        )
    })
    .await?;
    report_installed(env, id, &entry, true)
}

async fn sideload(env: &Env, source: PathBuf) -> Result<()> {
    let source = std::fs::canonicalize(&source).map_err(|error| {
        Failure::new(
            ExitCode::NotFound,
            format!("cannot open {}: {error}", source.display()),
        )
    })?;
    let path = source.join("manifest.json");
    let raw = std::fs::read_to_string(&path).map_err(|error| {
        Failure::new(
            ExitCode::NotFound,
            format!("cannot read {}: {error}", path.display()),
        )
    })?;
    let manifest = Manifest::parse(&raw).map_err(|error| {
        Failure::new(
            ExitCode::VerificationFailed,
            format!("{} is invalid: {error}", path.display()),
        )
    })?;
    let id = manifest.id.clone();
    let preview = InstallPreview {
        action: "install",
        id: id.clone(),
        title: manifest.title.clone(),
        version: manifest.version.to_string(),
        origin: Origin::Sideload,
        source: Some(source.clone()),
        download_bytes: None,
        replaces: env
            .registry()?
            .get(&id)
            .map(|entry| entry.version.to_string()),
        permissions: manifest.permissions,
    };
    env.confirm(&preview, || preview.render())?;
    let installer = installer(env, true);
    let entry = blocking(move || installer.install_sideload(&source)).await?;
    eprintln!("vr: {id} is a development install and is never updated automatically");
    report_installed(env, &id, &entry, true)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Planned {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    to: String,
}

fn skipped(skip: Skip) -> (ExitCode, &'static str) {
    match skip {
        Skip::NotFromCatalog => (
            ExitCode::Failed,
            "it was not installed from the catalog; install it from the catalog first",
        ),
        Skip::AutoUpdateOff => (ExitCode::Failed, "automatic updates are off for it"),
        Skip::NotInCatalog => (ExitCode::NotFound, "it is no longer in the catalog"),
        Skip::NoCompatibleVersion => (
            ExitCode::Unavailable,
            "no published version fits this computer",
        ),
        Skip::UpToDate => (ExitCode::Success, "it is already up to date"),
    }
}

fn pending(registry: &Registry, catalog: &Catalog, only: Option<&str>) -> Result<Vec<Planned>> {
    let decisions = plan(
        registry,
        catalog,
        &HostIdentity::current(),
        &AutoUpdate::default(),
        Trigger::Manual,
    );
    let planned = |id: &String, decision: &Decision| match decision {
        Decision::Update { from, to } => Some(Planned {
            id: id.clone(),
            from: Some(from.to_string()),
            to: to.to_string(),
        }),
        Decision::Skip(_) => None,
    };
    let Some(only) = only else {
        return Ok(decisions
            .iter()
            .filter_map(|(id, decision)| planned(id, decision))
            .collect());
    };
    match decisions.get(only) {
        Some(Decision::Skip(skip)) if *skip != Skip::UpToDate => {
            let (code, reason) = skipped(*skip);
            Err(Failure::new(code, format!("{only} cannot be updated: {reason}")).into())
        }
        Some(decision) => Ok(planned(&only.to_string(), decision).into_iter().collect()),
        None => Err(not_found(only)),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Attempt {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn install_each(
    env: &Env,
    catalog: Catalog,
    ids: Vec<String>,
) -> Result<Vec<(Attempt, Option<ExitCode>)>> {
    let installer = installer(env, true);
    let outcomes = blocking(move || {
        let transport = HttpTransport::new()?;
        let bundled = bundled();
        Ok(ids
            .into_iter()
            .map(|id| {
                let outcome =
                    install_preferred(&installer, &catalog, bundled.as_ref(), &transport, &id);
                (id, outcome)
            })
            .collect::<Vec<_>>())
    })
    .await?;
    Ok(outcomes
        .into_iter()
        .map(|(id, outcome)| match outcome {
            Ok(entry) => (
                Attempt {
                    id,
                    version: Some(entry.version.to_string()),
                    error: None,
                },
                None,
            ),
            Err(error) => {
                let failure = classify(error);
                let code = failure
                    .downcast_ref::<Failure>()
                    .map(|failure| failure.code);
                (
                    Attempt {
                        id,
                        version: None,
                        error: Some(format!("{failure:#}")),
                    },
                    code,
                )
            }
        })
        .collect())
}

fn finish(env: &Env, key: &str, attempts: Vec<(Attempt, Option<ExitCode>)>) -> Result<()> {
    let first_failure = attempts.iter().find_map(|(_, code)| *code);
    let failed = attempts
        .iter()
        .filter(|(attempt, _)| attempt.error.is_some())
        .count();
    let attempts: Vec<Attempt> = attempts.into_iter().map(|(attempt, _)| attempt).collect();
    let report = serde_json::json!({ key: attempts });
    env.output.emit(&report, || {
        attempts
            .iter()
            .map(|attempt| match (&attempt.version, &attempt.error) {
                (_, Some(error)) => format!("{} failed: {error}", attempt.id),
                (Some(version), None) => format!("{key} {} {version}", attempt.id),
                (None, None) => attempt.id.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    match first_failure {
        Some(code) => Err(Failure::new(
            code,
            format!("{failed} of {} could not be installed", attempts.len()),
        )
        .into()),
        None => Ok(()),
    }
}

pub(super) async fn update(env: &Env, only: Option<&str>) -> Result<()> {
    let registry = env.registry()?;
    if let Some(id) = only.filter(|id| registry.get(id).is_none()) {
        return Err(
            built_in(id, "it updates together with the app").unwrap_or_else(|| not_found(id))
        );
    }
    let catalog = catalog(env).await?;
    let updates = pending(&registry, &catalog, only)?;
    if updates.is_empty() {
        let report = serde_json::json!({ "updated": [] });
        return env
            .output
            .emit(&report, || "every extension is up to date".to_string());
    }
    let preview = serde_json::json!({
        "action": "update",
        "updates": updates,
        "catalogRevision": catalog.revision,
    });
    env.confirm(&preview, || {
        updates
            .iter()
            .map(|update| {
                format!(
                    "would update {} {} to {}",
                    update.id,
                    update.from.as_deref().unwrap_or("?"),
                    update.to
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let ids = updates.into_iter().map(|update| update.id).collect();
    let attempts = install_each(env, catalog, ids).await?;
    finish(env, "updated", attempts)
}

pub(super) fn remove(env: &Env, id: &str, purge: bool) -> Result<()> {
    let registry = env.registry()?;
    let Some(entry) = registry.get(id) else {
        return Err(built_in(
            id,
            &format!("turn it off with `vr extensions disable {id}`"),
        )
        .unwrap_or_else(|| not_found(id)));
    };
    let data = env.layout.data_dir(id);
    let preview = serde_json::json!({
        "action": "remove",
        "id": id,
        "version": entry.version.to_string(),
        "origin": entry.origin,
        "purge": purge,
        "data": data,
    });
    env.confirm(&preview, || {
        let fate = if purge { "deleting" } else { "keeping" };
        format!(
            "would remove {id} {}, {fate} its data in {}",
            entry.version,
            data.display()
        )
    })?;
    installer(env, false).remove(id, purge)?;
    let report = serde_json::json!({ "removed": id, "purged": purge });
    env.output.emit(&report, || format!("removed {id}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Unavailable {
    id: String,
    reason: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupPlan {
    profile: Profile,
    title: String,
    install: Vec<Planned>,
    present: Vec<String>,
    unavailable: Vec<Unavailable>,
}

impl SetupPlan {
    fn render(&self) -> String {
        let mut lines = vec![format!("{} profile", self.title)];
        lines.extend(
            self.install
                .iter()
                .map(|item| format!("would install {} {}", item.id, item.to)),
        );
        lines.extend(
            self.present
                .iter()
                .map(|id| format!("{id} is already here")),
        );
        if self.install.is_empty() {
            lines.push("nothing to install".to_string());
        }
        lines.extend(
            self.unavailable
                .iter()
                .map(|item| format!("{} is unavailable: {}", item.id, item.reason)),
        );
        lines.join("\n")
    }
}

fn profile_plan(
    registry: &Registry,
    catalog: &Catalog,
    bundled: Option<&Bundled>,
    profile: Profile,
) -> Result<SetupPlan> {
    let listed = catalog
        .profiles
        .iter()
        .find(|candidate| candidate.id == profile);
    let packaged: Vec<_> = bundled
        .map(|bundled| bundled.index().extensions.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|package| package.profiles.contains(&profile))
        .collect();
    if listed.is_none() && packaged.is_empty() {
        return Err(Failure::new(ExitCode::NotFound, "the catalog has no such profile").into());
    }
    let mut members: Vec<String> = listed
        .map(|listed| listed.extensions.clone())
        .unwrap_or_default();
    for package in &packaged {
        if !members.contains(&package.id) {
            members.push(package.id.clone());
        }
    }
    let host = HostIdentity::current();
    let mut plan = SetupPlan {
        profile,
        title: listed.map_or_else(
            || match profile {
                Profile::Simple => "Simple".to_string(),
                Profile::Developer => "Developer".to_string(),
            },
            |listed| listed.title.clone(),
        ),
        install: Vec::new(),
        present: Vec::new(),
        unavailable: Vec::new(),
    };
    for id in members {
        let published =
            resolve(catalog, &id, &host, &BTreeSet::new()).map(|(version, _)| version.version);
        let shipped = packaged
            .iter()
            .find(|package| package.id == id)
            .map(|package| package.version);
        if registry.get(&id).is_some() {
            plan.present.push(id);
        } else if let Some(version) = published.max(shipped) {
            plan.install.push(Planned {
                id,
                from: None,
                to: version.to_string(),
            });
        } else if catalog.extension(&id).is_none() {
            plan.unavailable.push(Unavailable {
                id,
                reason: "not in the catalog",
            });
        } else {
            plan.unavailable.push(Unavailable {
                id,
                reason: "no published version fits this computer",
            });
        }
    }
    Ok(plan)
}

pub(super) async fn setup(env: &Env, profile: ProfileArg, dry_run: bool) -> Result<()> {
    let profile = match profile {
        ProfileArg::Simple => Profile::Simple,
        ProfileArg::Developer => Profile::Developer,
    };
    let catalog = if dry_run {
        env.catalog()?.catalog
    } else {
        catalog(env).await?
    };
    let plan = profile_plan(&env.registry()?, &catalog, bundled().as_ref(), profile)?;
    if dry_run || plan.install.is_empty() {
        return env.output.emit(&plan, || plan.render());
    }
    env.confirm(&plan, || plan.render())?;
    let ids = plan.install.into_iter().map(|item| item.id).collect();
    let attempts = install_each(env, catalog, ids).await?;
    finish(env, "installed", attempts)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusRow {
    id: String,
    version: String,
    origin: Origin,
    enabled: bool,
    auto_update: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    update: Option<String>,
}

pub(super) fn status(env: &Env) -> Result<()> {
    let registry = env.registry()?;
    let loaded = env.catalog()?;
    let state = CatalogStore::new(env.layout.clone()).state()?;
    let settings = AutoUpdate::from_settings(&env.settings()?);
    let decisions = plan(
        &registry,
        &loaded.catalog,
        &HostIdentity::current(),
        &settings,
        Trigger::Manual,
    );
    let rows: Vec<StatusRow> = registry
        .extensions
        .iter()
        .map(|(id, entry)| StatusRow {
            id: id.clone(),
            version: entry.version.to_string(),
            origin: entry.origin,
            enabled: entry.enabled,
            auto_update: entry.origin == Origin::Catalog && settings.allows(id),
            update: match decisions.get(id) {
                Some(Decision::Update { to, .. }) => Some(to.to_string()),
                _ => None,
            },
        })
        .collect();
    let report = serde_json::json!({
        "catalog": {
            "revision": loaded.catalog.revision,
            "source": provenance_label(loaded.provenance),
            "stale": loaded.stale,
            "expiresAt": loaded.catalog.expires_at,
            "lastChecked": state.last_success,
        },
        "autoUpdate": AutoUpdateReport::from(&settings),
        "extensions": rows,
    });
    env.output.emit(&report, || {
        let mut text = format!(
            "catalog            revision {} from the {} copy{}\nlast checked       {}\nautomatic updates  {}, every {}h",
            loaded.catalog.revision,
            provenance_label(loaded.provenance),
            if loaded.stale { ", expired" } else { "" },
            state.last_success.as_deref().unwrap_or("never"),
            if settings.enabled { "on" } else { "off" },
            settings.interval_hours,
        );
        let table: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                vec![
                    row.id.clone(),
                    row.version.clone(),
                    origin_label(row.origin).to_string(),
                    if row.enabled { "on" } else { "off" }.to_string(),
                    row.update.clone().unwrap_or_else(|| "-".to_string()),
                ]
            })
            .collect();
        if !table.is_empty() {
            let _ = write!(
                text,
                "\n\n{}",
                format::table(&["id", "version", "origin", "enabled", "update"], &table)
            );
        }
        text
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchRow {
    id: String,
    title: String,
    summary: String,
    category: String,
    labs: bool,
    latest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    installed: Option<String>,
}

pub(super) fn search(env: &Env, query: &str) -> Result<()> {
    let needle = query.to_lowercase();
    let registry = env.registry()?;
    let loaded = env.catalog()?;
    let rows: Vec<SearchRow> = loaded
        .catalog
        .extensions
        .iter()
        .filter(|listed| {
            [&listed.id, &listed.title, &listed.summary, &listed.category]
                .iter()
                .any(|field| field.to_lowercase().contains(&needle))
        })
        .map(|listed| SearchRow {
            id: listed.id.clone(),
            title: listed.title.clone(),
            summary: listed.summary.clone(),
            category: listed.category.clone(),
            labs: listed.labs,
            latest: listed.latest.to_string(),
            installed: registry
                .get(&listed.id)
                .map(|entry| entry.version.to_string()),
        })
        .collect();
    env.output.emit(&rows, || {
        if rows.is_empty() {
            return format!("nothing in the catalog matches '{query}'");
        }
        let table: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                vec![
                    row.id.clone(),
                    row.title.clone(),
                    row.category.clone(),
                    row.latest.clone(),
                    row.installed.clone().unwrap_or_else(|| "-".to_string()),
                ]
            })
            .collect();
        format::table(&["id", "title", "category", "latest", "installed"], &table)
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledInfo {
    version: String,
    origin: Origin,
    enabled: bool,
    installed_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageInfo {
    id: String,
    title: String,
    summary: String,
    category: String,
    labs: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    installed: Option<InstalledInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest: Option<String>,
    permissions: Permissions,
    requires: Requirements,
    commands: Vec<String>,
}

pub(super) fn info(env: &Env, registry: &Registry, id: &str) -> Result<()> {
    let installed = registry.get(id);
    let manifest = installed.map(|_| env.manifest(id)).transpose()?;
    let loaded = env.catalog()?;
    let listed = loaded.catalog.extension(id);
    let report = match (&manifest, listed) {
        (Some(manifest), _) => PackageInfo {
            id: id.to_string(),
            title: manifest.title.clone(),
            summary: manifest.summary.clone(),
            category: manifest.category.clone(),
            labs: manifest.labs,
            installed: None,
            latest: None,
            permissions: manifest.permissions.clone(),
            requires: manifest.requires.clone(),
            commands: manifest
                .leaf_routes()
                .into_iter()
                .map(|(route, _)| route.join(" "))
                .collect(),
        },
        (None, Some(listed)) => PackageInfo {
            id: id.to_string(),
            title: listed.title.clone(),
            summary: listed.summary.clone(),
            category: listed.category.clone(),
            labs: listed.labs,
            installed: None,
            latest: None,
            permissions: listed.permissions.clone(),
            requires: listed.requires.clone(),
            commands: Vec::new(),
        },
        (None, None) => return Err(not_found(id)),
    };
    let report = PackageInfo {
        installed: installed.map(|entry| InstalledInfo {
            version: entry.version.to_string(),
            origin: entry.origin,
            enabled: entry.enabled,
            installed_at: entry.installed_at.clone(),
            previous: entry.previous.map(|version| version.to_string()),
        }),
        latest: listed.map(|listed| listed.latest.to_string()),
        ..report
    };
    env.output.emit(&report, || render_info(&report))
}

fn render_info(report: &PackageInfo) -> String {
    let mut text = format!("{} ({})\n{}\n", report.title, report.id, report.summary);
    let mut line = |label: &str, value: &str| {
        let _ = write!(text, "\n{label:<13} {value}");
    };
    line("category", &report.category);
    match &report.installed {
        Some(installed) => {
            line(
                "installed",
                &format!(
                    "{} from {}",
                    installed.version,
                    origin_label(installed.origin)
                ),
            );
            line("enabled", if installed.enabled { "on" } else { "off" });
        }
        None => line("installed", "no"),
    }
    if let Some(latest) = &report.latest {
        line("latest", latest);
    }
    if !report.requires.tools.is_empty() {
        line("needs tools", &report.requires.tools.join(", "));
    }
    if !report.requires.capabilities.is_empty() {
        line("needs", &report.requires.capabilities.join(", "));
    }
    if !report.commands.is_empty() {
        let _ = write!(
            text,
            "\n\nCommands\n  vr {}",
            report.commands.join("\n  vr ")
        );
    }
    text
}
