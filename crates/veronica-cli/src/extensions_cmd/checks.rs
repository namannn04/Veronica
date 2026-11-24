use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use serde::Serialize;
use veronica_core::{Capabilities, Capability, CapabilityState};
use veronica_host::archive::{self, FILE_INDEX};
use veronica_host::catalog::CatalogStore;
use veronica_host::host_api::HostServices;
use veronica_host::registry::{Origin, Registry};
use veronica_host::supervisor::verify_starts;
use veronica_host::update::AutoUpdate;
use veronica_protocol::catalog::{Catalog, FileIndex};
use veronica_protocol::ExitCode;

use super::{not_found, origin_label, Env};
use crate::format::{self, Failure};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Verified {
    id: String,
    version: String,
    origin: Origin,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    problem: Option<String>,
}

fn verify_tree(tree: &Path) -> Result<(), String> {
    let listing = fs::read(tree.join(FILE_INDEX))
        .map_err(|error| format!("cannot read {FILE_INDEX}: {error}"))?;
    let index: FileIndex = serde_json::from_slice(&listing)
        .map_err(|error| format!("{FILE_INDEX} is not valid: {error}"))?;
    archive::verify_tree(tree, &index).map_err(|error| error.to_string())
}

fn verify_listing(
    catalog: &Catalog,
    id: &str,
    origin: Origin,
    version: &str,
) -> Result<(), String> {
    if origin != Origin::Catalog {
        return Ok(());
    }
    let listed = catalog.extension(id).and_then(|listed| {
        listed
            .versions
            .iter()
            .find(|entry| entry.version.to_string() == version)
    });
    match listed {
        Some(entry) if entry.yanked => Err(format!("{version} was withdrawn from the catalog")),
        _ => Ok(()),
    }
}

pub(super) fn verify(env: &Env, only: Option<&str>) -> Result<()> {
    let registry = env.registry()?;
    if let Some(id) = only.filter(|id| registry.get(id).is_none()) {
        if veronica_core::extensions::entry(id).is_some() {
            return Err(Failure::new(
                ExitCode::Usage,
                format!("{id} is built into Veronica and has no package to verify"),
            )
            .into());
        }
        return Err(not_found(id));
    }
    let catalog = env.catalog()?.catalog;
    let results: Vec<Verified> = registry
        .extensions
        .iter()
        .filter(|(id, _)| only.is_none_or(|only| only == id.as_str()))
        .map(|(id, entry)| {
            let version = entry.version.to_string();
            let outcome = fs::canonicalize(env.layout.current_link(id))
                .map_err(|error| format!("the current version is missing: {error}"))
                .and_then(|tree| verify_tree(&tree))
                .and_then(|()| verify_listing(&catalog, id, entry.origin, &version));
            Verified {
                id: id.clone(),
                version,
                origin: entry.origin,
                ok: outcome.is_ok(),
                problem: outcome.err(),
            }
        })
        .collect();
    env.output.emit(&results, || {
        if results.is_empty() {
            return "no installed extensions to verify".to_string();
        }
        let rows: Vec<Vec<String>> = results
            .iter()
            .map(|result| {
                vec![
                    result.id.clone(),
                    result.version.clone(),
                    origin_label(result.origin).to_string(),
                    result.problem.clone().unwrap_or_else(|| "ok".to_string()),
                ]
            })
            .collect();
        format::table(&["id", "version", "origin", "result"], &rows)
    })?;
    let failed = results.iter().filter(|result| !result.ok).count();
    if failed > 0 {
        return Err(Failure::new(
            ExitCode::VerificationFailed,
            format!("{failed} extension(s) failed verification"),
        )
        .into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Check {
    name: String,
    level: Level,
    detail: String,
}

impl Check {
    fn new(name: impl Into<String>, level: Level, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            level,
            detail: detail.into(),
        }
    }
}

fn exec_permission(packages: &Path) -> Check {
    const NAME: &str = "exec permission";
    let probe = packages.join(format!(".doctor-{}", std::process::id()));
    let outcome = fs::create_dir_all(packages)
        .and_then(|()| fs::write(&probe, b"#!/bin/sh\nexit 0\n"))
        .and_then(|()| fs::set_permissions(&probe, fs::Permissions::from_mode(0o755)))
        .and_then(|()| std::process::Command::new(&probe).status());
    let _ = fs::remove_file(&probe);
    match outcome {
        Ok(status) if status.success() => {
            Check::new(NAME, Level::Ok, packages.display().to_string())
        }
        Err(error) if error.kind() == ErrorKind::PermissionDenied => Check::new(
            NAME,
            Level::Fail,
            format!(
                "{} does not allow running programs; it is probably mounted noexec",
                packages.display()
            ),
        ),
        Ok(status) => Check::new(
            NAME,
            Level::Fail,
            format!("a test program exited with {status}"),
        ),
        Err(error) => Check::new(NAME, Level::Fail, error.to_string()),
    }
}

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| {
            fs::metadata(dir.join(name)).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
    })
}

async fn tool_checks(id: &str, tools: &[String]) -> Vec<Check> {
    if tools.is_empty() {
        return Vec::new();
    }
    let reports = veronica_system::tools::catalogue().await;
    tools
        .iter()
        .map(|tool| {
            let found = reports
                .iter()
                .find(|report| report.id == tool)
                .map_or_else(|| on_path(tool), |report| report.readiness.is_installed());
            let level = if found { Level::Ok } else { Level::Warn };
            let detail = if found { "installed" } else { "not installed" };
            Check::new(format!("{id}: tool {tool}"), level, detail)
        })
        .collect()
}

fn capability_checks(id: &str, required: &[String], capabilities: &Capabilities) -> Vec<Check> {
    required
        .iter()
        .map(|name| {
            let name_check = format!("{id}: capability {name}");
            let Ok(capability) =
                serde_json::from_value::<Capability>(serde_json::Value::String(name.clone()))
            else {
                return Check::new(name_check, Level::Warn, "this host does not know it");
            };
            match capabilities.state(capability) {
                CapabilityState::Available => Check::new(name_check, Level::Ok, "available"),
                state => Check::new(name_check, Level::Warn, crate::state_label(state)),
            }
        })
        .collect()
}

async fn extension_checks(env: &Env, registry: &Registry, checks: &mut Vec<Check>) {
    let services = Arc::new(HostServices::new(
        env.layout.clone(),
        env.directories.settings_file(),
    ));
    let capabilities = super::capabilities().await;
    for (id, entry) in &registry.extensions {
        let manifest = match env.manifest(id) {
            Ok(manifest) => manifest,
            Err(error) => {
                checks.push(Check::new(id.clone(), Level::Fail, format!("{error:#}")));
                continue;
            }
        };
        if !entry.enabled {
            checks.push(Check::new(id.clone(), Level::Ok, "turned off, not started"));
        } else if manifest.backend.is_some() {
            checks.push(match verify_starts(&services, id).await {
                Ok(()) => Check::new(
                    format!("{id}: backend"),
                    Level::Ok,
                    "answered the handshake",
                ),
                Err(error) => Check::new(format!("{id}: backend"), Level::Fail, error.to_string()),
            });
        }
        checks.extend(tool_checks(id, &manifest.requires.tools).await);
        checks.extend(capability_checks(
            id,
            &manifest.requires.capabilities,
            &capabilities,
        ));
    }
}

fn catalog_check(env: &Env) -> Result<Check> {
    const NAME: &str = "catalog";
    let loaded = env.catalog()?;
    let state = CatalogStore::new(env.layout.clone()).state()?;
    let interval = AutoUpdate::from_settings(&env.settings()?).interval();
    let now = Utc::now();
    Ok(match state.last_success_at() {
        _ if loaded.stale => Check::new(
            NAME,
            Level::Warn,
            format!(
                "the catalog expired at {}; refresh it with `vr extensions update --yes`",
                loaded.catalog.expires_at
            ),
        ),
        None => Check::new(
            NAME,
            Level::Warn,
            "never refreshed; the built-in copy is in use until the next install or update",
        ),
        Some(last) if now - last > interval * 2 => Check::new(
            NAME,
            Level::Warn,
            format!("last refreshed {}", last.to_rfc3339()),
        ),
        Some(last) => Check::new(
            NAME,
            Level::Ok,
            format!(
                "revision {}, refreshed {}",
                loaded.catalog.revision,
                last.to_rfc3339()
            ),
        ),
    })
}

fn leftovers(env: &Env) -> Check {
    const NAME: &str = "leftovers";
    let count = |dir: &Path| fs::read_dir(dir).map_or(0, Iterator::count);
    let staged = count(&env.layout.staging_dir());
    let partial = count(&env.layout.downloads_dir());
    if staged + partial == 0 {
        Check::new(NAME, Level::Ok, "no interrupted installs")
    } else {
        Check::new(
            NAME,
            Level::Warn,
            format!(
                "{staged} staged and {partial} partly downloaded item(s) from an interrupted install; the next install clears them"
            ),
        )
    }
}

pub(super) async fn doctor(env: &Env) -> Result<()> {
    let registry = env.registry()?;
    let mut checks = vec![exec_permission(&env.layout.packages_dir())];
    extension_checks(env, &registry, &mut checks).await;
    checks.push(catalog_check(env)?);
    checks.push(leftovers(env));
    env.output.emit(&checks, || {
        let rows: Vec<Vec<String>> = checks
            .iter()
            .map(|check| {
                let level = match check.level {
                    Level::Ok => "ok",
                    Level::Warn => "warn",
                    Level::Fail => "FAIL",
                };
                vec![level.to_string(), check.name.clone(), check.detail.clone()]
            })
            .collect();
        format::table(&["result", "check", "detail"], &rows)
    })?;
    let failed = checks
        .iter()
        .filter(|check| check.level == Level::Fail)
        .count();
    if failed > 0 {
        return Err(Failure::new(ExitCode::Failed, format!("{failed} check(s) failed")).into());
    }
    Ok(())
}
