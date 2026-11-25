use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use veronica_core::{Capabilities, Diagnostics};

use super::maintenance::install_kind;
use super::system::{sync_lid_awake, sync_prevent_sleep};
use super::{fail, CommandResult};
use crate::state::AppState;

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn diagnostics(state: State<'_, AppState>) -> Diagnostics {
    let session = state.session.lock().expect("session lock").clone();
    let settings = state.settings_snapshot();
    Diagnostics::collect(&state.directories, session, &settings)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn capabilities(state: State<'_, AppState>) -> Capabilities {
    let session = state.session.lock().expect("session lock").clone();
    Capabilities::resolve(&session)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn settings_all(state: State<'_, AppState>) -> CommandResult<serde_json::Value> {
    let settings = state.settings_snapshot();
    serde_json::to_value(settings.as_map()).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchAtLoginStatus {
    pub enabled: bool,
    pub entry_path: String,
}

pub(super) const SNAP_LAUNCHER: &str = "/snap/bin/veronica";

fn autostart_executable() -> CommandResult<std::path::PathBuf> {
    if install_kind() == "snap" {
        return Ok(SNAP_LAUNCHER.into());
    }
    let current = std::env::current_exe()
        .map_err(|error| format!("cannot locate the running Veronica executable: {error}"))?;
    Ok(veronica_core::autostart::preferred_executable(
        std::env::var_os("APPIMAGE").as_deref(),
        &current,
    ))
}

fn launch_at_login_status_for(state: &AppState) -> CommandResult<LaunchAtLoginStatus> {
    let executable = autostart_executable()?;
    let path = state.directories.autostart_file();
    Ok(LaunchAtLoginStatus {
        enabled: veronica_core::autostart::is_enabled(&path, &executable),
        entry_path: path.display().to_string(),
    })
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn launch_at_login_status(
    state: State<'_, AppState>,
) -> CommandResult<LaunchAtLoginStatus> {
    launch_at_login_status_for(&state)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn launch_at_login_set(
    state: State<'_, AppState>,
    enabled: bool,
) -> CommandResult<LaunchAtLoginStatus> {
    let executable = autostart_executable()?;
    let path = state.directories.autostart_file();
    veronica_core::autostart::set_enabled(&path, &executable, enabled).map_err(fail)?;
    launch_at_login_status_for(&state)
}

const SHELL_EXTENSION_UUID: &str = "veronica@namannn04.github.io";

const FLATPAK_SHELL_EXTENSION: &str = "/app/share/veronica/gnome-shell-extension";

const FLATPAK_CLI_SHIM: &str =
    "#!/bin/sh\nexec flatpak run --command=vr io.github.namannn04.Veronica \"$@\"\n";

const SNAP_CLI_SHIM: &str = "#!/bin/sh\nexec /snap/bin/veronica.vr \"$@\"\n";

fn bundled_shell_extension() -> Option<(std::path::PathBuf, &'static str)> {
    use veronica_core::packaging::Packaging;

    match Packaging::detect() {
        Packaging::Flatpak => Some((FLATPAK_SHELL_EXTENSION.into(), FLATPAK_CLI_SHIM)),
        Packaging::Snap => std::env::var_os("SNAP").map(|root| {
            let source = std::path::Path::new(&root)
                .join("usr/share/gnome-shell/extensions")
                .join(SHELL_EXTENSION_UUID);
            (source, SNAP_CLI_SHIM)
        }),
        Packaging::Deb | Packaging::AppImage => None,
    }
}

pub(crate) fn install_bundled_shell_extension(
    directories: &veronica_core::AppDirectories,
) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::os::unix::fs::PermissionsExt;

    let Some((source, cli_shim)) = bundled_shell_extension() else {
        return Ok(());
    };
    if !source.is_dir() {
        return Ok(());
    }
    let target = directories
        .data
        .parent()
        .context("the data directory has no parent")?
        .join("gnome-shell")
        .join("extensions")
        .join(SHELL_EXTENSION_UUID);
    let first_install = !target.join("metadata.json").exists();
    std::fs::create_dir_all(&target)
        .with_context(|| format!("cannot create {}", target.display()))?;

    let write_if_changed = |path: &std::path::Path, contents: &[u8]| -> anyhow::Result<()> {
        if std::fs::read(path).ok().as_deref() != Some(contents) {
            std::fs::write(path, contents)
                .with_context(|| format!("cannot write {}", path.display()))?;
        }
        Ok(())
    };
    for entry in std::fs::read_dir(&source)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            write_if_changed(
                &target.join(entry.file_name()),
                &std::fs::read(entry.path())?,
            )?;
        }
    }
    let shim = target.join("vr");
    write_if_changed(&shim, cli_shim.as_bytes())?;
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755))?;
    if first_install {
        enable_at_next_login().map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

fn enabled_shell_extensions() -> Option<String> {
    let output = veronica_core::host::command("gsettings")
        .args(["get", "org.gnome.shell", "enabled-extensions"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn with_veronica_enabled(current: &str) -> Option<String> {
    let quoted = format!("'{SHELL_EXTENSION_UUID}'");
    if current.contains(&quoted) {
        return None;
    }
    let inner = current
        .trim_start_matches("@as")
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    Some(if inner.is_empty() {
        format!("[{quoted}]")
    } else {
        format!("[{inner}, {quoted}]")
    })
}

fn enable_at_next_login() -> Result<(), String> {
    let current = enabled_shell_extensions().ok_or("cannot read GNOME's enabled extensions")?;
    let Some(value) = with_veronica_enabled(&current) else {
        return Ok(());
    };
    let output = veronica_core::host::command("gsettings")
        .args(["set", "org.gnome.shell", "enabled-extensions", &value])
        .output()
        .map_err(|error| format!("cannot run gsettings: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ShellExtensionStatus {
    pub state: String,
    pub detail: String,
    pub can_enable: bool,
}

fn shell_extension_installed(state: &AppState) -> bool {
    let relative = std::path::Path::new("gnome-shell")
        .join("extensions")
        .join(SHELL_EXTENSION_UUID)
        .join("metadata.json");
    let user = state
        .directories
        .data
        .parent()
        .map(|root| root.join(&relative));
    user.is_some_and(|path| path.is_file())
        || std::path::Path::new("/usr/share").join(&relative).is_file()
        || std::path::Path::new("/usr/local/share")
            .join(relative)
            .is_file()
}

fn parsed_shell_extension_status(info: &str) -> ShellExtensionStatus {
    let field = |name: &str| {
        info.lines()
            .map(str::trim)
            .find_map(|line| line.strip_prefix(name).map(str::trim))
            .unwrap_or("")
    };
    let state = field("State:").to_ascii_uppercase();
    let enabled = field("Enabled:").eq_ignore_ascii_case("yes");
    if state == "ACTIVE" {
        ShellExtensionStatus {
            state: "active".into(),
            detail: "The Veronica notch and compositor features are running.".into(),
            can_enable: false,
        }
    } else if state == "ERROR" {
        ShellExtensionStatus {
            state: "error".into(),
            detail: "GNOME loaded the extension, but it reported an error. Its message is in the session log: journalctl --user -b | grep -i veronica".into(),
            can_enable: false,
        }
    } else if enabled {
        ShellExtensionStatus {
            state: "restartRequired".into(),
            detail: "The extension is enabled but not active. Log out and back in once.".into(),
            can_enable: false,
        }
    } else {
        ShellExtensionStatus {
            state: "disabled".into(),
            detail: "The extension is installed but disabled.".into(),
            can_enable: true,
        }
    }
}

fn shell_extension_status_for(state: &AppState) -> ShellExtensionStatus {
    if !state.session.lock().expect("session lock").is_gnome {
        return ShellExtensionStatus {
            state: "unsupported".into(),
            detail: "The notch needs a GNOME desktop session.".into(),
            can_enable: false,
        };
    }

    let installed = shell_extension_installed(state);
    let output = veronica_core::host::command("gnome-extensions")
        .args(["info", SHELL_EXTENSION_UUID])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            parsed_shell_extension_status(&String::from_utf8_lossy(&output.stdout))
        }
        Ok(_) if installed => {
            let listed = enabled_shell_extensions()
                .is_some_and(|current| with_veronica_enabled(&current).is_none());
            ShellExtensionStatus {
                state: "restartRequired".into(),
                detail: if listed {
                    "The extension was installed after this GNOME session started. Log out and back in once.".into()
                } else {
                    "Installed after this GNOME session started. Enable it now and it starts at your next login.".into()
                },
                can_enable: !listed,
            }
        }
        Ok(_) => ShellExtensionStatus {
            state: "notInstalled".into(),
            detail: "The shell extension is not installed. Install Veronica's Debian package for top-bar integration.".into(),
            can_enable: false,
        },
        Err(error) => ShellExtensionStatus {
            state: "error".into(),
            detail: format!("Cannot run gnome-extensions: {error}"),
            can_enable: false,
        },
    }
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn shell_extension_status(state: State<'_, AppState>) -> ShellExtensionStatus {
    shell_extension_status_for(&state)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn shell_extension_enable(
    state: State<'_, AppState>,
) -> CommandResult<ShellExtensionStatus> {
    let output = veronica_core::host::command("gnome-extensions")
        .args(["enable", SHELL_EXTENSION_UUID])
        .output()
        .map_err(|error| format!("cannot run gnome-extensions: {error}"))?;
    if !output.status.success() && shell_extension_installed(&state) {
        enable_at_next_login()?;
    } else if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            "GNOME refused to enable the Veronica extension".into()
        } else {
            detail
        });
    }
    Ok(shell_extension_status_for(&state))
}

#[tauri::command]
pub(crate) async fn shell_action(action: String) -> CommandResult<()> {
    let method = match action.as_str() {
        "cleanKeys" => "CleanKeys",
        "pickColor" => "PickColor",
        "showClipboard" => "ShowClipboard",
        _ => return Err(format!("unknown Shell action: {action}")),
    };
    call_shell_method(method).await
}

pub(crate) async fn call_shell_method(method: &str) -> CommandResult<()> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("cannot reach the GNOME session bus: {error}"))?;
    connection
        .call_method(
            Some("io.github.namannn04.Veronica.Shell"),
            "/io/github/namannn04/Veronica/Shell",
            Some("io.github.namannn04.Veronica.ShellActions"),
            method,
            &(),
        )
        .await
        .map_err(|error| {
            format!(
                "Veronica's GNOME extension is unavailable; enable it before using {method}: {error}"
            )
        })?;
    Ok(())
}

fn clear_awake_deadline(app: &AppHandle, until_key: &str) {
    let state = app.state::<AppState>();
    if state
        .settings_snapshot()
        .get(until_key)
        .is_some_and(|value| !value.is_null())
    {
        if let Err(error) = state.set_setting(until_key, serde_json::Value::Null) {
            tracing::warn!("cannot clear {until_key}: {error:#}");
        }
    }
}

#[tauri::command]
pub(crate) async fn settings_set(
    app: AppHandle,
    key: String,
    value: serde_json::Value,
) -> CommandResult<()> {
    if key == "lidAwakeEnabled" {
        let enabled = value
            .as_bool()
            .ok_or_else(|| "lidAwakeEnabled must be true or false".to_string())?;
        let was_enabled = app
            .state::<AppState>()
            .settings_snapshot()
            .bool_or("lidAwakeEnabled", false);

        if enabled {
            sync_lid_awake(&app, true).await?;
        }
        clear_awake_deadline(&app, veronica_core::awake::LID_AWAKE_UNTIL_KEY);
        if let Err(error) = app.state::<AppState>().set_setting(&key, value) {
            if enabled && !was_enabled {
                let _ = sync_lid_awake(&app, false).await;
            }
            return Err(fail(error));
        }
        if !enabled {
            sync_lid_awake(&app, false).await?;
        }
    } else if key == "preventSleep" {
        let enabled = value
            .as_bool()
            .ok_or_else(|| "preventSleep must be true or false".to_string())?;
        let was_enabled = app
            .state::<AppState>()
            .settings_snapshot()
            .bool_or("preventSleep", false);
        if enabled {
            sync_prevent_sleep(&app, true).await?;
        }
        clear_awake_deadline(&app, veronica_core::awake::KEEP_AWAKE_UNTIL_KEY);
        if let Err(error) = app.state::<AppState>().set_setting(&key, value) {
            if enabled && !was_enabled {
                let _ = sync_prevent_sleep(&app, false).await;
            }
            return Err(fail(error));
        }
        if !enabled {
            sync_prevent_sleep(&app, false).await?;
        }
    } else {
        app.state::<AppState>()
            .set_setting(&key, value)
            .map_err(fail)?;
    }
    crate::shortcuts::sync(&app, &app.state::<AppState>().settings_snapshot());
    let _ = app.emit("settings-updated", &key);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupSummary {
    pub path: String,
    pub app_version: String,
    pub created_at: String,
    pub files: usize,
    pub decoded_bytes: u64,
}

fn backup_summary(path: &std::path::Path, archive: &veronica_core::BackupArchive) -> BackupSummary {
    BackupSummary {
        path: path.display().to_string(),
        app_version: archive.manifest.app_version.clone(),
        created_at: archive.manifest.created_at.to_rfc3339(),
        files: archive.manifest.files.len(),
        decoded_bytes: archive.manifest.files.iter().map(|file| file.bytes).sum(),
    }
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn backup_export(
    state: State<'_, AppState>,
    path: Option<String>,
) -> CommandResult<BackupSummary> {
    let now = chrono::Utc::now();
    let destination = if let Some(value) = path.filter(|value| !value.trim().is_empty()) {
        std::path::PathBuf::from(value)
    } else {
        let home = veronica_core::paths::home_dir()
            .ok_or_else(|| "cannot resolve your home directory".to_string())?;
        home.join("Downloads").join(format!(
            "Veronica-backup-{}.veronica-backup",
            now.format("%Y-%m-%d-%H%M%S")
        ))
    };
    let archive =
        veronica_core::BackupArchive::collect(&state.directories, env!("CARGO_PKG_VERSION"), now)
            .map_err(fail)?;
    archive.save(&destination).map_err(fail)?;
    Ok(backup_summary(&destination, &archive))
}

#[tauri::command]
pub(crate) fn backup_inspect(path: String) -> CommandResult<BackupSummary> {
    let source = std::path::PathBuf::from(path);
    let archive = veronica_core::BackupArchive::load(&source).map_err(fail)?;
    Ok(backup_summary(&source, &archive))
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn backup_import(
    app: AppHandle,
    path: String,
    confirm: bool,
) -> CommandResult<veronica_core::ImportReport> {
    if !confirm {
        return Err("confirm the import before replacing matching files".into());
    }
    let source = std::path::PathBuf::from(path);
    let archive = veronica_core::BackupArchive::load(&source).map_err(fail)?;
    let state = app.state::<AppState>();
    let report = archive.restore(&state.directories).map_err(fail)?;
    state.reload_persistent_data().map_err(fail)?;
    crate::shortcuts::sync(&app, &state.settings_snapshot());
    let _ = app.emit("settings-updated", "backup-import");
    Ok(report)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn database_connections(
    state: State<'_, AppState>,
) -> CommandResult<Vec<veronica_database::connection::ConnectionDefinition>> {
    veronica_database::store::MetadataStore::open(state.directories.database_store())
        .and_then(|store| store.connections())
        .map_err(fail)
}

#[tauri::command]
pub(crate) async fn database_test(
    app: AppHandle,
    connection: String,
) -> CommandResult<veronica_database::product::ProductIdentity> {
    let (definition, secrets) = database_context(&app, &connection)?;
    let mut session = veronica_database::session::Session::open(&definition, &secrets)
        .await
        .map_err(fail)?;
    session.identify().await.map_err(fail)
}

#[tauri::command]
pub(crate) async fn database_browse(
    app: AppHandle,
    connection: String,
    path: Vec<String>,
) -> CommandResult<Vec<veronica_database::identify::ObjectIdentifier>> {
    let (definition, secrets) = database_context(&app, &connection)?;
    let mut session = veronica_database::session::Session::open(&definition, &secrets)
        .await
        .map_err(fail)?;
    let parent = (!path.is_empty()).then(|| {
        veronica_database::identify::ObjectIdentifier::for_path(definition.product_hint, path)
    });
    session.objects(parent.as_ref()).await.map_err(fail)
}

#[tauri::command]
pub(crate) async fn database_query(
    app: AppHandle,
    connection: String,
    statement: String,
    limit: u32,
    offset: u64,
) -> CommandResult<serde_json::Value> {
    let (definition, secrets) = database_context(&app, &connection)?;
    let mut session = veronica_database::session::Session::open(&definition, &secrets)
        .await
        .map_err(fail)?;
    let page = session
        .query(
            &statement,
            &veronica_database::paging::PageRequest {
                page_size: veronica_database::paging::PageSize::clamped(limit),
                offset,
                ..Default::default()
            },
        )
        .await
        .map_err(fail)?;
    page_for_webview(&page)
}

#[tauri::command]
pub(crate) async fn database_read(
    app: AppHandle,
    connection: String,
    path: Vec<String>,
    limit: u32,
    offset: u64,
) -> CommandResult<serde_json::Value> {
    let (definition, secrets) = database_context(&app, &connection)?;
    let mut session = veronica_database::session::Session::open(&definition, &secrets)
        .await
        .map_err(fail)?;
    let page = session
        .read(
            &veronica_database::identify::ObjectIdentifier::for_path(definition.product_hint, path),
            &veronica_database::paging::PageRequest {
                page_size: veronica_database::paging::PageSize::clamped(limit),
                offset,
                ..Default::default()
            },
        )
        .await
        .map_err(fail)?;
    page_for_webview(&page)
}

const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

fn page_for_webview(page: &veronica_database::paging::Page) -> CommandResult<serde_json::Value> {
    let mut json = serde_json::to_value(page).map_err(|error| error.to_string())?;
    exact_large_integers(&mut json);
    Ok(json)
}

fn exact_large_integers(json: &mut serde_json::Value) {
    match json {
        serde_json::Value::Object(map) => {
            let integer = matches!(
                map.get("kind").and_then(serde_json::Value::as_str),
                Some("signedInteger" | "unsignedInteger")
            );
            if integer {
                if let Some(value) = map.get_mut("value") {
                    let unsafe_number = match value {
                        serde_json::Value::Number(number) => number
                            .as_i64()
                            .map(|n| n.unsigned_abs() > MAX_SAFE_INTEGER)
                            .or_else(|| number.as_u64().map(|n| n > MAX_SAFE_INTEGER))
                            .unwrap_or(false),
                        _ => false,
                    };
                    if unsafe_number {
                        *value = serde_json::Value::String(value.to_string());
                    }
                }
            }
            map.values_mut().for_each(exact_large_integers);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(exact_large_integers),
        _ => {}
    }
}

#[tauri::command]
pub(crate) async fn database_capabilities(
    app: AppHandle,
    connection: String,
) -> CommandResult<veronica_database::capabilities::Report> {
    let (definition, secrets) = database_context(&app, &connection)?;
    let mut session = veronica_database::session::Session::open(&definition, &secrets)
        .await
        .map_err(fail)?;
    session.capabilities().await.map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn database_operations(
    state: State<'_, AppState>,
    limit: usize,
) -> CommandResult<Vec<veronica_database::store::OperationRecord>> {
    veronica_database::store::MetadataStore::open(state.directories.database_store())
        .and_then(|store| store.operations(limit))
        .map_err(fail)
}

fn database_context(
    app: &AppHandle,
    connection: &str,
) -> CommandResult<(
    veronica_database::connection::ConnectionDefinition,
    veronica_database::secrets::SecretStore,
)> {
    let state = app.state::<AppState>();
    let store = veronica_database::store::MetadataStore::open(state.directories.database_store())
        .map_err(fail)?;
    let definition = store.resolve(connection).map_err(fail)?;
    let secrets =
        veronica_database::secrets::SecretStore::new(state.directories.database_secrets_fallback());
    Ok((definition, secrets))
}

#[tauri::command]
pub(crate) async fn packages_sources() -> Vec<PackageSource> {
    let mut sources = Vec::new();
    for source in veronica_system::packages::Source::ALL {
        sources.push(PackageSource {
            available: veronica_system::packages::available(source),
            needs_root: source.needs_root(),
            source: source.title().to_string(),
        });
    }
    sources
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageSource {
    pub source: String,
    pub available: bool,
    pub needs_root: bool,
}

#[tauri::command]
pub(crate) async fn packages_updates() -> Vec<veronica_system::packages::Update> {
    veronica_system::packages::updates().await
}

#[tauri::command]
pub(crate) async fn packages_inventory() -> Vec<veronica_system::packages::Installed> {
    veronica_system::packages::inventory().await
}

#[tauri::command]
pub(crate) async fn packages_update(
    source: String,
    names: Vec<String>,
) -> CommandResult<veronica_system::packages::UpdateResult> {
    let source = veronica_system::packages::Source::parse(&source)
        .ok_or_else(|| format!("unknown package source {source:?}"))?;
    veronica_system::packages::apply_update(source, &names)
        .await
        .map_err(fail)
}

#[tauri::command]
pub(crate) async fn packages_removal_plan(
    package: String,
) -> CommandResult<veronica_system::packages::RemovalPlan> {
    veronica_system::packages::removal_plan(&package)
        .await
        .map_err(fail)
}

#[tauri::command]
pub(crate) async fn tools_readiness() -> CommandResult<veronica_system::tools::Survey> {
    Ok(veronica_system::tools::survey().await)
}

fn requested_tool(id: &str) -> CommandResult<&'static veronica_core::tools::ToolSpec> {
    veronica_core::tools::spec(id).ok_or_else(|| {
        let choices = veronica_core::tools::CATALOG
            .iter()
            .map(|tool| tool.id)
            .collect::<Vec<_>>()
            .join(", ");
        format!("no tool called '{id}'; choose one of {choices}")
    })
}

#[tauri::command]
pub(crate) async fn tools_install(tool: String) -> CommandResult<veronica_system::tools::Outcome> {
    let spec = requested_tool(&tool)?;
    veronica_system::tools::install(spec, true)
        .await
        .map_err(fail)
}

#[cfg(test)]
mod tool_command_tests {
    use super::*;

    #[test]
    fn an_unknown_install_target_names_the_catalogue_choices() {
        let error = requested_tool("not-a-tool").unwrap_err();
        assert!(error.contains("no tool called 'not-a-tool'"));
        for tool in veronica_core::tools::CATALOG {
            assert!(error.contains(tool.id), "{error:?} omits {}", tool.id);
        }
    }

    #[test]
    fn a_catalogued_install_target_resolves_to_the_shared_spec() {
        assert_eq!(requested_tool("ssh").unwrap().id, "ssh");
    }
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn notifications_list(state: State<'_, AppState>) -> Vec<veronica_system::Notification> {
    state
        .notifications
        .lock()
        .expect("notifications lock")
        .clone()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn notifications_dismiss(state: State<'_, AppState>, id: u64) {
    state
        .notifications
        .lock()
        .expect("notifications lock")
        .retain(|entry| entry.id != id);
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn notifications_clear(state: State<'_, AppState>) {
    state
        .notifications
        .lock()
        .expect("notifications lock")
        .clear();
}

const MACHINES_KEY: &str = "machines";

fn machine_by_id(state: &AppState, id: &str) -> CommandResult<veronica_machines::Machine> {
    let stored = state
        .settings_snapshot()
        .get(MACHINES_KEY)
        .and_then(|value| {
            serde_json::from_value::<Vec<veronica_machines::Machine>>(value.clone()).ok()
        })
        .unwrap_or_default();
    veronica_machines::fleet(stored)
        .into_iter()
        .find(|machine| machine.id == id)
        .ok_or_else(|| format!("no machine called {id}"))
}

#[tauri::command]
pub(crate) async fn machines_probe(
    app: AppHandle,
) -> CommandResult<Vec<veronica_machines::MachineReport>> {
    let stored = {
        let state = app.state::<AppState>();
        let settings = state.settings_snapshot();
        settings
            .get(MACHINES_KEY)
            .and_then(|value| {
                serde_json::from_value::<Vec<veronica_machines::Machine>>(value.clone()).ok()
            })
            .unwrap_or_default()
    };
    let fleet = veronica_machines::fleet(stored);
    Ok(veronica_machines::probe_fleet(&fleet, veronica_machines::DEFAULT_TIMEOUT).await)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn machines_add(
    state: State<'_, AppState>,
    target: String,
    name: Option<String>,
    port: Option<u16>,
) -> CommandResult<veronica_machines::Machine> {
    use veronica_machines::host;

    let target = target.trim().to_string();
    host::validate_ssh_target(&target).map_err(fail)?;
    let label = name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| target.clone());
    let id = host::slugify(&label);
    if id == "local" {
        return Err("\"local\" is reserved for this computer".to_string());
    }

    let machine = veronica_machines::Machine {
        id,
        name: label,
        reach: veronica_machines::Reach::Ssh { target, port },
        mac: None,
    };
    state
        .update_settings(|settings| {
            let mut machines: Vec<veronica_machines::Machine> = settings
                .get(MACHINES_KEY)
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default();
            if machines.iter().any(|existing| existing.id == machine.id) {
                anyhow::bail!("a machine called {} already exists", machine.id);
            }
            machines.push(machine.clone());
            settings.set(MACHINES_KEY, serde_json::to_value(&machines)?);
            Ok(())
        })
        .map_err(fail)?;
    Ok(machine)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn machines_remove(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    state
        .update_settings(|settings| {
            let mut machines: Vec<veronica_machines::Machine> = settings
                .get(MACHINES_KEY)
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default();
            let before = machines.len();
            machines.retain(|machine| machine.id != id);
            if machines.len() == before {
                anyhow::bail!("no machine called {id}");
            }
            settings.set(MACHINES_KEY, serde_json::to_value(&machines)?);
            Ok(())
        })
        .map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn machines_discover(state: State<'_, AppState>) -> Vec<String> {
    use veronica_machines::host;

    let settings = state.settings_snapshot();
    let configured: Vec<veronica_machines::Machine> = settings
        .get(MACHINES_KEY)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    let config = veronica_core::paths::home_dir()
        .map(|home| home.join(".ssh/config"))
        .unwrap_or_default();
    host::ssh_config_hosts(&config)
        .into_iter()
        .filter(|alias| {
            !configured
                .iter()
                .any(|machine| machine.ssh_target() == Some(alias.as_str()))
        })
        .collect()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn machines_terminal(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    let machine = machine_by_id(&state, &id)?;
    veronica_machines::manage::open_terminal(&machine).map_err(fail)
}

#[tauri::command]
pub(crate) async fn machines_files(
    state: State<'_, AppState>,
    id: String,
    path: Option<String>,
) -> CommandResult<veronica_machines::MachineDirectory> {
    let machine = machine_by_id(&state, &id)?;
    veronica_machines::manage::list_directory(
        &machine,
        path.as_deref(),
        veronica_machines::DEFAULT_TIMEOUT,
    )
    .await
    .map_err(fail)
}

#[tauri::command]
pub(crate) async fn machines_file_download(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> CommandResult<String> {
    let machine = machine_by_id(&state, &id)?;
    if machine.is_local() {
        if !std::path::Path::new(&path).is_file() {
            return Err("selected path is not a file".to_string());
        }
        return Ok(path);
    }
    let downloads = veronica_core::paths::home_dir()
        .ok_or_else(|| "cannot resolve Downloads directory".to_string())?
        .join("Downloads");
    let destination = veronica_machines::manage::download_file(
        &machine,
        &path,
        &downloads,
        veronica_machines::DEFAULT_TIMEOUT,
    )
    .await
    .map_err(fail)?;
    Ok(destination.display().to_string())
}

#[tauri::command]
pub(crate) async fn machines_containers(
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<Vec<veronica_machines::ContainerInfo>> {
    let machine = machine_by_id(&state, &id)?;
    veronica_machines::manage::containers(&machine, veronica_machines::DEFAULT_TIMEOUT)
        .await
        .map_err(fail)
}

#[tauri::command]
pub(crate) async fn machines_container_action(
    state: State<'_, AppState>,
    id: String,
    engine: String,
    container: String,
    action: String,
) -> CommandResult<()> {
    let machine = machine_by_id(&state, &id)?;
    veronica_machines::manage::container_action(
        &machine,
        &engine,
        &container,
        &action,
        veronica_machines::DEFAULT_TIMEOUT,
    )
    .await
    .map_err(fail)
}

#[tauri::command]
pub(crate) async fn machines_container_logs(
    state: State<'_, AppState>,
    id: String,
    engine: String,
    container: String,
    lines: usize,
) -> CommandResult<String> {
    let machine = machine_by_id(&state, &id)?;
    veronica_machines::manage::container_logs(
        &machine,
        &engine,
        &container,
        lines,
        veronica_machines::DEFAULT_TIMEOUT,
    )
    .await
    .map_err(fail)
}

#[tauri::command]
pub(crate) async fn machines_power(
    state: State<'_, AppState>,
    id: String,
    action: String,
) -> CommandResult<()> {
    let machine = machine_by_id(&state, &id)?;
    let parsed = veronica_machines::manage::PowerAction::parse(&action)
        .ok_or_else(|| format!("unknown power action {action:?}"))?;
    veronica_machines::manage::power(&machine, parsed, veronica_machines::DEFAULT_TIMEOUT)
        .await
        .map_err(fail)
}

#[tauri::command]
pub(crate) async fn machines_wake(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    let machine = machine_by_id(&state, &id)?;
    let mac = machine.mac.clone().ok_or_else(|| {
        format!(
            "{} has no MAC address stored; add one with \
             `vr machines add <target> --mac aa:bb:cc:dd:ee:ff`",
            machine.name
        )
    })?;
    veronica_machines::manage::wake(&mac).map_err(fail)
}

#[tauri::command]
pub(crate) async fn presenter_state(
    app: AppHandle,
) -> CommandResult<crate::presenter::PresenterView> {
    let enabled = app
        .state::<AppState>()
        .settings_snapshot()
        .bool_or("presenterEnabled", false);
    let share = if enabled {
        veronica_system::screencast::detect().await
    } else {
        veronica_system::screencast::ScreenShareState::default()
    };
    Ok(crate::presenter::view(&app.state::<AppState>(), share))
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn presenter_set(app: AppHandle, action: String) -> CommandResult<()> {
    let state = app.state::<AppState>();
    let (key, value) = match action.as_str() {
        "enable" => ("presenterEnabled", serde_json::Value::Bool(true)),
        "disable" => ("presenterEnabled", serde_json::Value::Bool(false)),
        "start" => ("presenterMode", serde_json::Value::Bool(true)),
        "stop" => ("presenterMode", serde_json::Value::Bool(false)),
        "dismiss" => ("presenterAutoPaused", serde_json::Value::Bool(true)),
        "resume" => ("presenterAutoPaused", serde_json::Value::Bool(false)),
        other => return Err(format!("unknown presenter action: {other}")),
    };
    if matches!(action.as_str(), "start" | "stop")
        && !state.settings_snapshot().bool_or("presenterEnabled", false)
    {
        return Err("presenter mode is not enabled; turn it on in Settings › Presenter".into());
    }
    state.set_setting(key, value).map_err(fail)?;
    let _ = app.emit("settings-updated", "presenter");
    Ok(())
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn show_main_window(app: AppHandle) -> CommandResult<()> {
    crate::main_window::show(&app, None).map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn open_external(target: String) -> CommandResult<()> {
    let allowed =
        target.starts_with("https://") || target.starts_with("http://") || target.starts_with('/');
    if !allowed {
        return Err(format!("refusing to open {target:?}"));
    }
    veronica_core::host::spawn_detached(veronica_core::host::command("xdg-open").arg(&target))
        .map_err(|e| format!("cannot open {target}: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn integers_past_javascripts_range_reach_the_webview_exactly() {
        use veronica_database::value::Value;
        let mut json = serde_json::json!([
            serde_json::to_value(Value::SignedInteger(1_234_567_890_123_456_789)).unwrap(),
            serde_json::to_value(Value::SignedInteger(-9_007_199_254_740_993)).unwrap(),
            serde_json::to_value(Value::UnsignedInteger(u64::MAX)).unwrap(),
            serde_json::to_value(Value::SignedInteger(42)).unwrap(),
            serde_json::to_value(Value::Array(vec![Value::UnsignedInteger(1 << 60)])).unwrap(),
        ]);
        super::exact_large_integers(&mut json);
        assert_eq!(json[0]["value"], "1234567890123456789");
        assert_eq!(json[1]["value"], "-9007199254740993");
        assert_eq!(json[2]["value"], "18446744073709551615");
        assert_eq!(json[3]["value"], 42, "safe integers stay numbers");
        assert_eq!(json[4]["value"][0]["value"], "1152921504606846976");
    }

    use super::{parsed_shell_extension_status, with_veronica_enabled};

    #[test]
    fn shell_extension_info_becomes_an_actionable_ui_state() {
        let active = parsed_shell_extension_status(
            "Enabled: Yes\nState: ACTIVE\nPath: /usr/share/gnome-shell/extensions/example",
        );
        assert_eq!(active.state, "active");
        assert!(!active.can_enable);

        let disabled = parsed_shell_extension_status("State: INACTIVE\nEnabled: No");
        assert_eq!(disabled.state, "disabled");
        assert!(disabled.can_enable);

        let waiting = parsed_shell_extension_status("Enabled: Yes\nState: INACTIVE");
        assert_eq!(waiting.state, "restartRequired");
        assert!(!waiting.can_enable);

        let failed = parsed_shell_extension_status("Enabled: Yes\nState: ERROR");
        assert_eq!(failed.state, "error");
        assert!(!failed.can_enable);
    }

    #[test]
    fn the_extension_joins_gnomes_enabled_list_once() {
        assert_eq!(
            with_veronica_enabled("@as []").as_deref(),
            Some("['veronica@namannn04.github.io']")
        );
        assert_eq!(
            with_veronica_enabled("['ding@rastersoft.com']").as_deref(),
            Some("['ding@rastersoft.com', 'veronica@namannn04.github.io']")
        );
        assert_eq!(
            with_veronica_enabled("['veronica@namannn04.github.io', 'ding@rastersoft.com']"),
            None
        );
    }

    #[test]
    fn only_web_urls_and_absolute_paths_are_openable() {
        for refused in [
            "file:///etc/passwd",
            "veronica; rm -rf /",
            "relative/path",
            "",
            "ftp://example.com",
        ] {
            assert!(
                super::open_external(refused.to_string()).is_err(),
                "{refused:?} should be refused"
            );
        }
    }
}

#[cfg(test)]
mod wire_shape_tests {
    use super::*;
    use crate::commands::assert_camel_case;

    #[test]
    fn the_presenter_view_is_camel_case_including_the_flattened_state() {
        let view = crate::presenter::PresenterView {
            state: veronica_core::PresenterState::default(),
            active: false,
            blurred_classes: vec!["blur-money"],
            share: veronica_system::screencast::ScreenShareState::default(),
        };
        assert_camel_case(&view, "PresenterView");
        let json = serde_json::to_value(&view).unwrap();
        assert!(json["autoEnabled"].is_boolean());
        assert!(json["blurredClasses"].is_array());
        assert!(json["share"]["screencastSessions"].is_number());
    }

    #[test]
    fn the_extension_report_carries_the_settings_key_the_interface_toggles() {
        let report = Diagnostics::collect(
            &veronica_core::AppDirectories::with_env(
                std::path::Path::new("/tmp"),
                veronica_core::paths::EnvOverrides::default(),
            ),
            veronica_core::DesktopSession::unknown(),
            &veronica_core::Settings::default(),
        );
        let json = serde_json::to_value(&report).unwrap();
        let extensions = json["extensions"].as_array().expect("extensions");
        assert!(!extensions.is_empty());
        for entry in extensions {
            assert!(
                entry["defaultsKey"]
                    .as_str()
                    .is_some_and(|key| !key.is_empty()),
                "an extension without a settings key cannot be toggled: {entry}"
            );
        }
    }

    #[test]
    fn a_not_run_tool_install_is_a_result_the_interface_can_explain() {
        let outcome = veronica_system::tools::Outcome::NotRun {
            command: Some("npm install -g example".to_string()),
            instruction: "Install it manually.",
            reason: "The prefix is not writable.".to_string(),
        };
        let json = serde_json::to_value(outcome).unwrap();
        assert_eq!(json["outcome"], "notRun");
        assert_eq!(json["command"], "npm install -g example");
        assert_eq!(json["instruction"], "Install it manually.");
        assert_eq!(json["reason"], "The prefix is not writable.");
    }
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn agent_profiles(
    state: State<'_, AppState>,
) -> CommandResult<Vec<veronica_system::agents::AgentProfile>> {
    let path = state.directories.data.join("agents/profiles.json");
    let mut profiles: Vec<veronica_system::agents::AgentProfile> =
        veronica_system::private_store::load(&path).map_err(fail)?;
    if profiles.is_empty() {
        profiles = veronica_system::agents::defaults();
    }
    Ok(profiles)
}
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn agent_profiles_save(
    state: State<'_, AppState>,
    profiles: Vec<veronica_system::agents::AgentProfile>,
) -> CommandResult<()> {
    if profiles.len() > 16
        || profiles.iter().any(|p| {
            p.id.is_empty()
                || p.id.len() > 80
                || p.name.len() > 80
                || p.executable.len() > 4096
                || p.arguments.len() > 32
                || p.arguments.iter().any(|a| a.len() > 2048)
        })
    {
        return Err("Use at most 16 bounded provider profiles".into());
    }
    veronica_system::private_store::save(
        &state.directories.data.join("agents/profiles.json"),
        &profiles,
    )
    .map_err(fail)
}
#[tauri::command]
pub(crate) async fn agent_start(
    app: AppHandle,
    profile: veronica_system::agents::AgentProfile,
    cwd: String,
) -> CommandResult<veronica_system::agents::AgentView> {
    let epoch = app.state::<AppState>().agents.epoch();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .agents
            .start(profile, cwd, epoch)
            .map_err(fail)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn agent_list(state: State<'_, AppState>) -> Vec<veronica_system::agents::AgentView> {
    let root = state.directories.data.join("agents");
    state
        .agents
        .list()
        .into_iter()
        .map(|mut a| {
            if a.status == "running" {
                if let Some(status) = veronica_system::agent_tracking::status(&root, a.pid) {
                    a.status = format!("{status} · Claude hook (last reported)");
                }
            }
            a
        })
        .collect()
}
#[tauri::command]
pub(crate) async fn agent_external(
    app: AppHandle,
) -> CommandResult<Vec<veronica_system::agents::AgentView>> {
    let root = app.state::<AppState>().directories.data.join("agents");
    tauri::async_runtime::spawn_blocking(move || {
        veronica_system::agents::external()
            .into_iter()
            .map(|mut a| {
                if let Some(status) = veronica_system::agent_tracking::status(&root, a.pid) {
                    a.status = format!("{status} · Claude hook (last reported)");
                }
                a
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn agent_output(
    state: State<'_, AppState>,
    id: String,
    cursor: u64,
) -> CommandResult<veronica_system::agents::TerminalOutput> {
    state.agents.output(&id, cursor).map_err(fail)
}
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn agent_input(
    state: State<'_, AppState>,
    id: String,
    bytes: String,
) -> CommandResult<()> {
    state.agents.input(&id, bytes).map_err(fail)
}
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn agent_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> CommandResult<()> {
    state.agents.resize(&id, cols, rows).map_err(fail)
}
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn agent_stop(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    state.agents.stop(&id).map_err(fail)
}

#[tauri::command]
pub(crate) async fn agent_tracking(
    app: AppHandle,
    enabled: bool,
    confirmed: bool,
) -> CommandResult<()> {
    if !confirmed {
        return Err("Confirm changes to Claude's hook settings".into());
    }
    let home = veronica_core::paths::home_dir().ok_or("Cannot locate home directory")?;
    let root = app.state::<AppState>().directories.data.join("agents");
    tauri::async_runtime::spawn_blocking(move || {
        veronica_system::agent_tracking::configure(&home, &root, enabled).map_err(fail)
    })
    .await
    .map_err(|e| e.to_string())?
}
