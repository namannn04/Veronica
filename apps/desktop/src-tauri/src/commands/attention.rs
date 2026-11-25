use tauri::State;

use super::{fail, CommandResult};
use crate::state::AppState;

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
