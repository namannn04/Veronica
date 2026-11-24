use serde::Serialize;

use super::{fail, CommandResult};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgendaView {
    pub has_calendars: bool,
    pub days: Vec<veronica_calendar::AgendaDay>,
    pub next_up: Option<veronica_calendar::Event>,
    pub happening_now: Option<veronica_calendar::Event>,
}

#[tauri::command]
pub(crate) async fn calendar_agenda(days: i64, with_links: bool) -> CommandResult<AgendaView> {
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

#[tauri::command]
pub(crate) async fn calendar_open() -> CommandResult<()> {
    let output = tauri::async_runtime::spawn_blocking(|| {
        veronica_core::host::command("gtk-launch")
            .arg("org.gnome.Calendar")
            .stdin(std::process::Stdio::null())
            .output()
    })
    .await
    .map_err(|error| format!("cannot open GNOME Calendar: {error}"))?
    .map_err(|error| format!("cannot open GNOME Calendar: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if detail.contains("no such application") {
        "GNOME Calendar is not installed. Install it from the App Center, or add an account in Settings › Online Accounts to see events here.".to_string()
    } else {
        format!("cannot open GNOME Calendar: {detail}")
    })
}
