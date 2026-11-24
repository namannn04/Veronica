use tauri::{AppHandle, Emitter, Manager, State};

use super::{fail, CommandResult};
use crate::state::AppState;

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
