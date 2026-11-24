use serde::Serialize;
use tauri::State;

use super::{fail, CommandResult};
use crate::state::AppState;

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
