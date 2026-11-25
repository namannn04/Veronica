use serde::Deserialize;
use serde_json::Value;
use veronica_core::{AttentionRepository, AttentionSettings};
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::protocol::rpc::ErrorObject;
use veronica_sdk::rpc::{failure, params, reply};

use crate::shared::mirror;

pub(crate) const METHODS: [&str; 7] = [
    "status",
    "start",
    "stop",
    "history",
    "overview",
    "settings",
    "saveSettings",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartParams {
    name: String,
    duration_seconds: i64,
}

#[derive(Deserialize)]
struct OverviewParams {
    days: i64,
}

#[derive(Deserialize)]
struct SaveParams {
    settings: AttentionSettings,
}

pub(crate) fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    let fail = |error: anyhow::Error| failure(&error);
    let directories = AppDirectories::current().map_err(fail)?;
    let repository = AttentionRepository::new(directories.attention_dir());
    let now = chrono::Utc::now();
    match method {
        "status" => reply(&repository.status(now).map_err(fail)?),
        "start" => {
            let StartParams {
                name,
                duration_seconds,
            } = params(arguments)?;
            reply(
                &repository
                    .start_focus(&name, duration_seconds, now)
                    .map_err(fail)?,
            )
        }
        "stop" => reply(&repository.stop_focus(now).map_err(fail)?),
        "history" => {
            let mut sessions = repository.history().map_err(fail)?;
            sessions.reverse();
            reply(&sessions)
        }
        "overview" => {
            let OverviewParams { days } = params(arguments)?;
            let from = AttentionRepository::window_start(days, now);
            reply(&repository.overview(from, now).map_err(fail)?)
        }
        "settings" => reply(&repository.load_settings().map_err(fail)?),
        "saveSettings" => {
            let SaveParams { settings } = params(arguments)?;
            repository.save_settings(&settings).map_err(fail)?;
            mirror(&directories.settings_file(), &settings).map_err(fail)?;
            Ok(Value::Null)
        }
        other => Err(ErrorObject::method_not_found(other)),
    }
}
