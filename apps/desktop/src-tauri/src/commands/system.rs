use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use veronica_system::metrics::SystemSnapshot;

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
pub(crate) fn system_quit_process(pid: u32) -> CommandResult<()> {
    veronica_system::metrics::terminate_process(pid).map_err(fail)
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
