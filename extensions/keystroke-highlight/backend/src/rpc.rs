use serde::Deserialize;
use serde_json::{json, Value};
use veronica_core::keystroke::{clamp_duration, KeystrokeHighlightSettings, Position};
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::protocol::rpc::{error_code, ErrorObject};
use veronica_sdk::rpc::{failure, params};
use veronica_sdk::settings::Settings;

use crate::cli::report;

pub(crate) const METHODS: [&str; 2] = ["status", "update"];

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Update {
    active: Option<bool>,
    duration_seconds: Option<f64>,
    position: Option<String>,
}

pub(crate) fn apply(settings: &mut Settings, update: &Update) -> Result<(), String> {
    if let Some(active) = update.active {
        settings.set("keystrokeHighlightActive", json!(active));
    }
    if let Some(seconds) = update.duration_seconds {
        settings.set("keystrokeHighlightDuration", json!(clamp_duration(seconds)));
    }
    if let Some(position) = &update.position {
        let parsed = Position::parse(position);
        if !position.eq_ignore_ascii_case(parsed.key()) {
            return Err(format!("unknown position '{position}'"));
        }
        settings.set("keystrokeHighlightPosition", json!(parsed.key()));
    }
    Ok(())
}

pub(crate) async fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    let fail = |error: anyhow::Error| failure(&error);
    let path = AppDirectories::current().map_err(fail)?.settings_file();
    let mut settings = Settings::load(&path).map_err(fail)?;
    match method {
        "status" => {}
        "update" => {
            let update: Update = params(arguments)?;
            apply(&mut settings, &update)
                .map_err(|message| ErrorObject::new(error_code::INVALID_PARAMS, message))?;
            settings.save(&path).map_err(fail)?;
        }
        other => return Err(ErrorObject::method_not_found(other)),
    }
    let can_apply = veronica_system::detect_session().await.is_gnome;
    Ok(report(
        &KeystrokeHighlightSettings::read(&settings),
        can_apply,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_write_the_keys_the_shell_reads() {
        let mut settings = Settings::default();
        let update = Update {
            active: Some(true),
            duration_seconds: Some(2.0),
            position: Some("top".into()),
        };
        apply(&mut settings, &update).unwrap();
        let resolved = KeystrokeHighlightSettings::read(&settings);
        assert!(resolved.active);
        assert_eq!(resolved.position, Position::parse("top"));
        let wrong = Update {
            position: Some("middle".into()),
            ..Update::default()
        };
        assert!(apply(&mut settings, &wrong).is_err());
    }
}
