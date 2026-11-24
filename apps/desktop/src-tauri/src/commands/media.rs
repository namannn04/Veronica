use super::{fail, CommandResult};

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
