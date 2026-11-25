use std::process::Stdio;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use veronica_calendar::{agenda, server, AgendaDay, Event};
use veronica_sdk::protocol::rpc::{error_code, ErrorObject};
use veronica_sdk::rpc::{failure, params, reply};

pub(crate) const METHODS: [&str; 2] = ["agenda", "open"];
const CALENDAR_APP: &str = "org.gnome.Calendar";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgendaParams {
    days: i64,
    #[serde(default)]
    with_links: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgendaView {
    pub has_calendars: bool,
    pub days: Vec<AgendaDay>,
    pub next_up: Option<Event>,
    pub happening_now: Option<Event>,
}

pub(crate) async fn agenda(days: i64, with_links: bool) -> anyhow::Result<AgendaView> {
    let connection = zbus::Connection::session().await?;
    let has_calendars = server::has_calendars(&connection).await.unwrap_or(false);
    let events = if with_links {
        server::events_with_links(&connection, days).await?
    } else {
        server::events(&connection, days).await?
    };
    let now = chrono::Local::now();
    let remaining = agenda::upcoming(&events, now);
    Ok(AgendaView {
        has_calendars,
        next_up: agenda::next_up(&remaining, now).cloned(),
        happening_now: agenda::happening_now(&remaining, now).cloned(),
        days: agenda::group_by_day(&remaining, now),
    })
}

fn open_calendar() -> Result<(), ErrorObject> {
    let output = veronica_sdk::spawn::command("gtk-launch")
        .arg(CALENDAR_APP)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| {
            ErrorObject::new(
                error_code::UNAVAILABLE,
                format!("cannot open GNOME Calendar: {error}"),
            )
        })?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let message = if detail.contains("no such application") {
        "GNOME Calendar is not installed. Install it from the App Center, or add an account in Settings › Online Accounts to see events here.".to_string()
    } else {
        format!("cannot open GNOME Calendar: {detail}")
    };
    Err(ErrorObject::new(error_code::UNAVAILABLE, message))
}

pub(crate) async fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    match method {
        "agenda" => {
            let AgendaParams { days, with_links } = params(arguments)?;
            let view = agenda(days, with_links)
                .await
                .map_err(|error| failure(&error))?;
            reply(&view)
        }
        "open" => {
            tokio::task::spawn_blocking(open_calendar)
                .await
                .map_err(|error| {
                    ErrorObject::new(error_code::INTERNAL_ERROR, error.to_string())
                })??;
            Ok(Value::Null)
        }
        other => Err(ErrorObject::method_not_found(other)),
    }
}
