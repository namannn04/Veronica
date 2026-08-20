//! Commands the interface calls.
//!
//! Each returns plain serialisable data. Errors become strings, because a Tauri
//! command's error type must serialise and `anyhow::Error` does not; the
//! `{:#}` format keeps the whole context chain so the UI can show a real reason
//! rather than "failed".

use serde::Serialize;
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

#[tauri::command]
pub fn settings_all(state: State<'_, AppState>) -> CommandResult<serde_json::Value> {
    let settings = state.settings_snapshot();
    serde_json::to_value(settings.as_map()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn settings_set(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: serde_json::Value,
) -> CommandResult<()> {
    state.set_setting(&key, value).map_err(fail)?;
    let _ = app.emit("settings-updated", &key);
    Ok(())
}

#[tauri::command]
pub fn system_snapshot(state: State<'_, AppState>) -> CommandResult<SystemSnapshot> {
    let mut sampler = state.sampler.lock().expect("sampler lock");
    Ok(sampler.sample())
}

#[tauri::command]
pub async fn microphone_state() -> CommandResult<veronica_system::audio::VolumeState> {
    veronica_system::audio::microphone().await.map_err(fail)
}

#[tauri::command]
pub async fn microphone_toggle() -> CommandResult<veronica_system::audio::VolumeState> {
    veronica_system::audio::toggle_microphone().await.map_err(fail)
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
    let mut playing = veronica_media::now_playing(&connection).await.map_err(fail)?;

    // Replace the file:// art URL with an inline copy the webview can render.
    // Unusable art becomes None so the interface shows its placeholder rather
    // than an empty tile.
    if let Some(playing) = playing.as_mut() {
        playing.art_url = playing
            .art_url
            .as_deref()
            .and_then(crate::art::to_data_url);
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

#[tauri::command]
pub fn show_main_window(app: AppHandle) -> CommandResult<()> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    Ok(())
}

/// Open a path or URL with the desktop's default handler.
#[tauri::command]
pub fn open_external(target: String) -> CommandResult<()> {
    // Only http(s) and absolute local paths, so a crafted string cannot be used
    // to launch an arbitrary command through the handler.
    let allowed = target.starts_with("https://")
        || target.starts_with("http://")
        || target.starts_with('/');
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
