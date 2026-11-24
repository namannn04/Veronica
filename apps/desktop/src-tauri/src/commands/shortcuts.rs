use tauri::State;
use veronica_host::shortcuts::Planned;

use crate::shortcuts::Shortcuts;

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn shortcuts_list(shortcuts: State<'_, Shortcuts>) -> Vec<Planned> {
    shortcuts.planned()
}
