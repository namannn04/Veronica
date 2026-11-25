use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use super::{fail, CommandResult};
use crate::state::AppState;

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
