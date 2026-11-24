use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use veronica_core::{Capabilities, Diagnostics};
use veronica_system::metrics::SystemSnapshot;
use veronica_usage::aggregate::{self, DayRange, SourceSelection};
use veronica_usage::collector;

use crate::state::AppState;

type CommandResult<T> = Result<T, String>;

fn fail(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageView {
    pub dashboard: Option<aggregate::Dashboard>,
    pub generated_at: Option<String>,
    pub sources: Vec<SourceOption>,
    pub models: Vec<String>,
    pub has_data: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceOption {
    pub id: String,
    pub label: String,
}

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
pub(crate) fn usage_view(
    state: State<'_, AppState>,
    days: Option<usize>,
    sources: Vec<String>,
) -> UsageView {
    let guard = state.usage.lock().expect("usage lock");
    let Some(document) = guard.as_ref() else {
        return UsageView {
            dashboard: None,
            generated_at: None,
            sources: Vec::new(),
            models: Vec::new(),
            has_data: false,
        };
    };

    let range = match days {
        Some(days) => DayRange::last_days(days, chrono::Local::now().date_naive()),
        None => DayRange::default(),
    };
    let selection = if sources.is_empty() {
        SourceSelection::All
    } else {
        SourceSelection::Only(sources)
    };

    let mut models: Vec<String> = document
        .daily
        .iter()
        .flat_map(|day| day.by_source.values())
        .flatten()
        .map(|row| row.model_name.clone())
        .collect();
    models.sort_unstable();
    models.dedup();

    UsageView {
        dashboard: Some(aggregate::dashboard(document, &range, &selection)),
        generated_at: Some(document.generated_at.clone()),
        sources: document
            .sources
            .iter()
            .map(|id| SourceOption {
                id: id.clone(),
                label: document.label_for(id),
            })
            .collect(),
        models,
        has_data: true,
    }
}

#[tauri::command]
pub(crate) async fn usage_export(
    state: State<'_, AppState>,
    cards: Vec<String>,
    days: Option<usize>,
    sources: Option<Vec<String>>,
) -> CommandResult<Vec<String>> {
    use veronica_usage::cards::{self, Card};

    let chosen: Vec<Card> = if cards.is_empty() {
        Card::ALL.to_vec()
    } else {
        let mut chosen = Vec::new();
        for raw in &cards {
            for card in Card::parse(raw).ok_or_else(|| format!("unknown card {raw:?}"))? {
                if !chosen.contains(&card) {
                    chosen.push(card);
                }
            }
        }
        chosen
    };

    let (board, label) = {
        let guard = state.usage.lock().expect("usage lock");
        let document = guard
            .as_ref()
            .ok_or_else(|| "No usage has been collected yet.".to_string())?;
        let range = match days {
            Some(days) => DayRange::last_days(days, chrono::Local::now().date_naive()),
            None => DayRange::default(),
        };
        let label = match (range.start.as_deref(), range.end.as_deref()) {
            (Some(start), Some(end)) if start == end => start.to_string(),
            (Some(start), Some(end)) => format!("{start} to {end}"),
            _ => "all time".to_string(),
        };
        let selection = match sources.filter(|sources| !sources.is_empty()) {
            Some(sources) => SourceSelection::Only(sources),
            None => SourceSelection::All,
        };
        (
            veronica_usage::dashboard(document, &range, &selection),
            label,
        )
    };
    if board.totals.tokens == 0 {
        return Err("No usage in this window, so there is nothing to put on a card.".to_string());
    }

    let directory = veronica_core::paths::home_dir()
        .map(|home| home.join("Pictures"))
        .filter(|pictures| pictures.is_dir())
        .or_else(veronica_core::paths::home_dir)
        .ok_or_else(|| "cannot resolve the home directory".to_string())?;
    let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string();

    tauri::async_runtime::spawn_blocking(move || {
        let mut written = Vec::new();
        for card in chosen {
            let png = cards::render_png(&cards::render_svg(card, &board, &label)).map_err(fail)?;
            let path = directory.join(cards::file_name(card, &stamp, "png"));
            std::fs::write(&path, &png)
                .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
            written.push(path.display().to_string());
        }
        Ok(written)
    })
    .await
    .map_err(|error| format!("the export could not run: {error}"))?
}

#[tauri::command]
pub(crate) async fn usage_refresh(app: AppHandle) -> CommandResult<String> {
    {
        let state = app.state::<AppState>();
        let mut refreshing = state.refreshing.lock().expect("refresh lock");
        if *refreshing {
            return Err("A refresh is already running.".to_string());
        }
        *refreshing = true;
    }

    let result = run_refresh(&app).await;

    let state = app.state::<AppState>();
    *state.refreshing.lock().expect("refresh lock") = false;

    result
}

async fn run_refresh(app: &AppHandle) -> CommandResult<String> {
    let (script, out_dir, cache_dir) = {
        let state = app.state::<AppState>();
        (
            state.directories.collector_script(),
            state.directories.usage_dir(),
            state.directories.cache.clone(),
        )
    };

    collector::install_script(&script).map_err(fail)?;

    let emitter = app.clone();
    let outcome = collector::refresh(&script, &out_dir, &cache_dir, move |event| {
        let _ = emitter.emit("usage-progress", event);
    })
    .await
    .map_err(fail)?;

    let generated_at = outcome.document.generated_at.clone();
    {
        let state = app.state::<AppState>();
        *state.usage.lock().expect("usage lock") = Some(outcome.document);
    }
    let _ = app.emit("usage-updated", &generated_at);
    Ok(generated_at)
}

#[tauri::command]
pub(crate) async fn usage_limits() -> CommandResult<veronica_usage::GaugeReport> {
    Ok(veronica_usage::gauges::collect(
        chrono::Utc::now(),
        veronica_usage::gauges::DEFAULT_PACING_MARGIN,
    )
    .await)
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

const SNAP_LAUNCHER: &str = "/snap/bin/veronica";

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
    match install_kind() {
        "flatpak" => Some((FLATPAK_SHELL_EXTENSION.into(), FLATPAK_CLI_SHIM)),
        "snap" => std::env::var_os("SNAP").map(|root| {
            let source = std::path::Path::new(&root)
                .join("usr/share/gnome-shell/extensions")
                .join(SHELL_EXTENSION_UUID);
            (source, SNAP_CLI_SHIM)
        }),
        _ => None,
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
    crate::sync_global_shortcuts(&app, &app.state::<AppState>().settings_snapshot());
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
    crate::sync_global_shortcuts(&app, &state.settings_snapshot());
    let _ = app.emit("settings-updated", "backup-import");
    let _ = app.emit("usage-updated", "backup-import");
    Ok(report)
}

fn companion_repository(state: &AppState) -> veronica_core::CompanionRepository {
    veronica_core::CompanionRepository::new(state.directories.companion_dir())
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn companion_list(
    state: State<'_, AppState>,
    query: String,
) -> CommandResult<Vec<veronica_core::CompanionItem>> {
    companion_repository(&state).list(&query).map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn companion_note_create(
    app: AppHandle,
    title: String,
    body: String,
) -> CommandResult<veronica_core::CompanionItem> {
    let item = companion_repository(&app.state::<AppState>())
        .create_note(&title, &body, chrono::Utc::now())
        .map_err(fail)?;
    let _ = app.emit("companion-updated", item.id);
    Ok(item)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn companion_update(
    app: AppHandle,
    id: u64,
    title: String,
    body: String,
    pinned: bool,
) -> CommandResult<veronica_core::CompanionItem> {
    let item = companion_repository(&app.state::<AppState>())
        .update(id, &title, &body, pinned, chrono::Utc::now())
        .map_err(fail)?;
    let _ = app.emit("companion-updated", item.id);
    Ok(item)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn companion_remove(app: AppHandle, id: u64) -> CommandResult<()> {
    companion_repository(&app.state::<AppState>())
        .remove(id)
        .map_err(fail)?;
    let _ = app.emit("companion-updated", id);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompanionRecordingStatus {
    pub recording: bool,
    pub elapsed_seconds: u64,
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn companion_recording_status(state: State<'_, AppState>) -> CompanionRecordingStatus {
    let recording = state
        .companion_recording
        .lock()
        .expect("companion recording lock");
    CompanionRecordingStatus {
        recording: recording.is_some(),
        elapsed_seconds: recording
            .as_ref()
            .map_or(0, |recording| recording.started.elapsed().as_secs()),
    }
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn companion_record_start(state: State<'_, AppState>) -> CommandResult<()> {
    let mut active = state
        .companion_recording
        .lock()
        .expect("companion recording lock");
    if active.is_some() {
        return Err("a voice memo is already recording".to_string());
    }
    let repository = companion_repository(&state);
    let directory = repository.recordings_dir();
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!(
        "voice-{}-{}.wav",
        chrono::Utc::now().timestamp_millis(),
        std::process::id()
    ));
    let mut child = veronica_core::host::command("pw-record")
        .args([
            "--media-category",
            "Capture",
            "--media-role",
            "Communication",
            "--rate",
            "48000",
            "--channels",
            "1",
        ])
        .arg(&path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot start PipeWire recorder: {error}"))?;
    std::thread::sleep(std::time::Duration::from_millis(120));
    if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
        let _ = std::fs::remove_file(&path);
        return Err(format!("PipeWire recorder exited immediately ({status})"));
    }
    *active = Some(crate::state::CompanionRecording {
        child,
        path,
        started: std::time::Instant::now(),
    });
    Ok(())
}

fn stop_companion_process(recording: &mut crate::state::CompanionRecording) {
    let _ = std::process::Command::new("kill")
        .args(["-INT", &recording.child.id().to_string()])
        .status();
    for _ in 0..20 {
        if recording.child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let _ = recording.child.kill();
    let _ = recording.child.wait();
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn companion_record_stop(app: AppHandle) -> CommandResult<veronica_core::CompanionItem> {
    let state = app.state::<AppState>();
    let recording = state
        .companion_recording
        .lock()
        .expect("companion recording lock")
        .take()
        .ok_or_else(|| "no voice memo is recording".to_string())?;
    let item = finish_companion_recording(&state, recording)?;
    let _ = app.emit("companion-updated", item.id);
    Ok(item)
}

pub(crate) fn finish_companion_recording_on_exit(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(recording) = state
        .companion_recording
        .lock()
        .expect("companion recording lock")
        .take()
    else {
        return;
    };
    match finish_companion_recording(&state, recording) {
        Ok(item) => tracing::info!("saved the voice memo in progress at quit as #{}", item.id),
        Err(error) => tracing::warn!("cannot save the voice memo in progress at quit: {error}"),
    }
}

fn finish_companion_recording(
    state: &AppState,
    mut recording: crate::state::CompanionRecording,
) -> CommandResult<veronica_core::CompanionItem> {
    stop_companion_process(&mut recording);
    let bytes = std::fs::metadata(&recording.path).map_or(0, |metadata| metadata.len());
    if bytes <= 44 {
        let _ = std::fs::remove_file(&recording.path);
        return Err("the microphone produced no audio; check PipeWire input access".to_string());
    }
    companion_repository(state)
        .add_voice(
            &recording.path,
            recording.started.elapsed().as_secs().max(1),
            chrono::Utc::now(),
        )
        .map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn companion_record_cancel(state: State<'_, AppState>) {
    let Some(mut recording) = state
        .companion_recording
        .lock()
        .expect("companion recording lock")
        .take()
    else {
        return;
    };
    stop_companion_process(&mut recording);
    let _ = std::fs::remove_file(recording.path);
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn companion_audio(state: State<'_, AppState>, id: u64) -> CommandResult<String> {
    let path = companion_repository(&state).audio_path(id).map_err(fail)?;
    Ok(path.display().to_string())
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn attention_status(
    state: State<'_, AppState>,
) -> CommandResult<veronica_core::AttentionStatus> {
    veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .status(chrono::Utc::now())
        .map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn attention_start(
    state: State<'_, AppState>,
    name: String,
    duration_seconds: i64,
) -> CommandResult<veronica_core::AttentionFocusSession> {
    veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .start_focus(&name, duration_seconds, chrono::Utc::now())
        .map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn attention_stop(
    state: State<'_, AppState>,
) -> CommandResult<veronica_core::AttentionFocusSession> {
    veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .stop_focus(chrono::Utc::now())
        .map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn attention_history(
    state: State<'_, AppState>,
) -> CommandResult<Vec<veronica_core::AttentionFocusSession>> {
    let mut sessions = veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .history()
        .map_err(fail)?;
    sessions.reverse();
    Ok(sessions)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn attention_overview(
    state: State<'_, AppState>,
    days: i64,
) -> CommandResult<veronica_core::AttentionOverview> {
    let now = chrono::Utc::now();
    veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .overview(
            veronica_core::AttentionRepository::window_start(days, now),
            now,
        )
        .map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn attention_settings(
    state: State<'_, AppState>,
) -> CommandResult<veronica_core::AttentionSettings> {
    veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .load_settings()
        .map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn attention_settings_save(
    app: AppHandle,
    settings: veronica_core::AttentionSettings,
) -> CommandResult<()> {
    let state = app.state::<AppState>();
    veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .save_settings(&settings)
        .map_err(fail)?;
    state
        .update_settings(|shared| {
            shared.set(
                "attentionTrackingEnabled",
                serde_json::json!(settings.enabled),
            );
            shared.set(
                "attentionPrivacy",
                serde_json::json!(match settings.privacy {
                    veronica_core::AttentionPrivacy::Detailed => "detailed",
                    veronica_core::AttentionPrivacy::Applications => "applications",
                }),
            );
            shared.set(
                "attentionIdleSeconds",
                serde_json::json!(settings.idle_threshold_seconds),
            );
            Ok(())
        })
        .map_err(fail)?;
    let _ = app.emit("settings-updated", ());
    Ok(())
}

pub(crate) async fn sync_prevent_sleep(app: &AppHandle, enabled: bool) -> CommandResult<()> {
    if !enabled {
        app.state::<AppState>()
            .prevent_sleep
            .lock()
            .expect("prevent sleep lock")
            .take();
        return Ok(());
    }
    if app
        .state::<AppState>()
        .prevent_sleep
        .lock()
        .expect("prevent sleep lock")
        .is_some()
    {
        return Ok(());
    }

    let connection = zbus::Connection::system()
        .await
        .map_err(|error| format!("cannot reach systemd-logind: {error}"))?;
    let inhibitor = veronica_system::power::inhibit(
        &connection,
        veronica_system::power::InhibitWhat::Idle,
        "Veronica",
        "Keep Awake is enabled",
        veronica_system::power::InhibitMode::Block,
    )
    .await
    .map_err(fail)?;

    *app.state::<AppState>()
        .prevent_sleep
        .lock()
        .expect("prevent sleep lock") = Some(inhibitor);
    Ok(())
}

pub(crate) async fn sync_lid_awake(app: &AppHandle, enabled: bool) -> CommandResult<()> {
    if !enabled {
        app.state::<AppState>()
            .lid_awake
            .lock()
            .expect("lid awake lock")
            .take();
        return Ok(());
    }

    if app
        .state::<AppState>()
        .lid_awake
        .lock()
        .expect("lid awake lock")
        .is_some()
    {
        return Ok(());
    }

    let connection = zbus::Connection::system()
        .await
        .map_err(|error| format!("cannot reach systemd-logind: {error}"))?;
    let inhibitor = veronica_system::power::inhibit(
        &connection,
        veronica_system::power::InhibitWhat::IdleAndLidSwitch,
        "Veronica",
        "Lid Awake is enabled",
        veronica_system::power::InhibitMode::Block,
    )
    .await
    .map_err(fail)?;

    *app.state::<AppState>()
        .lid_awake
        .lock()
        .expect("lid awake lock") = Some(inhibitor);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PowerStatus {
    pub has_lid: bool,
    pub lid_awake_active: bool,
    pub prevent_sleep_active: bool,
    pub lid_awake_remaining_secs: Option<i64>,
    pub prevent_sleep_remaining_secs: Option<i64>,
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn power_status(state: State<'_, AppState>) -> PowerStatus {
    let settings = state.settings_snapshot();
    let now = chrono::Utc::now().timestamp_millis();
    PowerStatus {
        lid_awake_remaining_secs: veronica_core::AwakeState::read(
            &settings,
            veronica_core::Awake::LidAwake,
        )
        .remaining_secs(now),
        prevent_sleep_remaining_secs: veronica_core::AwakeState::read(
            &settings,
            veronica_core::Awake::KeepAwake,
        )
        .remaining_secs(now),
        has_lid: veronica_system::power::has_lid(),
        lid_awake_active: state.lid_awake.lock().expect("lid awake lock").is_some(),
        prevent_sleep_active: state
            .prevent_sleep
            .lock()
            .expect("prevent sleep lock")
            .is_some(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
    pub package_url: Option<String>,
    pub published_at: Option<String>,
    pub notes: String,
    pub install_kind: String,
    pub can_install: bool,
}

fn install_kind() -> &'static str {
    if veronica_core::host::is_sandboxed() {
        "flatpak"
    } else if std::env::var_os("SNAP_NAME").is_some_and(|name| name == "veronica") {
        "snap"
    } else if std::env::var_os("APPIMAGE").is_some() {
        "appimage"
    } else {
        "deb"
    }
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    published_at: Option<String>,
    body: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

fn package_asset<'a>(assets: &'a [GithubAsset], kind: &str) -> Option<&'a GithubAsset> {
    let suffix = match kind {
        "appimage" => ".AppImage",
        "flatpak" => ".flatpak",
        "snap" => ".snap",
        _ => "_amd64.deb",
    };
    assets.iter().find(|asset| asset.name.ends_with(suffix))
}

fn version_parts(value: &str) -> Vec<u64> {
    value
        .trim_start_matches(['v', 'V'])
        .split('.')
        .map(|part| {
            part.split(['-', '+'])
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0)
        })
        .collect()
}

fn newer_version(latest: &str, current: &str) -> bool {
    let mut latest = version_parts(latest);
    let mut current = version_parts(current);
    let width = latest.len().max(current.len());
    latest.resize(width, 0);
    current.resize(width, 0);
    latest > current
}

#[tauri::command]
pub(crate) async fn update_check() -> CommandResult<UpdateInfo> {
    tauri::async_runtime::spawn_blocking(|| {
        let fetch = |url: &str| std::process::Command::new("curl")
            .args([
                "--fail", "--silent", "--show-error", "--location",
                "--max-time", "12", "--header", "Accept: application/vnd.github+json",
                "--header", "X-GitHub-Api-Version: 2022-11-28",
                url,
            ])
            .output()
            .map_err(|error| format!("cannot run curl to check updates: {error}"));
        let current = env!("CARGO_PKG_VERSION").to_string();
        let release_output = fetch("https://api.github.com/repos/namannn04/Veronica/releases/latest")?;
        if release_output.status.success() {
            let release: GithubRelease = serde_json::from_slice(&release_output.stdout)
                .map_err(|error| format!("GitHub returned invalid release data: {error}"))?;
            let package_url = package_asset(&release.assets, install_kind())
                .map(|asset| asset.browser_download_url.clone());
            return Ok(UpdateInfo {
                update_available: newer_version(&release.tag_name, &current),
                current_version: current,
                latest_version: release.tag_name.trim_start_matches(['v', 'V']).to_string(),
                release_url: release.html_url,
                package_url,
                published_at: release.published_at,
                notes: release.body.unwrap_or_default(),
                install_kind: install_kind().into(),
                can_install: install_kind() != "appimage",
            });
        }

        let manifest_output = fetch("https://raw.githubusercontent.com/namannn04/Veronica/main/apps/desktop/src-tauri/tauri.conf.json")?;
        if !manifest_output.status.success() {
            let detail = String::from_utf8_lossy(&manifest_output.stderr);
            return Err(format!("cannot check Veronica releases: {}", detail.trim()));
        }
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_output.stdout)
            .map_err(|error| format!("GitHub returned an invalid Veronica manifest: {error}"))?;
        let latest = manifest.get("version").and_then(|value| value.as_str())
            .ok_or_else(|| "Veronica's online manifest has no version".to_string())?;
        Ok(UpdateInfo {
            update_available: newer_version(latest, &current),
            current_version: current,
            latest_version: latest.to_string(),
            release_url: "https://github.com/namannn04/Veronica".to_string(),
            package_url: None,
            published_at: None,
            notes: "No packaged GitHub Release has been published yet.".to_string(),
            install_kind: install_kind().into(),
            can_install: false,
        })
    }).await.map_err(|error| format!("update check failed: {error}"))?
}

#[tauri::command]
pub(crate) async fn update_install(version: String) -> CommandResult<String> {
    let valid = version.split('.').count() == 3
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    if !valid {
        return Err(format!("invalid release version: {version}"));
    }
    tauri::async_runtime::spawn_blocking(move || match install_kind() {
        "flatpak" => {
            let output = veronica_core::host::command("flatpak")
                .args(["update", "--noninteractive", "-y", veronica_core::APP_ID])
                .output()
                .map_err(|error| format!("cannot run flatpak: {error}"))?;
            if output.status.success() {
                Ok(format!("Veronica {version} is installed."))
            } else {
                Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
            }
        }
        "snap" => {
            let output = std::process::Command::new("snap")
                .args(["refresh", "veronica"])
                .output()
                .map_err(|error| format!("cannot run snap: {error}"))?;
            if output.status.success() {
                Ok(format!("Veronica {version} is installed."))
            } else {
                Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
            }
        }
        "deb" => install_deb_release(&version),
        _ => Err(
            "An AppImage cannot update itself; download the new one from the release page.".into(),
        ),
    })
    .await
    .map_err(|error| format!("update failed: {error}"))?
}

fn install_deb_release(version: &str) -> CommandResult<String> {
    let asset = format!("Veronica_{version}_amd64.deb");
    let root = format!("https://github.com/namannn04/Veronica/releases/download/v{version}");
    let directory = tempfile::Builder::new()
        .prefix(&format!("veronica-update-{version}-"))
        .tempdir()
        .map_err(|error| format!("cannot prepare update download: {error}"))?;
    let work = directory.path();
    (|| {
        let download = |name: &str| -> CommandResult<std::path::PathBuf> {
            let path = work.join(name);
            veronica_system::download::download_https(&format!("{root}/{name}"), &path)
                .map_err(|error| format!("cannot download {name}: {error:#}"))
        };
        let package = download(&asset)?;
        let sums =
            std::fs::read_to_string(download("SHA256SUMS")?).map_err(|error| error.to_string())?;
        let expected = sums
            .lines()
            .find_map(|line| {
                let mut fields = line.split_whitespace();
                let hash = fields.next()?;
                (fields.next()? == asset).then(|| hash.to_string())
            })
            .ok_or_else(|| format!("the release checksums do not list {asset}"))?;
        let actual = std::process::Command::new("sha256sum")
            .arg(&package)
            .output()
            .map_err(|error| format!("cannot run sha256sum: {error}"))?;
        let actual = String::from_utf8_lossy(&actual.stdout);
        if actual.split_whitespace().next() != Some(expected.as_str()) {
            return Err(
                "the download does not match its published checksum; nothing was installed".into(),
            );
        }

        let output = std::process::Command::new("pkexec")
            .args(["apt-get", "install", "--reinstall", "-y"])
            .arg(&package)
            .env("DEBIAN_FRONTEND", "noninteractive")
            .output()
            .map_err(|error| format!("cannot run pkexec: {error}"))?;
        match output.status.code() {
            Some(0) => Ok(format!("Veronica {version} is installed.")),
            Some(126 | 127) => Err("Authentication was cancelled; nothing was installed.".into()),
            _ => {
                let text = String::from_utf8_lossy(&output.stderr);
                let mut tail: Vec<&str> = text.lines().rev().take(5).collect();
                tail.reverse();
                Err(tail.join("\n"))
            }
        }
    })()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn app_restart(app: AppHandle) -> CommandResult<()> {
    let relaunch = restart_command(install_kind(), std::env::current_exe().ok());
    std::process::Command::new(&relaunch[0])
        .args(&relaunch[1..])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot start the new version: {error}"))?;
    app.exit(0);
    Ok(())
}

fn restart_command(kind: &str, current_exe: Option<std::path::PathBuf>) -> Vec<String> {
    const WAIT: &str = "sleep 2";
    if kind == "flatpak" {
        vec![
            "flatpak-spawn".into(),
            "--host".into(),
            "sh".into(),
            "-c".into(),
            format!("{WAIT}; exec flatpak run io.github.namannn04.Veronica"),
        ]
    } else {
        let program = if kind == "snap" {
            Some(SNAP_LAUNCHER.to_string())
        } else {
            std::env::var("APPIMAGE").ok().or_else(|| {
                current_exe.map(|path| {
                    let shown = path.display().to_string();
                    shown.trim_end_matches(" (deleted)").to_string()
                })
            })
        };
        let program = program.unwrap_or_else(|| "veronica".into());
        vec![
            "setsid".into(),
            "sh".into(),
            "-c".into(),
            format!("{WAIT}; exec \"$0\""),
            program,
        ]
    }
}

#[tauri::command]
pub(crate) async fn system_snapshot(app: AppHandle) -> CommandResult<SystemSnapshot> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut sampler = match state.sampler.try_lock() {
            Ok(sampler) => sampler,
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err("the previous reading is still in progress".to_string())
            }
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };
        Ok(sampler.sample())
    })
    .await
    .map_err(|error| format!("cannot read the system: {error}"))?
}

#[tauri::command]
pub(crate) async fn system_processes(
) -> CommandResult<Vec<veronica_system::metrics::RunningProcess>> {
    tauri::async_runtime::spawn_blocking(veronica_system::metrics::running_processes)
        .await
        .map_err(|error| format!("cannot read running processes: {error}"))
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
pub(crate) async fn audit_site(
    site: String,
    limit: usize,
    concurrency: usize,
) -> CommandResult<veronica_audit::Report> {
    veronica_audit::audit(&site, limit, concurrency)
        .await
        .map_err(fail)
}

#[tauri::command]
pub(crate) async fn packages_sources() -> Vec<PackageSource> {
    let mut sources = Vec::new();
    for source in veronica_system::packages::Source::ALL {
        sources.push(PackageSource {
            available: veronica_system::packages::available(source).await,
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
pub(crate) fn cleaner_categories() -> &'static [veronica_core::CleanerCategory] {
    veronica_core::cleaner::CATEGORIES
}

#[tauri::command]
pub(crate) async fn cleaner_scan(
    categories: Vec<String>,
) -> CommandResult<veronica_core::CleanerScan> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = veronica_core::paths::home_dir()
            .ok_or_else(|| "cannot resolve the home directory".to_string())?;
        let selected = if categories.is_empty() {
            None
        } else {
            Some(categories)
        };
        Ok(veronica_core::cleaner::scan_caches(
            &home,
            selected.as_deref(),
        ))
    })
    .await
    .map_err(|error| format!("the scan could not run: {error}"))?
}

#[tauri::command]
pub(crate) async fn cleaner_clean(
    items: Vec<veronica_core::cleaner::Item>,
) -> CommandResult<veronica_core::cleaner::CleanReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = veronica_core::paths::home_dir()
            .ok_or_else(|| "cannot resolve the home directory".to_string())?;
        let mut report = veronica_core::cleaner::CleanReport::default();
        for item in items {
            let target = std::path::Path::new(&item.path);
            if !target.starts_with(&home) {
                report
                    .failed
                    .push((item.path, "outside the home directory".to_string()));
                continue;
            }
            let bytes = veronica_core::cleaner::directory_size(target);
            match veronica_core::cleaner::trash(&home, target) {
                Ok(_) => {
                    report.bytes_reclaimed = report.bytes_reclaimed.saturating_add(bytes);
                    report
                        .trashed
                        .push(veronica_core::cleaner::Item { bytes, ..item });
                }
                Err(error) => report.failed.push((item.path, format!("{error:#}"))),
            }
        }
        Ok(report)
    })
    .await
    .map_err(|error| format!("the clean could not run: {error}"))?
}

#[tauri::command]
pub(crate) fn system_quit_process(pid: u32) -> CommandResult<()> {
    veronica_system::metrics::terminate_process(pid).map_err(fail)
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
pub(crate) async fn herdr_board() -> CommandResult<veronica_system::herdr::HerdrBoard> {
    veronica_system::herdr::board().await.map_err(fail)
}

#[tauri::command]
pub(crate) async fn quinjet_projects() -> CommandResult<QuinjetBoard> {
    tauri::async_runtime::spawn_blocking(|| {
        if !veronica_system::quinjet::installed() {
            return Ok(QuinjetBoard {
                installed: false,
                projects: Vec::new(),
            });
        }
        Ok(QuinjetBoard {
            installed: true,
            projects: veronica_system::quinjet::projects().map_err(fail)?,
        })
    })
    .await
    .map_err(|error| format!("cannot read Quinjet: {error}"))?
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuinjetBoard {
    pub installed: bool,
    pub projects: Vec<veronica_system::quinjet::Project>,
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn quinjet_open(state: State<'_, AppState>, worktree: String) -> CommandResult<()> {
    let settings = state.settings_snapshot();
    let appearance =
        veronica_core::appearance::scheme(settings.string("appearance").unwrap_or("system"))
            .name()
            .map(str::to_string);
    veronica_system::quinjet::open_terminal(
        &worktree,
        &veronica_system::quinjet::LaunchOptions {
            theme: settings.string("quinjetTheme").map(str::to_string),
            appearance,
        },
    )
    .map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn herdr_open(session: String, pane_id: Option<String>) -> CommandResult<()> {
    veronica_system::herdr::open_terminal(&session, pane_id.as_deref()).map_err(fail)
}

#[tauri::command]
pub(crate) async fn audio_streams() -> CommandResult<Vec<veronica_system::audio::AudioStream>> {
    veronica_system::audio::streams().await.map_err(fail)
}

#[tauri::command]
pub(crate) async fn audio_stream_volume(id: u32, volume: f32) -> CommandResult<()> {
    veronica_system::audio::set_stream_volume(id, volume)
        .await
        .map_err(fail)
}

#[tauri::command]
pub(crate) async fn audio_stream_toggle_mute(id: u32) -> CommandResult<bool> {
    veronica_system::audio::toggle_stream_muted(id)
        .await
        .map_err(fail)
}

#[tauri::command]
pub(crate) async fn bluetooth_state() -> veronica_system::BluetoothState {
    veronica_system::bluetooth::state().await
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn emoji_groups(state: State<'_, AppState>) -> Vec<veronica_core::emoji::Group> {
    state.emoji.groups.clone()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn emoji_search(
    state: State<'_, AppState>,
    query: String,
    group: Option<usize>,
    limit: usize,
) -> Vec<EmojiRow> {
    let tone = state.emoji_tone();
    state
        .emoji
        .search(&query, usize::MAX)
        .into_iter()
        .filter(|emoji| group.is_none_or(|index| emoji.group_index == index))
        .take(limit)
        .map(|emoji| EmojiRow::new(emoji, tone))
        .collect()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn emoji_recents(
    state: State<'_, AppState>,
    limit: usize,
) -> CommandResult<Vec<EmojiRow>> {
    let ledger = veronica_core::emoji::UsageLedger::load(&state.directories.emoji_usage_file())
        .map_err(fail)?;
    let tone = state.emoji_tone();
    Ok(ledger
        .ranked(chrono::Utc::now().timestamp_millis(), limit)
        .into_iter()
        .filter_map(|character| {
            state
                .emoji
                .find(&character)
                .map(|emoji| EmojiRow::new(emoji, tone))
        })
        .collect())
}

#[tauri::command]
pub(crate) async fn emoji_copy(
    app: AppHandle,
    window: tauri::WebviewWindow,
    character: String,
    insert: bool,
) -> CommandResult<EmojiCopyResult> {
    let (path, base) = {
        let state = app.state::<AppState>();
        let base = state
            .emoji
            .emoji()
            .iter()
            .find(|emoji| emoji.character(state.emoji_tone()) == character)
            .map_or_else(|| character.clone(), |emoji| emoji.character.clone());
        (state.directories.emoji_usage_file(), base)
    };

    let mut ledger = veronica_core::emoji::UsageLedger::load(&path).map_err(fail)?;
    ledger.record(&base, chrono::Utc::now().timestamp_millis());
    ledger.save(&path).map_err(fail)?;

    let copied_via = veronica_system::selection::write(&character)
        .await
        .map(|writer| writer.title().to_string())
        .map_err(fail)?;

    let inserted = if insert {
        let result = if uses_emoji_target(window.label(), insert) {
            veronica_system::selection::paste_from_picker().await
        } else {
            veronica_system::selection::paste_in_place().await
        };
        match result {
            Ok(()) => true,
            Err(error) => {
                tracing::info!(target: "veronica", "cannot insert the emoji in place: {error:#}");
                false
            }
        }
    } else {
        false
    };

    Ok(EmojiCopyResult {
        character,
        copied_via,
        inserted,
    })
}

fn uses_emoji_target(window_label: &str, insert: bool) -> bool {
    insert && window_label == crate::emoji_picker::LABEL
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmojiRow {
    pub character: String,
    pub name: String,
    pub group_index: usize,
    pub supports_skin_tones: bool,
}

impl EmojiRow {
    fn new(emoji: &veronica_core::Emoji, tone: veronica_core::SkinTone) -> Self {
        Self {
            character: emoji.character(tone).to_string(),
            name: emoji.name.clone(),
            group_index: emoji.group_index,
            supports_skin_tones: emoji.supports_skin_tones(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmojiCopyResult {
    pub character: String,
    pub copied_via: String,
    pub inserted: bool,
}

#[tauri::command]
pub(crate) async fn microphone_state() -> CommandResult<veronica_system::audio::VolumeState> {
    veronica_system::audio::microphone().await.map_err(fail)
}

#[tauri::command]
pub(crate) async fn microphone_toggle() -> CommandResult<veronica_system::audio::VolumeState> {
    veronica_system::audio::toggle_microphone()
        .await
        .map_err(fail)
}

#[tauri::command]
pub(crate) async fn media_now_playing() -> CommandResult<Option<veronica_media::NowPlaying>> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|e| format!("cannot reach the session bus: {e}"))?;
    let mut playing = veronica_media::now_playing(&connection)
        .await
        .map_err(fail)?;

    if let Some(playing) = playing.as_mut() {
        playing.art_url = playing.art_url.as_deref().and_then(crate::art::to_data_url);
    }
    Ok(playing)
}

#[tauri::command]
pub(crate) async fn media_control(action: String) -> CommandResult<()> {
    use veronica_media::Transport;
    let transport = match action.as_str() {
        "play" => Transport::Play,
        "pause" => Transport::Pause,
        "toggle" => Transport::PlayPause,
        "next" => Transport::Next,
        "previous" => Transport::Previous,
        "stop" => Transport::Stop,
        other => return Err(format!("unknown media action {other:?}")),
    };
    let connection = zbus::Connection::session()
        .await
        .map_err(|e| format!("cannot reach the session bus: {e}"))?;
    veronica_media::control(&connection, transport)
        .await
        .map_err(fail)
}

#[tauri::command]
pub(crate) fn music_library() -> CommandResult<Vec<veronica_media::LocalTrack>> {
    let home = veronica_core::paths::home_dir()
        .ok_or_else(|| "cannot resolve home directory".to_string())?;
    let root = home.join("Music");
    if !root.exists() {
        return Ok(Vec::new());
    }
    veronica_media::scan_library(&root).map_err(fail)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgendaView {
    pub has_calendars: bool,
    pub days: Vec<veronica_calendar::AgendaDay>,
    pub next_up: Option<veronica_calendar::Event>,
    pub happening_now: Option<veronica_calendar::Event>,
}

#[tauri::command]
pub(crate) async fn calendar_agenda(days: i64, with_links: bool) -> CommandResult<AgendaView> {
    use veronica_calendar::{agenda, server};

    let connection = zbus::Connection::session()
        .await
        .map_err(|e| format!("cannot reach the session bus: {e}"))?;

    let has_calendars = server::has_calendars(&connection).await.unwrap_or(false);
    let events = if with_links {
        server::events_with_links(&connection, days).await
    } else {
        server::events(&connection, days).await
    }
    .map_err(fail)?;

    let now = chrono::Local::now();
    let remaining = agenda::upcoming(&events, now);

    Ok(AgendaView {
        has_calendars,
        next_up: agenda::next_up(&remaining, now).cloned(),
        happening_now: agenda::happening_now(&remaining, now).cloned(),
        days: agenda::group_by_day(&remaining, now),
    })
}

#[tauri::command]
pub(crate) async fn calendar_open() -> CommandResult<()> {
    let output = tauri::async_runtime::spawn_blocking(|| {
        veronica_core::host::command("gtk-launch")
            .arg("org.gnome.Calendar")
            .stdin(std::process::Stdio::null())
            .output()
    })
    .await
    .map_err(|error| format!("cannot open GNOME Calendar: {error}"))?
    .map_err(|error| format!("cannot open GNOME Calendar: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if detail.contains("no such application") {
        "GNOME Calendar is not installed. Install it from the App Center, or add an account in Settings › Online Accounts to see events here.".to_string()
    } else {
        format!("cannot open GNOME Calendar: {detail}")
    })
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
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn clipboard_list(
    state: State<'_, AppState>,
    query: String,
) -> CommandResult<Vec<ClipRow>> {
    use veronica_core::ClipboardHistory;

    let history = ClipboardHistory::load(&state.directories.clipboard_db()).map_err(fail)?;
    Ok(history
        .search(&query)
        .into_iter()
        .map(|entry| ClipRow {
            id: entry.id,
            preview: entry.preview(),
            text: entry.text.clone(),
            lines: entry.line_count(),
            bytes: entry.byte_len(),
            count: entry.count,
            last_seen: entry.last_seen.to_rfc3339(),
        })
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClipRow {
    pub id: u64,
    pub preview: String,
    pub text: String,
    pub lines: usize,
    pub bytes: usize,
    pub count: u32,
    pub last_seen: String,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AlertsView {
    pub settings: veronica_usage::alerts::NotifySettings,
    pub poll_seconds: u64,
    pub session: Option<WatchedWindow>,
    pub week: Option<WatchedWindow>,
    pub note: Option<String>,
    pub session_reminder_at: Option<String>,
    pub week_reminder_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatchedWindow {
    pub percent: f64,
    pub resets_at: Option<String>,
    pub resets_in: Option<String>,
    pub level: veronica_usage::limits::UsageLevel,
    pub zone: veronica_usage::limits::PacingZone,
}

#[tauri::command]
pub(crate) async fn alerts_view(app: AppHandle) -> CommandResult<AlertsView> {
    use veronica_usage::alerts::{self, NotifySettings};
    use veronica_usage::limits::{
        level_for_risk, pacing_delta, pacing_zone, smart_risk, LimitWindow, LimitWindowKind,
    };

    let settings = app.state::<AppState>().settings_snapshot();
    let notify = NotifySettings::from_settings(&settings);
    let poll_seconds = crate::alerts::poll_interval(&settings).as_secs();
    let now = chrono::Utc::now();

    let describe = |window: LimitWindow, kind: LimitWindowKind| WatchedWindow {
        percent: window.percent,
        resets_at: window.resets_at.map(|at| at.to_rfc3339()),
        resets_in: window
            .resets_at
            .filter(|at| *at > now)
            .map(|at| alerts::countdown(now, at)),
        level: if notify.smart_color {
            level_for_risk(smart_risk(
                window.percent,
                window.resets_at,
                kind.duration_secs(),
                notify.pacing_margin,
                now,
            ))
        } else {
            veronica_usage::limits::UsageLevel::from_percent(window.percent, notify.thresholds)
        },
        zone: window
            .resets_at
            .map_or(veronica_usage::limits::PacingZone::OnTrack, |at| {
                pacing_zone(
                    pacing_delta(window.percent, at, kind.duration_secs(), now),
                    notify.pacing_margin,
                )
            }),
    };

    let limits = if notify.master {
        veronica_usage::claude::limits_for_user(now).await
    } else {
        return Ok(AlertsView {
            poll_seconds,
            session: None,
            week: None,
            session_reminder_at: None,
            week_reminder_at: None,
            note: None,
            settings: notify,
        });
    };
    let (session, week, note) = match limits {
        Ok(Some(limits)) => (limits.session, limits.week, None),
        Ok(None) => (
            None,
            None,
            Some("Claude is not signed in on this computer, so there is nothing to watch.".into()),
        ),
        Err(error) => (None, None, Some(format!("{error:#}"))),
    };

    Ok(AlertsView {
        poll_seconds,
        session: session.map(|window| describe(window, LimitWindowKind::Session)),
        week: week.map(|window| describe(window, LimitWindowKind::Weekly)),
        session_reminder_at: alerts::reminder_fire_at(
            session.and_then(|window| window.resets_at),
            notify.reminder_session_offset_min,
            now,
        )
        .filter(|_| notify.reminder_session)
        .map(|at| at.to_rfc3339()),
        week_reminder_at: alerts::reminder_fire_at(
            week.and_then(|window| window.resets_at),
            notify.reminder_weekly_offset_min,
            now,
        )
        .filter(|_| notify.reminder_weekly)
        .map(|at| at.to_rfc3339()),
        note,
        settings: notify,
    })
}

#[tauri::command]
pub(crate) async fn alerts_test() -> CommandResult<String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("no session bus, so no notifications: {error}"))?;
    match crate::alerts::send_test(&connection).await {
        Ok(_) => Ok("Sent. Check your notification list if no banner appeared.".to_string()),
        Err(error) => Err(fail(error)),
    }
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn alerts_reset(state: State<'_, AppState>) -> CommandResult<()> {
    let path = state.directories.alerts_state_file();
    let mut notifier = veronica_usage::alerts::NotifierState::load(&path);
    notifier.reset_tracking();
    notifier.save(&path).map_err(fail)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SwatchRow {
    pub id: u64,
    pub hex: String,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub profile: String,
    pub profile_label: &'static str,
    pub picked_at: String,
    pub formats: std::collections::BTreeMap<&'static str, String>,
    pub prefers_dark_text: bool,
}

fn swatch_row(swatch: &veronica_core::Swatch) -> SwatchRow {
    use veronica_core::CopyFormat;
    SwatchRow {
        id: swatch.id,
        hex: swatch.hex(),
        red: swatch.red,
        green: swatch.green,
        blue: swatch.blue,
        profile: swatch.profile.key().to_string(),
        profile_label: swatch.profile.title(),
        picked_at: swatch.picked_at.to_rfc3339(),
        formats: CopyFormat::ALL
            .iter()
            .map(|format| (format.key(), swatch.format(*format)))
            .collect(),
        prefers_dark_text: swatch.prefers_dark_text(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FormatOption {
    pub id: &'static str,
    pub label: &'static str,
}

#[tauri::command]
pub(crate) fn color_formats() -> Vec<FormatOption> {
    veronica_core::CopyFormat::ALL
        .iter()
        .map(|format| FormatOption {
            id: format.key(),
            label: format.title(),
        })
        .collect()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn color_swatches(state: State<'_, AppState>) -> CommandResult<Vec<SwatchRow>> {
    let history =
        veronica_core::SwatchHistory::load(&state.directories.swatches_file()).map_err(fail)?;
    Ok(history.swatches().iter().map(swatch_row).collect())
}

#[tauri::command]
pub(crate) async fn color_pick(app: AppHandle) -> CommandResult<PickResult> {
    use veronica_core::swatches::{srgb_to_display_p3, ColorProfile, CopyFormat, SwatchHistory};

    let (path, format, profile, limit) = {
        let state = app.state::<AppState>();
        let settings = state.settings_snapshot();
        (
            state.directories.swatches_file(),
            CopyFormat::parse(settings.string("colorPickerCopyFormat").unwrap_or("hex")),
            ColorProfile::parse(settings.string("colorPickerProfile").unwrap_or("srgb")),
            SwatchHistory::clamp_limit(
                settings
                    .get("colorPickerHistorySize")
                    .and_then(serde_json::Value::as_u64)
                    .map_or(veronica_core::swatches::DEFAULT_HISTORY_SIZE, |size| {
                        usize::try_from(size).unwrap_or(usize::MAX)
                    }),
            ),
        )
    };

    let picked = veronica_system::color::pick().await.map_err(fail)?;
    let (red, green, blue) = match profile {
        ColorProfile::Srgb => (picked.red, picked.green, picked.blue),
        ColorProfile::DisplayP3 => srgb_to_display_p3(picked.red, picked.green, picked.blue),
    };

    let mut history = SwatchHistory::load(&path).map_err(fail)?;
    let swatch = history.record(red, green, blue, profile, chrono::Utc::now(), limit);
    history.save(&path).map_err(fail)?;

    let value = swatch.format(format);
    let (copied_via, copy_error) = match veronica_system::selection::write(&value).await {
        Ok(writer) => (Some(writer.title()), None),
        Err(error) => (None, Some(format!("{error:#}"))),
    };

    let _ = app.emit("swatches-updated", swatch.id);
    Ok(PickResult {
        swatch: swatch_row(&swatch),
        value,
        format: format.key(),
        source: picked.source.title(),
        copied_via,
        copy_error,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PickResult {
    pub swatch: SwatchRow,
    pub value: String,
    pub format: &'static str,
    pub source: &'static str,
    pub copied_via: Option<&'static str>,
    pub copy_error: Option<String>,
}

#[tauri::command]
pub(crate) async fn color_copy(
    app: AppHandle,
    id: u64,
    format: Option<String>,
) -> CommandResult<CopyResult> {
    use veronica_core::CopyFormat;

    let (path, configured) = {
        let state = app.state::<AppState>();
        let settings = state.settings_snapshot();
        (
            state.directories.swatches_file(),
            CopyFormat::parse(settings.string("colorPickerCopyFormat").unwrap_or("hex")),
        )
    };
    let chosen = format.as_deref().map_or(configured, CopyFormat::parse);

    let history = veronica_core::SwatchHistory::load(&path).map_err(fail)?;
    let swatch = history.get(id).ok_or_else(|| format!("no swatch {id}"))?;
    let value = swatch.format(chosen);
    let writer = veronica_system::selection::write(&value)
        .await
        .map_err(fail)?;
    Ok(CopyResult {
        value,
        format: chosen.key(),
        copied_via: writer.title(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CopyResult {
    pub value: String,
    pub format: &'static str,
    pub copied_via: &'static str,
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn color_forget(state: State<'_, AppState>, id: u64) -> CommandResult<()> {
    let path = state.directories.swatches_file();
    let mut history = veronica_core::SwatchHistory::load(&path).map_err(fail)?;
    if !history.remove(id) {
        return Err(format!("no swatch {id}"));
    }
    history.save(&path).map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn color_clear(state: State<'_, AppState>) -> CommandResult<()> {
    let path = state.directories.swatches_file();
    let mut history = veronica_core::SwatchHistory::load(&path).map_err(fail)?;
    history.clear();
    history.save(&path).map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn clipboard_remove(state: State<'_, AppState>, id: u64) -> CommandResult<()> {
    use veronica_core::ClipboardHistory;

    let path = state.directories.clipboard_db();
    let mut history = ClipboardHistory::load(&path).map_err(fail)?;
    if !history.remove(id) {
        return Err(format!("no clipboard entry {id}"));
    }
    history.save(&path).map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn clipboard_clear(state: State<'_, AppState>) -> CommandResult<()> {
    use veronica_core::ClipboardHistory;

    let path = state.directories.clipboard_db();
    let mut history = ClipboardHistory::load(&path).map_err(fail)?;
    history.clear();
    history.save(&path).map_err(fail)
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

    use super::{newer_version, parsed_shell_extension_status, with_veronica_enabled};

    #[test]
    fn each_installation_is_offered_its_own_package() {
        let assets: Vec<super::GithubAsset> = [
            "SHA256SUMS",
            "Veronica_0.1.15_amd64.AppImage",
            "Veronica_0.1.15_amd64.deb",
            "Veronica_0.1.15_amd64.snap",
            "Veronica_0.1.15_x86_64.flatpak",
        ]
        .into_iter()
        .map(|name| super::GithubAsset {
            name: name.into(),
            browser_download_url: format!("https://example.test/{name}"),
        })
        .collect();
        let pick = |kind| super::package_asset(&assets, kind).map(|asset| asset.name.as_str());
        assert_eq!(pick("deb"), Some("Veronica_0.1.15_amd64.deb"));
        assert_eq!(pick("appimage"), Some("Veronica_0.1.15_amd64.AppImage"));
        assert_eq!(pick("flatpak"), Some("Veronica_0.1.15_x86_64.flatpak"));
        assert_eq!(pick("snap"), Some("Veronica_0.1.15_amd64.snap"));
    }

    #[test]
    fn a_snap_restarts_through_its_stable_launcher() {
        let command =
            super::restart_command("snap", Some("/snap/veronica/12/usr/bin/veronica".into()));
        assert_eq!(
            command.last().map(String::as_str),
            Some("/snap/bin/veronica")
        );
    }

    #[test]
    fn a_restart_starts_the_replaced_binary_without_background() {
        let command = super::restart_command(
            "deb",
            Some(std::path::PathBuf::from("/usr/bin/veronica (deleted)")),
        );
        assert_eq!(command[0], "setsid");
        assert_eq!(command.last().unwrap(), "/usr/bin/veronica");
        assert!(!command.iter().any(|part| part.contains("--background")));

        let flatpak = super::restart_command("flatpak", None);
        assert_eq!(&flatpak[..2], ["flatpak-spawn", "--host"]);
        assert!(flatpak
            .last()
            .unwrap()
            .ends_with("flatpak run io.github.namannn04.Veronica"));
    }

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
    fn update_versions_compare_numerically_and_ignore_release_prefixes() {
        assert!(newer_version("v0.2.0", "0.1.99"));
        assert!(newer_version("1.0.1", "1.0.0"));
        assert!(!newer_version("0.1.8", "0.1.8"));
        assert!(!newer_version("0.1.7", "0.1.8"));
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

    #[test]
    fn compact_picker_uses_the_shell_target_without_hiding() {
        assert!(uses_emoji_target(crate::emoji_picker::LABEL, true));
        assert!(!uses_emoji_target("main", true));
        assert!(!uses_emoji_target(crate::emoji_picker::LABEL, false));
    }

    fn keys(value: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
        if let serde_json::Value::Object(map) = value {
            for (key, nested) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                out.push(path.clone());
                keys(nested, &path, out);
            }
        }
    }

    fn assert_camel_case<T: Serialize>(value: &T, label: &str) {
        let json = serde_json::to_value(value).expect("serialisable");
        let mut found = Vec::new();
        keys(&json, "", &mut found);
        assert!(!found.is_empty(), "{label} serialised to nothing");
        for path in &found {
            let leaf = path.rsplit('.').next().unwrap();
            assert!(
                !leaf.contains('_'),
                "{label} exposes snake_case at {path}; the interface reads camelCase"
            );
        }
    }

    #[test]
    fn the_swatch_row_is_camel_case_throughout() {
        let swatch = veronica_core::Swatch {
            id: 1,
            red: 0.2,
            green: 0.4,
            blue: 0.6,
            profile: veronica_core::ColorProfile::DisplayP3,
            picked_at: chrono::Utc::now(),
        };
        let row = swatch_row(&swatch);
        assert_camel_case(&row, "SwatchRow");

        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(json["hex"], "#336699");
        assert!(json["prefersDarkText"].is_boolean());
        assert!(json["profileLabel"].is_string());
        assert!(json["pickedAt"].is_string());
        assert!(json["formats"]["gdkRgba"].is_string());
    }

    #[test]
    fn the_pick_result_is_camel_case_throughout() {
        let result = PickResult {
            swatch: swatch_row(&veronica_core::Swatch {
                id: 1,
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                profile: veronica_core::ColorProfile::Srgb,
                picked_at: chrono::Utc::now(),
            }),
            value: "#000000".into(),
            format: "hex",
            source: "GNOME Shell",
            copied_via: Some("wl-copy"),
            copy_error: None,
        };
        assert_camel_case(&result, "PickResult");
        let json = serde_json::to_value(&result).unwrap();
        assert!(json["copiedVia"].is_string());
        assert!(json["copyError"].is_null());
        assert!(json["swatch"]["prefersDarkText"].is_boolean());
    }

    #[test]
    fn the_watched_window_is_camel_case_throughout() {
        let window = WatchedWindow {
            percent: 81.0,
            resets_at: Some("2026-08-27T18:00:00Z".into()),
            resets_in: Some("2 h 14 min".into()),
            level: veronica_usage::limits::UsageLevel::Orange,
            zone: veronica_usage::limits::PacingZone::OnTrack,
        };
        assert_camel_case(&window, "WatchedWindow");
        let json = serde_json::to_value(&window).unwrap();
        assert_eq!(json["resetsIn"], "2 h 14 min");
        assert_eq!(json["level"], "orange");
        assert_eq!(json["zone"], "onTrack");
    }

    #[test]
    fn the_alerts_view_is_camel_case_including_its_nested_settings() {
        let view = AlertsView {
            settings: veronica_usage::alerts::NotifySettings::default(),
            poll_seconds: 60,
            session: None,
            week: None,
            note: None,
            session_reminder_at: None,
            week_reminder_at: None,
        };
        assert_camel_case(&view, "AlertsView");
        let json = serde_json::to_value(&view).unwrap();
        assert!(json["pollSeconds"].is_number());
        assert!(json["settings"]["thresholds"]["warningPercent"].is_number());
    }

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
pub(crate) async fn ports_snapshot() -> CommandResult<veronica_system::ports::PortsSnapshot> {
    veronica_system::ports::snapshot().await.map_err(fail)
}
#[tauri::command]
pub(crate) async fn ports_stop(
    request: veronica_system::ports::StopRequest,
) -> CommandResult<String> {
    veronica_system::ports::stop(&request).await.map_err(fail)
}
#[tauri::command]
pub(crate) async fn docker_snapshot() -> veronica_system::docker::DockerSnapshot {
    veronica_system::docker::snapshot().await
}
#[tauri::command]
pub(crate) async fn docker_action(
    endpoint: String,
    id: String,
    action: String,
) -> CommandResult<()> {
    veronica_system::docker::action(&endpoint, &id, &action)
        .await
        .map_err(fail)
}

#[tauri::command]
pub(crate) async fn workspace_doctor(
    request: veronica_system::workspace::DoctorRequest,
) -> CommandResult<veronica_system::workspace::DoctorReport> {
    veronica_system::workspace::doctor(request)
        .await
        .map_err(fail)
}

#[tauri::command]
pub(crate) async fn graft_discover(
    connection: veronica_system::graftcode::Connection,
) -> CommandResult<veronica_system::graftcode::Discovery> {
    veronica_system::graftcode::discover(connection)
        .await
        .map_err(fail)
}
#[tauri::command]
pub(crate) async fn graft_call(
    connection: veronica_system::graftcode::Connection,
    name: String,
    arguments: serde_json::Value,
    confirmed: bool,
) -> CommandResult<serde_json::Value> {
    veronica_system::graftcode::call(connection, name, arguments, confirmed)
        .await
        .map_err(fail)
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

#[tauri::command]
pub(crate) async fn chat_view(
    app: AppHandle,
) -> CommandResult<veronica_system::companion_ai::ChatView> {
    app.state::<AppState>().chat.view().map_err(fail)
}
#[tauri::command]
pub(crate) async fn chat_configure(
    app: AppHandle,
    config: veronica_system::companion_ai::ModelConfig,
    memory: String,
) -> CommandResult<()> {
    app.state::<AppState>()
        .chat
        .configure(config, memory)
        .map_err(fail)
}
#[tauri::command]
pub(crate) async fn chat_enable(
    app: AppHandle,
    enabled: bool,
) -> CommandResult<veronica_system::companion_ai::ChatView> {
    app.state::<AppState>().chat.enable(enabled);
    app.state::<AppState>().chat.view().map_err(fail)
}
#[tauri::command]
pub(crate) async fn chat_send(
    app: AppHandle,
    text: String,
    key: String,
    include_history: bool,
    include_memory: bool,
) -> CommandResult<veronica_system::companion_ai::Message> {
    app.state::<AppState>()
        .chat
        .send(text, key, include_history, include_memory)
        .await
        .map_err(fail)
}
#[tauri::command]
pub(crate) async fn chat_preview(
    app: AppHandle,
    text: String,
    include_history: bool,
    include_memory: bool,
) -> CommandResult<Vec<serde_json::Value>> {
    app.state::<AppState>()
        .chat
        .preview(&text, include_history, include_memory)
        .map_err(fail)
}
#[tauri::command]
pub(crate) async fn chat_models(
    app: AppHandle,
    config: veronica_system::companion_ai::ModelConfig,
) -> CommandResult<Vec<String>> {
    app.state::<AppState>()
        .chat
        .models(config)
        .await
        .map_err(fail)
}
#[tauri::command]
pub(crate) async fn chat_pull(
    app: AppHandle,
    config: veronica_system::companion_ai::ModelConfig,
    confirmed: bool,
) -> CommandResult<()> {
    app.state::<AppState>()
        .chat
        .pull(config, confirmed)
        .await
        .map_err(fail)
}
#[tauri::command]
pub(crate) async fn chat_clear(app: AppHandle, memory: bool) -> CommandResult<()> {
    app.state::<AppState>().chat.clear(memory).map_err(fail)
}
#[tauri::command]
pub(crate) async fn chat_export(app: AppHandle) -> CommandResult<String> {
    app.state::<AppState>().chat.export().map_err(fail)
}

#[tauri::command]
pub(crate) async fn chat_journal(
    app: AppHandle,
    text: String,
) -> CommandResult<veronica_system::companion_ai::Message> {
    app.state::<AppState>().chat.journal(text).map_err(fail)
}

#[tauri::command]
pub(crate) async fn agent_herdr_attach(
    app: AppHandle,
    session: String,
    pane: String,
    confirmed: bool,
) -> CommandResult<veronica_system::agents::AgentView> {
    if !confirmed {
        return Err("Confirm taking over the existing Herdr frontend".into());
    }
    if session.len() > 128 || pane.len() > 128 {
        return Err("Invalid Herdr session/pane".into());
    }
    let epoch = app.state::<AppState>().agents.epoch();
    let board = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        veronica_system::herdr::board(),
    )
    .await
    .map_err(|_| "Herdr did not respond within eight seconds".to_owned())?
    .map_err(fail)?;
    let agent = board
        .agents
        .iter()
        .find(|a| a.session == session && a.pane_id == pane)
        .ok_or("This Herdr agent is no longer available")?;
    let cwd = agent.cwd.clone();
    let executable = veronica_system::herdr::executable().ok_or("Herdr is not installed")?;
    let profile = veronica_system::agents::AgentProfile {
        id: "herdr-attach".into(),
        name: "Herdr attached agent".into(),
        executable: executable.display().to_string(),
        arguments: veronica_system::herdr::attach_args(&session, Some(&pane)),
    };
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .agents
            .start(profile, cwd, epoch)
            .map_err(fail)
    })
    .await
    .map_err(|e| e.to_string())?
}
