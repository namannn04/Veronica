use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;
use tauri::State;

use super::bridge::{dispatch, Failure};
use super::host::Host;
use super::jobs::{submit, Action, Job, Kind};
use super::library::{self, Offer, Row, Shelf};
use super::updates::Preferences;
use crate::state::AppState;

type CommandResult<T> = Result<T, String>;

fn fail(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}

fn host(state: &State<'_, AppState>) -> Arc<Host> {
    Arc::clone(&state.extensions)
}

async fn blocking<T: Send + 'static>(
    host: Arc<Host>,
    work: impl FnOnce(&Host) -> anyhow::Result<T> + Send + 'static,
) -> CommandResult<T> {
    tauri::async_runtime::spawn_blocking(move || work(&host))
        .await
        .map_err(fail)?
        .map_err(fail)
}

#[tauri::command]
pub(crate) async fn ext_list(state: State<'_, AppState>) -> CommandResult<Vec<Row>> {
    library::rows(&host(&state)).await.map_err(fail)
}

#[tauri::command]
pub(crate) async fn ext_status(state: State<'_, AppState>, id: String) -> CommandResult<Row> {
    library::row(&host(&state), &id).await.map_err(fail)
}

#[tauri::command]
pub(crate) async fn ext_restart(state: State<'_, AppState>, id: String) -> CommandResult<Row> {
    let host = host(&state);
    host.supervisor().restart(&id).await.map_err(fail)?;
    library::row(&host, &id).await.map_err(fail)
}

#[tauri::command]
pub(crate) async fn ext_catalog_refresh(state: State<'_, AppState>) -> CommandResult<Shelf> {
    blocking(host(&state), library::refresh).await
}

#[tauri::command]
pub(crate) async fn ext_catalog_search(
    state: State<'_, AppState>,
    query: String,
    category: Option<String>,
) -> CommandResult<Shelf> {
    blocking(host(&state), move |host| {
        library::search(host, &query, category.as_deref())
    })
    .await
}

#[tauri::command]
pub(crate) async fn ext_catalog_info(
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<Offer> {
    blocking(host(&state), move |host| library::info(host, &id)).await
}

fn queue(state: &State<'_, AppState>, kind: Kind, id: String) -> Job {
    submit(&host(state), Action::on(kind, id))
}

#[tauri::command]
pub(crate) async fn ext_install(state: State<'_, AppState>, id: String) -> CommandResult<Job> {
    Ok(queue(&state, Kind::Install, id))
}

#[tauri::command]
pub(crate) async fn ext_sideload(state: State<'_, AppState>, path: PathBuf) -> CommandResult<Job> {
    Ok(submit(&host(&state), Action::sideload(path).map_err(fail)?))
}

#[tauri::command]
pub(crate) async fn ext_update(state: State<'_, AppState>, id: String) -> CommandResult<Job> {
    Ok(queue(&state, Kind::Update, id))
}

#[tauri::command]
pub(crate) async fn ext_enable(state: State<'_, AppState>, id: String) -> CommandResult<Job> {
    Ok(queue(&state, Kind::Enable, id))
}

#[tauri::command]
pub(crate) async fn ext_disable(state: State<'_, AppState>, id: String) -> CommandResult<Job> {
    Ok(queue(&state, Kind::Disable, id))
}

#[tauri::command]
pub(crate) async fn ext_remove(
    state: State<'_, AppState>,
    id: String,
    purge: bool,
) -> CommandResult<Job> {
    let kind = if purge { Kind::Purge } else { Kind::Remove };
    Ok(queue(&state, kind, id))
}

#[tauri::command]
pub(crate) async fn ext_rollback(state: State<'_, AppState>, id: String) -> CommandResult<Job> {
    Ok(queue(&state, Kind::Rollback, id))
}

#[tauri::command]
pub(crate) async fn ext_jobs(state: State<'_, AppState>) -> CommandResult<Vec<Job>> {
    Ok(state.extensions.jobs().list())
}

#[tauri::command]
pub(crate) async fn ext_page_opened(
    state: State<'_, AppState>,
    extension: String,
    page: String,
) -> CommandResult<()> {
    state.extensions.open_page(&extension, &page);
    Ok(())
}

#[tauri::command]
pub(crate) async fn ext_page_closed(
    state: State<'_, AppState>,
    extension: String,
    page: String,
) -> CommandResult<()> {
    state.extensions.close_page(&extension, &page);
    Ok(())
}

#[tauri::command]
pub(crate) async fn ext_confirm_answer(
    state: State<'_, AppState>,
    request: u64,
    accepted: bool,
) -> CommandResult<bool> {
    Ok(state.extensions.answer(request, accepted))
}

#[tauri::command]
pub(crate) async fn ext_bridge(
    state: State<'_, AppState>,
    extension: String,
    page: String,
    method: String,
    params: Option<Value>,
) -> Result<Value, Failure> {
    let host = host(&state);
    dispatch(
        &host,
        &extension,
        &page,
        &method,
        params.unwrap_or(Value::Null),
    )
    .await
}

#[tauri::command]
pub(crate) async fn ext_auto_update(state: State<'_, AppState>) -> CommandResult<Preferences> {
    Ok(Preferences::read(&state.settings_snapshot()))
}

#[tauri::command]
pub(crate) async fn ext_auto_update_set(
    state: State<'_, AppState>,
    preferences: Preferences,
) -> CommandResult<Preferences> {
    state
        .update_settings(|settings| {
            preferences.write(settings);
            Ok(Preferences::read(settings))
        })
        .map_err(fail)
}
