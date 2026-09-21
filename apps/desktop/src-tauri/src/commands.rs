//! Commands the interface calls.
//!
//! Each returns plain serialisable data. Errors become strings, because a Tauri
//! command's error type must serialise and `anyhow::Error` does not; the
//! `{:#}` format keeps the whole context chain so the UI can show a real reason
//! rather than "failed".

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use veronica_core::{Capabilities, Diagnostics};
use veronica_system::metrics::SystemSnapshot;
use veronica_usage::aggregate::{self, DayRange, SourceSelection};
use veronica_usage::collector;

use crate::state::AppState;

/// Errors cross the IPC boundary as messages.
type CommandResult<T> = Result<T, String>;

fn fail(error: anyhow::Error) -> String {
    format!("{error:#}")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageView {
    /// None until the collector has run at least once.
    pub dashboard: Option<aggregate::Dashboard>,
    pub generated_at: Option<String>,
    /// Every source the collector found, with its display label.
    pub sources: Vec<SourceOption>,
    /// Every model name in the whole document, sorted.
    ///
    /// Colour must follow the entity, not its rank: without a stable order, a
    /// source filter that drops one model would repaint all the others.
    pub models: Vec<String>,
    pub has_data: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceOption {
    pub id: String,
    pub label: String,
}

#[tauri::command]
pub fn diagnostics(state: State<'_, AppState>) -> CommandResult<Diagnostics> {
    let session = state.session.lock().expect("session lock").clone();
    let settings = state.settings_snapshot();
    Ok(Diagnostics::collect(&state.directories, session, &settings))
}

#[tauri::command]
pub fn capabilities(state: State<'_, AppState>) -> CommandResult<Capabilities> {
    let session = state.session.lock().expect("session lock").clone();
    Ok(Capabilities::resolve(&session))
}

/// The dashboard for a window and source selection.
///
/// `days` of `None` means the full history; an empty `sources` list means every
/// source the collector marked as default, which is what the UI sends before
/// the user has narrowed anything.
#[tauri::command]
pub fn usage_view(
    state: State<'_, AppState>,
    days: Option<usize>,
    sources: Vec<String>,
) -> CommandResult<UsageView> {
    let guard = state.usage.lock().expect("usage lock");
    let Some(document) = guard.as_ref() else {
        return Ok(UsageView {
            dashboard: None,
            generated_at: None,
            sources: Vec::new(),
            models: Vec::new(),
            has_data: false,
        });
    };

    let range = match days {
        Some(days) => DayRange::last_days(document, days),
        None => DayRange::default(),
    };
    let selection = if sources.is_empty() {
        SourceSelection::All
    } else {
        SourceSelection::Only(sources)
    };

    // Collected across the entire document, not the filtered range, so a model
    // keeps its colour when the user narrows the window.
    let mut models: Vec<String> = document
        .daily
        .iter()
        .flat_map(|day| day.by_source.values())
        .flatten()
        .map(|row| row.model_name.clone())
        .collect();
    models.sort_unstable();
    models.dedup();

    Ok(UsageView {
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
    })
}

/// Run the collector, streaming each phase to the UI as a `usage://progress`
/// event so the refresh shows what it is doing rather than freezing.
/// Render usage as branded PNG cards and write them somewhere the user can
/// reach.
///
/// A card never shows a repository name, a folder path, a chat title or a
/// dollar cost — see `veronica_usage::cards`, where a test enforces it.
/// Rasterising four cards takes a moment, so it runs off the UI thread.
#[tauri::command]
pub async fn usage_export(
    state: State<'_, AppState>,
    cards: Vec<String>,
    days: Option<usize>,
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
            Some(days) => veronica_usage::DayRange::last_days(document, days),
            None => veronica_usage::DayRange::default(),
        };
        let label = match (range.start.as_deref(), range.end.as_deref()) {
            (Some(start), Some(end)) if start == end => start.to_string(),
            (Some(start), Some(end)) => format!("{start} to {end}"),
            _ => "all time".to_string(),
        };
        (
            veronica_usage::dashboard(document, &range, &veronica_usage::SourceSelection::All),
            label,
        )
    };
    if board.totals.tokens == 0 {
        return Err("No usage in this window, so there is nothing to put on a card.".to_string());
    }

    // Pictures belong with the user's other pictures, not in a config
    // directory they would have to be told about.
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
pub async fn usage_refresh(app: AppHandle) -> CommandResult<String> {
    {
        let state = app.state::<AppState>();
        let mut refreshing = state.refreshing.lock().expect("refresh lock");
        if *refreshing {
            // Two collectors writing the same output directory would corrupt it.
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
    // Copy the paths out so the state guard is not held across an await.
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
    // Tell every window the numbers changed, so the notch updates too.
    let _ = app.emit("usage-updated", &generated_at);
    Ok(generated_at)
}

/// Rate-limit gauges for every provider that can be read.
///
/// A slow call by nature: it reaches the provider over the network and may spawn
/// the codex server, so the interface treats it as a refresh rather than
/// something to poll tightly.
#[tauri::command]
pub async fn usage_limits() -> CommandResult<veronica_usage::GaugeReport> {
    Ok(veronica_usage::gauges::collect(
        chrono::Utc::now(),
        veronica_usage::gauges::DEFAULT_PACING_MARGIN,
    )
    .await)
}

#[tauri::command]
pub fn settings_all(state: State<'_, AppState>) -> CommandResult<serde_json::Value> {
    let settings = state.settings_snapshot();
    serde_json::to_value(settings.as_map()).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchAtLoginStatus {
    pub enabled: bool,
    pub entry_path: String,
}

fn autostart_executable() -> CommandResult<std::path::PathBuf> {
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
pub fn launch_at_login_status(state: State<'_, AppState>) -> CommandResult<LaunchAtLoginStatus> {
    launch_at_login_status_for(&state)
}

#[tauri::command]
pub fn launch_at_login_set(
    state: State<'_, AppState>,
    enabled: bool,
) -> CommandResult<LaunchAtLoginStatus> {
    let executable = autostart_executable()?;
    let path = state.directories.autostart_file();
    veronica_core::autostart::set_enabled(&path, &executable, enabled).map_err(fail)?;
    launch_at_login_status_for(&state)
}

const SHELL_EXTENSION_UUID: &str = "veronica@namannn04.github.io";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShellExtensionStatus {
    /// active, disabled, restartRequired, notInstalled, error or unsupported.
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
            detail: "GNOME loaded the extension, but it reported an error. Open Permissions for diagnostics.".into(),
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
    let output = std::process::Command::new("gnome-extensions")
        .args(["info", SHELL_EXTENSION_UUID])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            parsed_shell_extension_status(&String::from_utf8_lossy(&output.stdout))
        }
        Ok(_) if installed => ShellExtensionStatus {
            state: "restartRequired".into(),
            detail: "The extension was installed after this GNOME session started. Log out and back in once.".into(),
            can_enable: false,
        },
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
pub fn shell_extension_status(state: State<'_, AppState>) -> CommandResult<ShellExtensionStatus> {
    Ok(shell_extension_status_for(&state))
}

#[tauri::command]
pub fn shell_extension_enable(state: State<'_, AppState>) -> CommandResult<ShellExtensionStatus> {
    let output = std::process::Command::new("gnome-extensions")
        .args(["enable", SHELL_EXTENSION_UUID])
        .output()
        .map_err(|error| format!("cannot run gnome-extensions: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            "GNOME refused to enable the Veronica extension".into()
        } else {
            detail
        });
    }
    Ok(shell_extension_status_for(&state))
}

/// Run a compositor-owned quick action through the GNOME Shell extension.
/// Both the app and notch therefore use one implementation for modal input
/// suppression and color picking instead of pretending a WebView can do it.
#[tauri::command]
pub async fn shell_action(action: String) -> CommandResult<()> {
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

#[tauri::command]
pub async fn settings_set(
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

        // Acquire before persisting an enabled state, so the switch cannot say
        // on when logind refused the lock. Disable is the reverse: preserve the
        // existing lock if writing the setting fails.
        if enabled {
            sync_lid_awake(&app, true).await?;
        }
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

// -- backup -----------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
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

/// Export to Downloads by default, or to an explicit path supplied by the UI.
#[tauri::command]
pub fn backup_export(
    state: State<'_, AppState>,
    path: Option<String>,
) -> CommandResult<BackupSummary> {
    let now = chrono::Utc::now();
    let destination = match path.filter(|value| !value.trim().is_empty()) {
        Some(value) => std::path::PathBuf::from(value),
        None => {
            let home = veronica_core::paths::home_dir()
                .ok_or_else(|| "cannot resolve your home directory".to_string())?;
            home.join("Downloads").join(format!(
                "Veronica-backup-{}.veronica-backup",
                now.format("%Y-%m-%d-%H%M%S")
            ))
        }
    };
    let archive =
        veronica_core::BackupArchive::collect(&state.directories, env!("CARGO_PKG_VERSION"), now)
            .map_err(fail)?;
    archive.save(&destination).map_err(fail)?;
    Ok(backup_summary(&destination, &archive))
}

#[tauri::command]
pub fn backup_inspect(path: String) -> CommandResult<BackupSummary> {
    let source = std::path::PathBuf::from(path);
    let archive = veronica_core::BackupArchive::load(&source).map_err(fail)?;
    Ok(backup_summary(&source, &archive))
}

#[tauri::command]
pub fn backup_import(
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
pub fn companion_list(
    state: State<'_, AppState>,
    query: String,
) -> CommandResult<Vec<veronica_core::CompanionItem>> {
    companion_repository(&state).list(&query).map_err(fail)
}

#[tauri::command]
pub fn companion_note_create(
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
pub fn companion_update(
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
pub fn companion_remove(app: AppHandle, id: u64) -> CommandResult<()> {
    companion_repository(&app.state::<AppState>())
        .remove(id)
        .map_err(fail)?;
    let _ = app.emit("companion-updated", id);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionRecordingStatus {
    pub recording: bool,
    pub elapsed_seconds: u64,
}

#[tauri::command]
pub fn companion_recording_status(state: State<'_, AppState>) -> CompanionRecordingStatus {
    let recording = state
        .companion_recording
        .lock()
        .expect("companion recording lock");
    CompanionRecordingStatus {
        recording: recording.is_some(),
        elapsed_seconds: recording
            .as_ref()
            .map(|recording| recording.started.elapsed().as_secs())
            .unwrap_or(0),
    }
}

#[tauri::command]
pub fn companion_record_start(state: State<'_, AppState>) -> CommandResult<()> {
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
    let mut child = std::process::Command::new("pw-record")
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
pub fn companion_record_stop(app: AppHandle) -> CommandResult<veronica_core::CompanionItem> {
    let state = app.state::<AppState>();
    let mut recording = state
        .companion_recording
        .lock()
        .expect("companion recording lock")
        .take()
        .ok_or_else(|| "no voice memo is recording".to_string())?;
    stop_companion_process(&mut recording);
    let bytes = std::fs::metadata(&recording.path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if bytes <= 44 {
        let _ = std::fs::remove_file(&recording.path);
        return Err("the microphone produced no audio; check PipeWire input access".to_string());
    }
    let item = companion_repository(&state)
        .add_voice(
            &recording.path,
            recording.started.elapsed().as_secs().max(1),
            chrono::Utc::now(),
        )
        .map_err(fail)?;
    let _ = app.emit("companion-updated", item.id);
    Ok(item)
}

#[tauri::command]
pub fn companion_record_cancel(state: State<'_, AppState>) -> CommandResult<()> {
    let Some(mut recording) = state
        .companion_recording
        .lock()
        .expect("companion recording lock")
        .take()
    else {
        return Ok(());
    };
    stop_companion_process(&mut recording);
    let _ = std::fs::remove_file(recording.path);
    Ok(())
}

#[tauri::command]
pub fn companion_audio(state: State<'_, AppState>, id: u64) -> CommandResult<String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let path = companion_repository(&state).audio_path(id).map_err(fail)?;
    let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > 64 * 1024 * 1024 {
        return Err("voice memo is too large to play in the app".to_string());
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    Ok(format!("data:audio/wav;base64,{}", STANDARD.encode(bytes)))
}

#[tauri::command]
pub fn attention_status(
    state: State<'_, AppState>,
) -> CommandResult<veronica_core::AttentionStatus> {
    veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .status(chrono::Utc::now())
        .map_err(fail)
}

#[tauri::command]
pub fn attention_start(
    state: State<'_, AppState>,
    name: String,
    duration_seconds: i64,
) -> CommandResult<veronica_core::AttentionFocusSession> {
    veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .start_focus(&name, duration_seconds, chrono::Utc::now())
        .map_err(fail)
}

#[tauri::command]
pub fn attention_stop(
    state: State<'_, AppState>,
) -> CommandResult<veronica_core::AttentionFocusSession> {
    veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .stop_focus(chrono::Utc::now())
        .map_err(fail)
}

#[tauri::command]
pub fn attention_history(
    state: State<'_, AppState>,
) -> CommandResult<Vec<veronica_core::AttentionFocusSession>> {
    let mut sessions = veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .history()
        .map_err(fail)?;
    sessions.reverse();
    Ok(sessions)
}

#[tauri::command]
pub fn attention_overview(
    state: State<'_, AppState>,
    days: i64,
) -> CommandResult<veronica_core::AttentionOverview> {
    let now = chrono::Utc::now();
    veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .overview(now - chrono::Duration::days(days.clamp(1, 365)), now)
        .map_err(fail)
}

#[tauri::command]
pub fn attention_settings(
    state: State<'_, AppState>,
) -> CommandResult<veronica_core::AttentionSettings> {
    veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .load_settings()
        .map_err(fail)
}

#[tauri::command]
pub fn attention_settings_save(
    app: AppHandle,
    settings: veronica_core::AttentionSettings,
) -> CommandResult<()> {
    let state = app.state::<AppState>();
    veronica_core::AttentionRepository::new(state.directories.attention_dir())
        .save_settings(&settings)
        .map_err(fail)?;
    let mut shared = state.settings.lock().expect("settings mutex poisoned");
    shared.set("tabAttentionEnabled", serde_json::json!(settings.enabled));
    shared.set(
        "attentionPrivacy",
        serde_json::json!(match settings.privacy {
            veronica_core::AttentionPrivacy::Detailed => "detailed",
            _ => "applications",
        }),
    );
    shared.set(
        "attentionIdleSeconds",
        serde_json::json!(settings.idle_threshold_seconds),
    );
    shared
        .save(&state.directories.settings_file())
        .map_err(fail)?;
    let _ = app.emit("settings-updated", ());
    Ok(())
}

/// Match the process-owned idle inhibitor to Edith's Keep Awake switch.
pub async fn sync_prevent_sleep(app: &AppHandle, enabled: bool) -> CommandResult<()> {
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

/// Match the process-owned logind inhibitor to the stored Lid Awake switch.
pub async fn sync_lid_awake(app: &AppHandle, enabled: bool) -> CommandResult<()> {
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
pub struct PowerStatus {
    pub has_lid: bool,
    pub lid_awake_active: bool,
    pub prevent_sleep_active: bool,
    /// Seconds left on a timed session, or `None` when the switch is open-ended
    /// or off. A countdown is the difference between "on until I say" and "on
    /// for another eight minutes", which the switch alone cannot show.
    pub lid_awake_remaining_secs: Option<i64>,
    pub prevent_sleep_remaining_secs: Option<i64>,
}

/// Report actual process-owned locks, rather than echoing the saved switches.
#[tauri::command]
pub fn power_status(state: State<'_, AppState>) -> PowerStatus {
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
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
    pub package_url: Option<String>,
    pub published_at: Option<String>,
    pub notes: String,
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

/// Check the signed-in user's normal network path for the latest GitHub release.
/// Curl has an explicit timeout, redirect policy and failure status so the UI can
/// never hang indefinitely or mistake an HTML error page for release metadata.
#[tauri::command]
pub async fn update_check() -> CommandResult<UpdateInfo> {
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
            let package_url = release.assets.iter()
                .find(|asset| asset.name.ends_with("_amd64.deb") || asset.name.ends_with(".AppImage"))
                .map(|asset| asset.browser_download_url.clone());
            return Ok(UpdateInfo {
                update_available: newer_version(&release.tag_name, &current),
                current_version: current,
                latest_version: release.tag_name.trim_start_matches(['v', 'V']).to_string(),
                release_url: release.html_url,
                package_url,
                published_at: release.published_at,
                notes: release.body.unwrap_or_default(),
            });
        }

        // Before the first packaged Release GitHub answers 404. Fall back to
        // the source manifest so current installations still get a truthful
        // answer instead of a permanent error.
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
        })
    }).await.map_err(|error| format!("update check failed: {error}"))?
}

#[tauri::command]
pub fn system_snapshot(state: State<'_, AppState>) -> CommandResult<SystemSnapshot> {
    let mut sampler = state.sampler.lock().expect("sampler lock");
    Ok(sampler.sample())
}

#[tauri::command]
pub async fn system_processes() -> CommandResult<Vec<veronica_system::metrics::RunningProcess>> {
    tauri::async_runtime::spawn_blocking(veronica_system::metrics::running_processes)
        .await
        .map_err(|error| format!("cannot read running processes: {error}"))
}

/// The saved database connections. No credential is in a definition, which is
/// why the whole thing can be handed to the interface.
#[tauri::command]
pub fn database_connections(
    state: State<'_, AppState>,
) -> CommandResult<Vec<veronica_database::connection::ConnectionDefinition>> {
    veronica_database::store::MetadataStore::open(state.directories.database_store())
        .and_then(|store| store.connections())
        .map_err(fail)
}

/// Reach a server and report what it turned out to be.
#[tauri::command]
pub async fn database_test(
    app: AppHandle,
    connection: String,
) -> CommandResult<veronica_database::product::ProductIdentity> {
    let (definition, secrets) = database_context(&app, &connection)?;
    let mut session = veronica_database::session::Session::open(&definition, &secrets)
        .await
        .map_err(fail)?;
    session.identify().await.map_err(fail)
}

/// What is inside a database, one level at a time.
#[tauri::command]
pub async fn database_browse(
    app: AppHandle,
    connection: String,
    path: Vec<String>,
) -> CommandResult<Vec<veronica_database::identify::ObjectIdentifier>> {
    let (definition, secrets) = database_context(&app, &connection)?;
    let mut session = veronica_database::session::Session::open(&definition, &secrets)
        .await
        .map_err(fail)?;
    let parent = (!path.is_empty()).then(|| {
        veronica_database::identify::ObjectIdentifier::new(
            veronica_database::identify::ObjectKind::Table,
            path,
        )
    });
    session.objects(parent.as_ref()).await.map_err(fail)
}

/// Run a statement that reads. A write is refused by the adapter.
#[tauri::command]
pub async fn database_query(
    app: AppHandle,
    connection: String,
    statement: String,
    limit: u32,
    offset: u64,
) -> CommandResult<veronica_database::paging::Page> {
    let (definition, secrets) = database_context(&app, &connection)?;
    let mut session = veronica_database::session::Session::open(&definition, &secrets)
        .await
        .map_err(fail)?;
    session
        .query(
            &statement,
            &veronica_database::paging::PageRequest {
                page_size: veronica_database::paging::PageSize::clamped(limit),
                offset,
                ..Default::default()
            },
        )
        .await
        .map_err(fail)
}

/// Read a bounded page of one object.
#[tauri::command]
pub async fn database_read(
    app: AppHandle,
    connection: String,
    path: Vec<String>,
    limit: u32,
    offset: u64,
) -> CommandResult<veronica_database::paging::Page> {
    let (definition, secrets) = database_context(&app, &connection)?;
    let mut session = veronica_database::session::Session::open(&definition, &secrets)
        .await
        .map_err(fail)?;
    session
        .read(
            &veronica_database::identify::ObjectIdentifier::new(
                veronica_database::identify::ObjectKind::Table,
                path,
            ),
            &veronica_database::paging::PageRequest {
                page_size: veronica_database::paging::PageSize::clamped(limit),
                offset,
                ..Default::default()
            },
        )
        .await
        .map_err(fail)
}

/// What this server can be asked to do.
#[tauri::command]
pub async fn database_capabilities(
    app: AppHandle,
    connection: String,
) -> CommandResult<veronica_database::capabilities::Report> {
    let (definition, secrets) = database_context(&app, &connection)?;
    let mut session = veronica_database::session::Session::open(&definition, &secrets)
        .await
        .map_err(fail)?;
    session.capabilities().await.map_err(fail)
}

/// What Veronica has done, newest first.
#[tauri::command]
pub fn database_operations(
    state: State<'_, AppState>,
    limit: usize,
) -> CommandResult<Vec<veronica_database::store::OperationRecord>> {
    veronica_database::store::MetadataStore::open(state.directories.database_store())
        .and_then(|store| store.operations(limit))
        .map_err(fail)
}

/// Resolve a connection and the secret store together, since every database
/// command needs both.
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

/// Crawl a site and audit every page it lists.
///
/// Every run stays local: Veronica fetches the pages being audited and nothing
/// else. Crawling takes as long as the site takes to answer, so this is one
/// long-running command rather than a poll.
#[tauri::command]
pub async fn audit_site(
    site: String,
    limit: usize,
    concurrency: usize,
) -> CommandResult<veronica_audit::Report> {
    veronica_audit::audit(&site, limit, concurrency)
        .await
        .map_err(fail)
}

/// Which package sources this computer has, and whether changing one needs
/// authentication.
#[tauri::command]
pub async fn packages_sources() -> Vec<PackageSource> {
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
pub struct PackageSource {
    pub source: String,
    pub available: bool,
    pub needs_root: bool,
}

/// What has a newer version available. Installs nothing.
#[tauri::command]
pub async fn packages_updates() -> Vec<veronica_system::packages::Update> {
    veronica_system::packages::updates().await
}

/// What is installed, from every source.
#[tauri::command]
pub async fn packages_inventory() -> Vec<veronica_system::packages::Installed> {
    veronica_system::packages::inventory().await
}

/// Apply updates for one source.
///
/// An empty `names` means everything from that source. The interface confirms
/// first, and an apt or snap change then raises the desktop's own
/// authentication dialog through pkexec.
#[tauri::command]
pub async fn packages_update(
    source: String,
    names: Vec<String>,
) -> CommandResult<veronica_system::packages::UpdateResult> {
    let source = veronica_system::packages::Source::parse(&source)
        .ok_or_else(|| format!("unknown package source {source:?}"))?;
    veronica_system::packages::apply_update(source, &names)
        .await
        .map_err(fail)
}

/// What removing a package would take with it. Changes nothing.
#[tauri::command]
pub async fn packages_removal_plan(
    package: String,
) -> CommandResult<veronica_system::packages::RemovalPlan> {
    veronica_system::packages::removal_plan(&package)
        .await
        .map_err(fail)
}

/// The categories the cleaner knows, for the interface's checkboxes.
#[tauri::command]
pub fn cleaner_categories() -> &'static [veronica_core::CleanerCategory] {
    veronica_core::cleaner::CATEGORIES
}

/// Measure what could be reclaimed. Reads only.
///
/// Walking a home directory takes a moment, so this runs off the UI thread.
#[tauri::command]
pub async fn cleaner_scan(categories: Vec<String>) -> CommandResult<veronica_core::CleanerScan> {
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

/// Move the scanned items to the Trash.
///
/// The interface passes back the items it actually showed the user, rather than
/// this re-scanning: a scan between the display and the click could turn up
/// something the user never saw and never agreed to. Each item's own size is
/// re-measured, because the number on screen may be seconds old.
#[tauri::command]
pub async fn cleaner_clean(
    items: Vec<veronica_core::cleaner::Item>,
) -> CommandResult<veronica_core::cleaner::CleanReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = veronica_core::paths::home_dir()
            .ok_or_else(|| "cannot resolve the home directory".to_string())?;
        let mut report = veronica_core::cleaner::CleanReport::default();
        for item in items {
            let target = std::path::Path::new(&item.path);
            // Refuse anything outside the home directory, whatever the caller
            // asked for: the cleaner's whole contract is that it touches only
            // the user's own rebuildable files.
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
pub fn system_quit_process(pid: u32) -> CommandResult<()> {
    veronica_system::metrics::terminate_process(pid).map_err(fail)
}

/// Every catalogued tool, probed.
///
/// Separate from `diagnostics` on purpose: that is read on every launch and on
/// every settings change, and running five processes each time to answer a
/// question only the Extensions page asks would make the whole app wait.
#[tauri::command]
pub async fn tools_readiness() -> CommandResult<veronica_system::tools::Survey> {
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

/// Install one catalogued tool after an explicit click in the Extensions page.
///
/// Apt routes opt into `pkexec`: the system authentication dialog is the
/// confirmation. npm and manual routes preserve `Outcome::NotRun`, allowing
/// the page to explain the next step without pretending the request failed.
#[tauri::command]
pub async fn tools_install(tool: String) -> CommandResult<veronica_system::tools::Outcome> {
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

/// The live Herdr board.
///
/// Every `herdr` call inside carries its own timeout, so the page's poll
/// cannot leave work queued behind a wedged Herdr server.
#[tauri::command]
pub async fn herdr_board() -> CommandResult<veronica_system::herdr::HerdrBoard> {
    veronica_system::herdr::board().await.map_err(fail)
}

/// The Quinjet projects on this computer.
///
/// Shelling out to Quinjet and parsing its JSON is quick but not instant, so it
/// runs off the UI thread.
#[tauri::command]
pub async fn quinjet_projects() -> CommandResult<QuinjetBoard> {
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
pub struct QuinjetBoard {
    /// False rather than an error: not having Quinjet is a normal state that
    /// the page explains, not a failure.
    pub installed: bool,
    pub projects: Vec<veronica_system::quinjet::Project>,
}

/// Open one worktree in the installed terminal.
#[tauri::command]
pub fn quinjet_open(state: State<'_, AppState>, worktree: String) -> CommandResult<()> {
    // The appearance follows the app's own, so Quinjet does not open light
    // inside a dark desktop. It is the scheme behind the theme that matters,
    // not its name: Midnight and Carbon are as dark as Graphite.
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
pub fn herdr_open(session: String, pane_id: Option<String>) -> CommandResult<()> {
    veronica_system::herdr::open_terminal(&session, pane_id.as_deref()).map_err(fail)
}

/// The per-application mixer: every stream PipeWire currently holds.
///
/// One `pw-dump` describes the whole graph, so the mixer refreshes with a
/// single process rather than one call per row.
#[tauri::command]
pub async fn audio_streams() -> CommandResult<Vec<veronica_system::audio::AudioStream>> {
    veronica_system::audio::streams().await.map_err(fail)
}

#[tauri::command]
pub async fn audio_stream_volume(id: u32, volume: f32) -> CommandResult<()> {
    veronica_system::audio::set_stream_volume(id, volume)
        .await
        .map_err(fail)
}

/// Flip one stream's mute and report what it became, so a row updates without
/// re-reading the whole graph.
#[tauri::command]
pub async fn audio_stream_toggle_mute(id: u32) -> CommandResult<bool> {
    veronica_system::audio::toggle_stream_muted(id)
        .await
        .map_err(fail)
}

/// Bluetooth adapters and devices, from BlueZ.
///
/// This never fails: a machine with no radio, or with the daemon stopped, is a
/// normal outcome that comes back as `unavailable` with a reason to show.
#[tauri::command]
pub async fn bluetooth_state() -> veronica_system::BluetoothState {
    veronica_system::bluetooth::state().await
}

/// The emoji catalogue's groups, for the picker's tab rail.
///
/// The catalogue is embedded, so this is a parse of a `&'static str` rather
/// than a file read; it is cached in state so a hotkey-opened picker does not
/// re-parse two thousand entries.
#[tauri::command]
pub fn emoji_groups(state: State<'_, AppState>) -> Vec<veronica_core::emoji::Group> {
    state.emoji.groups.clone()
}

/// Search the catalogue, or list one group when `query` is empty.
///
/// Ranking happens before the group filter, deliberately: narrowing first would
/// change which results win, and the ranking is the point.
#[tauri::command]
pub fn emoji_search(
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

/// The emoji you reach for most, as the ledger ranks them.
#[tauri::command]
pub fn emoji_recents(state: State<'_, AppState>, limit: usize) -> CommandResult<Vec<EmojiRow>> {
    let ledger = veronica_core::emoji::UsageLedger::load(&state.directories.emoji_usage_file())
        .map_err(fail)?;
    let tone = state.emoji_tone();
    Ok(ledger
        .ranked(chrono::Utc::now().timestamp_millis(), limit)
        .into_iter()
        .filter_map(|character| {
            // A character the catalogue no longer carries is dropped rather
            // than shown as a blank cell.
            state
                .emoji
                .find(&character)
                .map(|emoji| EmojiRow::new(emoji, tone))
        })
        .collect())
}

/// Copy one emoji and record the pick.
///
/// The clipboard is the part that always works; inserting into the app you were
/// typing in needs the RemoteDesktop portal, so it is attempted and reported
/// rather than assumed.
#[tauri::command]
pub async fn emoji_copy(
    app: AppHandle,
    window: tauri::WebviewWindow,
    character: String,
    insert: bool,
) -> CommandResult<EmojiCopyResult> {
    let (path, base) = {
        let state = app.state::<AppState>();
        let base = state
            .emoji
            // The tone variant is what gets copied; the ledger counts the base
            // character, so picking 👋🏿 and 👋 rank as the same habit.
            .emoji()
            .iter()
            .find(|emoji| emoji.character(state.emoji_tone()) == character)
            .map(|emoji| emoji.character.clone())
            .unwrap_or_else(|| character.clone());
        (state.directories.emoji_usage_file(), base)
    };

    let mut ledger = veronica_core::emoji::UsageLedger::load(&path).map_err(fail)?;
    ledger.record(&base, chrono::Utc::now().timestamp_millis());
    ledger.save(&path).map_err(fail)?;

    let copied_via = veronica_system::selection::write(&character)
        .await
        .map(|writer| writer.title().to_string())
        .map_err(fail)?;

    // The compact picker temporarily owns focus. Hide it before synthesising
    // Ctrl+V so GNOME restores the app the user was typing in, then give the
    // compositor one frame to settle before sending the keystroke.
    if should_restore_emoji_target(window.label(), insert) {
        window
            .hide()
            .map_err(|error| format!("cannot dismiss the emoji picker: {error}"))?;
        tokio::time::sleep(std::time::Duration::from_millis(180)).await;
    }

    let inserted = if insert {
        match veronica_system::selection::paste_in_place().await {
            Ok(()) => true,
            Err(error) => {
                // The emoji is on the clipboard either way, so a refused portal
                // request is reported, not raised.
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

fn should_restore_emoji_target(window_label: &str, insert: bool) -> bool {
    insert && window_label == crate::emoji_picker::LABEL
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmojiRow {
    /// The character in the configured skin tone, which is what gets copied.
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
pub struct EmojiCopyResult {
    pub character: String,
    pub copied_via: String,
    /// Whether it also went into the app you were typing in.
    pub inserted: bool,
}

#[tauri::command]
pub async fn microphone_state() -> CommandResult<veronica_system::audio::VolumeState> {
    veronica_system::audio::microphone().await.map_err(fail)
}

#[tauri::command]
pub async fn microphone_toggle() -> CommandResult<veronica_system::audio::VolumeState> {
    veronica_system::audio::toggle_microphone()
        .await
        .map_err(fail)
}

/// What is playing, or `None` when no MPRIS player is registered.
///
/// A fresh session bus connection per call keeps this stateless; the calls are
/// infrequent (a poll while a surface is visible) and a cached connection would
/// have to be re-established whenever the bus restarts anyway.
#[tauri::command]
pub async fn media_now_playing() -> CommandResult<Option<veronica_media::NowPlaying>> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|e| format!("cannot reach the session bus: {e}"))?;
    let mut playing = veronica_media::now_playing(&connection)
        .await
        .map_err(fail)?;

    // Replace the file:// art URL with an inline copy the webview can render.
    // Unusable art becomes None so the interface shows its placeholder rather
    // than an empty tile.
    if let Some(playing) = playing.as_mut() {
        playing.art_url = playing.art_url.as_deref().and_then(crate::art::to_data_url);
    }
    Ok(playing)
}

/// Send a transport command to the active player.
#[tauri::command]
pub async fn media_control(action: String) -> CommandResult<()> {
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
pub fn music_library() -> CommandResult<Vec<veronica_media::LocalTrack>> {
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
pub struct AgendaView {
    /// False when no calendar is configured, which is different from having
    /// nothing scheduled and needs different wording.
    pub has_calendars: bool,
    pub days: Vec<veronica_calendar::AgendaDay>,
    pub next_up: Option<veronica_calendar::Event>,
    pub happening_now: Option<veronica_calendar::Event>,
}

/// The agenda for the next `days`, grouped by day.
///
/// `with_links` controls whether each event is looked up in Evolution Data
/// Server for a join link: worth it for the calendar page, skipped by the notch,
/// where it would add a D-Bus round trip per event to a one-line readout.
#[tauri::command]
pub async fn calendar_agenda(days: i64, with_links: bool) -> CommandResult<AgendaView> {
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

/// Open the user's normal GNOME calendar application without sending calendar
/// data anywhere. `gtk-launch` respects the installed desktop entry.
#[tauri::command]
pub fn calendar_open() -> CommandResult<()> {
    std::process::Command::new("gtk-launch")
        .arg("org.gnome.Calendar")
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("cannot open GNOME Calendar: {error}"))
}

/// The notification history, newest first.
#[tauri::command]
pub fn notifications_list(
    state: State<'_, AppState>,
) -> CommandResult<Vec<veronica_system::Notification>> {
    Ok(state
        .notifications
        .lock()
        .expect("notifications lock")
        .clone())
}

/// Remove one entry from the history.
///
/// This does not recall the desktop's own banner: GNOME owns that, and Veronica
/// is only watching. The distinction is reflected in the interface wording.
#[tauri::command]
pub fn notifications_dismiss(state: State<'_, AppState>, id: u64) -> CommandResult<()> {
    state
        .notifications
        .lock()
        .expect("notifications lock")
        .retain(|entry| entry.id != id);
    Ok(())
}

#[tauri::command]
pub fn notifications_clear(state: State<'_, AppState>) -> CommandResult<()> {
    state
        .notifications
        .lock()
        .expect("notifications lock")
        .clear();
    Ok(())
}

/// Settings key holding the stored fleet, shared with the CLI.
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

/// Probe every machine in the fleet.
///
/// One unreachable host reports its own error rather than failing the view, so
/// a laptop that is off does not hide the machines that are on.
#[tauri::command]
pub async fn machines_probe(
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

/// Add a machine reached over SSH.
#[tauri::command]
pub fn machines_add(
    state: State<'_, AppState>,
    target: String,
    name: Option<String>,
    port: Option<u16>,
) -> CommandResult<veronica_machines::Machine> {
    use veronica_machines::host;

    let label = name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| target.clone());
    let id = host::slugify(&label);
    if id == "local" {
        return Err("\"local\" is reserved for this computer".to_string());
    }

    let mut settings = state.settings.lock().expect("settings lock");
    let mut machines: Vec<veronica_machines::Machine> = settings
        .get(MACHINES_KEY)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();

    if machines.iter().any(|machine| machine.id == id) {
        return Err(format!("a machine called {id} already exists"));
    }

    let machine = veronica_machines::Machine {
        id,
        name: label,
        reach: veronica_machines::Reach::Ssh { target, port },
        // Wake-on-LAN is set up from the CLI, where a MAC address is something
        // the user has to hand; the interface adds machines by SSH target.
        mac: None,
    };
    machines.push(machine.clone());
    settings.set(
        MACHINES_KEY,
        serde_json::to_value(&machines).map_err(|e| e.to_string())?,
    );
    settings
        .save(&state.directories.settings_file())
        .map_err(fail)?;
    Ok(machine)
}

#[tauri::command]
pub fn machines_remove(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    let mut settings = state.settings.lock().expect("settings lock");
    let mut machines: Vec<veronica_machines::Machine> = settings
        .get(MACHINES_KEY)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    let before = machines.len();
    machines.retain(|machine| machine.id != id);
    if machines.len() == before {
        return Err(format!("no machine called {id}"));
    }
    settings.set(
        MACHINES_KEY,
        serde_json::to_value(&machines).map_err(|e| e.to_string())?,
    );
    settings
        .save(&state.directories.settings_file())
        .map_err(fail)?;
    Ok(())
}

/// SSH aliases in the user's config that are not configured yet.
#[tauri::command]
pub fn machines_discover(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    use veronica_machines::host;

    let settings = state.settings_snapshot();
    let configured: Vec<veronica_machines::Machine> = settings
        .get(MACHINES_KEY)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    let config = veronica_core::paths::home_dir()
        .map(|home| home.join(".ssh/config"))
        .unwrap_or_default();
    Ok(host::ssh_config_hosts(&config)
        .into_iter()
        .filter(|alias| {
            !configured
                .iter()
                .any(|machine| machine.ssh_target() == Some(alias.as_str()))
        })
        .collect())
}

#[tauri::command]
pub fn machines_terminal(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    let machine = machine_by_id(&state, &id)?;
    veronica_machines::manage::open_terminal(&machine).map_err(fail)
}

#[tauri::command]
pub async fn machines_files(
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
pub async fn machines_file_download(
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
pub async fn machines_containers(
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<Vec<veronica_machines::ContainerInfo>> {
    let machine = machine_by_id(&state, &id)?;
    veronica_machines::manage::containers(&machine, veronica_machines::DEFAULT_TIMEOUT)
        .await
        .map_err(fail)
}

#[tauri::command]
pub async fn machines_container_action(
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

/// A container's recent output, tailed rather than followed.
#[tauri::command]
pub async fn machines_container_logs(
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

/// Restart or shut down a remote machine.
///
/// Refused for this computer, and never escalated: the account Veronica
/// connects as has to be permitted already. The interface confirms first.
#[tauri::command]
pub async fn machines_power(
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

/// Send a Wake-on-LAN packet to a machine that has a MAC address stored.
///
/// Nothing can be confirmed: the machine is not answering yet, which is the
/// point, so this reports that the packet went out and no more.
#[tauri::command]
pub async fn machines_wake(state: State<'_, AppState>, id: String) -> CommandResult<()> {
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

/// The clipboard history, newest first.
#[tauri::command]
pub fn clipboard_list(state: State<'_, AppState>, query: String) -> CommandResult<Vec<ClipRow>> {
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
pub struct ClipRow {
    pub id: u64,
    pub preview: String,
    /// The full text, so copying back needs no second call.
    pub text: String,
    pub lines: usize,
    pub bytes: usize,
    pub count: u32,
    pub last_seen: String,
}

// -- presenter ---------------------------------------------------------------

/// The resolved presenter state, plus what detection currently sees.
#[tauri::command]
pub async fn presenter_state(app: AppHandle) -> CommandResult<crate::presenter::PresenterView> {
    // Detection is only meaningful while the feature is on, and introspecting
    // the compositor for a screen nobody asked about would be pointless work.
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

/// Presenter's own actions, so the interface does not have to know which
/// settings key each one writes.
#[tauri::command]
pub fn presenter_set(app: AppHandle, action: String) -> CommandResult<()> {
    let state = app.state::<AppState>();
    let (key, value) = match action.as_str() {
        "enable" => ("presenterEnabled", serde_json::Value::Bool(true)),
        "disable" => ("presenterEnabled", serde_json::Value::Bool(false)),
        "start" => ("presenterMode", serde_json::Value::Bool(true)),
        "stop" => ("presenterMode", serde_json::Value::Bool(false)),
        // Dismiss the *current* detected share without turning detection off.
        // Cleared automatically when that share ends.
        "dismiss" => ("presenterAutoPaused", serde_json::Value::Bool(true)),
        "resume" => ("presenterAutoPaused", serde_json::Value::Bool(false)),
        other => return Err(format!("unknown presenter action: {other}")),
    };
    state.set_setting(key, value).map_err(fail)?;
    let _ = app.emit("settings-updated", "presenter");
    Ok(())
}

// -- alerts ------------------------------------------------------------------

/// Everything the Alerts screen needs in one call: the resolved switches, the
/// windows currently being watched, and when each reminder would next fire.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertsView {
    pub settings: veronica_usage::alerts::NotifySettings,
    /// Seconds between polls, after clamping.
    pub poll_seconds: u64,
    pub session: Option<WatchedWindow>,
    pub week: Option<WatchedWindow>,
    /// Why there is nothing to watch, when that is the case.
    pub note: Option<String>,
    pub session_reminder_at: Option<String>,
    pub week_reminder_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchedWindow {
    pub percent: f64,
    pub resets_at: Option<String>,
    /// "2 h 14 min", or absent when the provider gave no reset time.
    pub resets_in: Option<String>,
    pub level: veronica_usage::limits::UsageLevel,
    pub zone: veronica_usage::limits::PacingZone,
}

#[tauri::command]
pub async fn alerts_view(app: AppHandle) -> CommandResult<AlertsView> {
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
            .map(|at| {
                pacing_zone(
                    pacing_delta(window.percent, at, kind.duration_secs(), now),
                    notify.pacing_margin,
                )
            })
            .unwrap_or(veronica_usage::limits::PacingZone::OnTrack),
    };

    // Read directly rather than through the gauge collector, because the alerts
    // watch Claude's two windows specifically, as Edith's notifier does.
    let (session, week, note) = match veronica_usage::claude::limits_for_user(now).await {
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

/// Post one sample banner, so the user can confirm alerts arrive at all.
///
/// Deliberately leaves the notifier state untouched: consuming a real edge to
/// prove notifications work would suppress the alert it was testing.
#[tauri::command]
pub async fn alerts_test() -> CommandResult<String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("no session bus, so no notifications: {error}"))?;
    match crate::alerts::send_test(&connection).await {
        Ok(_) => Ok("Sent — check your notification list if no banner appeared.".to_string()),
        Err(error) => Err(fail(error)),
    }
}

/// Forget the remembered levels and zones.
///
/// Useful after changing thresholds: the next poll then treats the current state
/// as a fresh edge rather than comparing it against the old scale.
#[tauri::command]
pub fn alerts_reset(state: State<'_, AppState>) -> CommandResult<()> {
    let path = state.directories.alerts_state_file();
    let mut notifier = veronica_usage::alerts::NotifierState::load(&path);
    notifier.reset_tracking();
    notifier.save(&path).map_err(fail)
}

// -- colour picker -----------------------------------------------------------

/// One recorded swatch, with every representation the interface offers so the
/// grid needs no second call to show or copy a colour.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwatchRow {
    pub id: u64,
    pub hex: String,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub profile: String,
    pub profile_label: &'static str,
    pub picked_at: String,
    /// Keyed by format id, e.g. `hex` or `gdkRgba`.
    pub formats: std::collections::BTreeMap<&'static str, String>,
    /// Whether a dark label is legible on this colour.
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

/// Which copy formats exist, so the interface never hard-codes the list.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatOption {
    pub id: &'static str,
    pub label: &'static str,
}

#[tauri::command]
pub fn color_formats() -> CommandResult<Vec<FormatOption>> {
    Ok(veronica_core::CopyFormat::ALL
        .iter()
        .map(|format| FormatOption {
            id: format.key(),
            label: format.title(),
        })
        .collect())
}

#[tauri::command]
pub fn color_swatches(state: State<'_, AppState>) -> CommandResult<Vec<SwatchRow>> {
    let history =
        veronica_core::SwatchHistory::load(&state.directories.swatches_file()).map_err(fail)?;
    Ok(history.swatches().iter().map(swatch_row).collect())
}

/// Open the eyedropper, record what comes back and copy it.
///
/// The recording happens before the copy, so a session without any clipboard
/// route still keeps the colour the user just sampled; the reply says whether
/// the copy landed and by which route.
#[tauri::command]
pub async fn color_pick(app: AppHandle) -> CommandResult<PickResult> {
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
                    .and_then(|value| value.as_u64())
                    .unwrap_or(veronica_core::swatches::DEFAULT_HISTORY_SIZE as u64)
                    as usize,
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
pub struct PickResult {
    pub swatch: SwatchRow,
    /// The text that was put on the clipboard, in the configured format.
    pub value: String,
    pub format: &'static str,
    /// Which backend opened the eyedropper.
    pub source: &'static str,
    pub copied_via: Option<&'static str>,
    /// Set when the colour was recorded but could not be copied.
    pub copy_error: Option<String>,
}

/// Copy one recorded swatch again, in the requested or configured format.
#[tauri::command]
pub async fn color_copy(
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
    let chosen = format
        .as_deref()
        .map(CopyFormat::parse)
        .unwrap_or(configured);

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
pub struct CopyResult {
    pub value: String,
    pub format: &'static str,
    pub copied_via: &'static str,
}

#[tauri::command]
pub fn color_forget(state: State<'_, AppState>, id: u64) -> CommandResult<()> {
    let path = state.directories.swatches_file();
    let mut history = veronica_core::SwatchHistory::load(&path).map_err(fail)?;
    if !history.remove(id) {
        return Err(format!("no swatch {id}"));
    }
    history.save(&path).map_err(fail)
}

#[tauri::command]
pub fn color_clear(state: State<'_, AppState>) -> CommandResult<()> {
    let path = state.directories.swatches_file();
    let mut history = veronica_core::SwatchHistory::load(&path).map_err(fail)?;
    history.clear();
    history.save(&path).map_err(fail)
}

#[tauri::command]
pub fn clipboard_remove(state: State<'_, AppState>, id: u64) -> CommandResult<()> {
    use veronica_core::ClipboardHistory;

    let path = state.directories.clipboard_db();
    let mut history = ClipboardHistory::load(&path).map_err(fail)?;
    if !history.remove(id) {
        return Err(format!("no clipboard entry {id}"));
    }
    history.save(&path).map_err(fail)
}

#[tauri::command]
pub fn clipboard_clear(state: State<'_, AppState>) -> CommandResult<()> {
    use veronica_core::ClipboardHistory;

    let path = state.directories.clipboard_db();
    let mut history = ClipboardHistory::load(&path).map_err(fail)?;
    history.clear();
    history.save(&path).map_err(fail)
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) -> CommandResult<()> {
    crate::main_window::show(&app, None).map_err(fail)
}

/// Open a path or URL with the desktop's default handler.
#[tauri::command]
pub fn open_external(target: String) -> CommandResult<()> {
    // Only http(s) and absolute local paths, so a crafted string cannot be used
    // to launch an arbitrary command through the handler.
    let allowed =
        target.starts_with("https://") || target.starts_with("http://") || target.starts_with('/');
    if !allowed {
        return Err(format!("refusing to open {target:?}"));
    }
    std::process::Command::new("xdg-open")
        .arg(&target)
        .spawn()
        .map_err(|e| format!("cannot open {target}: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{newer_version, parsed_shell_extension_status};

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
    fn update_versions_compare_numerically_and_ignore_release_prefixes() {
        assert!(newer_version("v0.2.0", "0.1.99"));
        assert!(newer_version("1.0.1", "1.0.0"));
        assert!(!newer_version("0.1.8", "0.1.8"));
        assert!(!newer_version("0.1.7", "0.1.8"));
    }

    #[test]
    fn only_web_urls_and_absolute_paths_are_openable() {
        // A relative or scheme-less string could otherwise reach a handler that
        // treats it as something executable.
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
    //! The interface reads these payloads by field name, and a nested struct
    //! does not inherit its parent's `rename_all`. That mismatch compiles, ships
    //! and renders as `undefined`, so every shape the interface depends on is
    //! pinned here rather than assumed.

    use super::*;

    #[test]
    fn compact_emoji_picker_releases_focus_before_inserting() {
        assert!(should_restore_emoji_target(
            crate::emoji_picker::LABEL,
            true
        ));
        assert!(!should_restore_emoji_target("main", true));
        assert!(!should_restore_emoji_target(
            crate::emoji_picker::LABEL,
            false
        ));
    }

    /// Every key in a JSON object, recursively, as `parent.child` paths.
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
        // Spot-check the names the colour page reads directly.
        assert_eq!(json["hex"], "#336699");
        assert!(json["prefersDarkText"].is_boolean());
        assert!(json["profileLabel"].is_string());
        assert!(json["pickedAt"].is_string());
        // Format ids are map keys, not renamed fields, so they stay as written.
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
        // The enums are values rather than field names, and the interface's
        // unions match them exactly.
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
        // This nesting is the one that actually broke: a struct two levels down.
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
        // Flattened, so the state's fields sit at the top level.
        assert!(json["autoEnabled"].is_boolean());
        assert!(json["blurredClasses"].is_array());
        assert!(json["share"]["screencastSessions"].is_number());
    }

    #[test]
    fn the_extension_report_carries_the_settings_key_the_interface_toggles() {
        // Without this the interface needs its own copy of all fifteen keys.
        let report = veronica_core::Diagnostics::collect(
            &veronica_core::AppDirectories::with_env(
                std::path::Path::new("/tmp"),
                Default::default(),
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
