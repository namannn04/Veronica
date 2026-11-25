use serde::Deserialize;
use serde_json::{json, Value};
use veronica_core::focus_dim::{
    clamp_animation_secs, clamp_intensity, DisplayMode, FocusDimSettings,
};
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::protocol::rpc::{error_code, ErrorObject};
use veronica_sdk::rpc::{failure, params};
use veronica_sdk::settings::Settings;

use crate::cli::report;

pub(crate) const METHODS: [&str; 2] = ["status", "update"];

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Update {
    enabled: Option<bool>,
    intensity_percent: Option<f64>,
    fade_seconds: Option<f64>,
    mode: Option<String>,
}

pub(crate) fn apply(settings: &mut Settings, update: &Update) -> Result<(), String> {
    if let Some(enabled) = update.enabled {
        settings.set("focusDimEnabled", json!(enabled));
    }
    if let Some(percent) = update.intensity_percent {
        settings.set("focusDimIntensity", json!(clamp_intensity(percent / 100.0)));
    }
    if let Some(seconds) = update.fade_seconds {
        settings.set(
            "focusDimAnimationDuration",
            json!(clamp_animation_secs(seconds)),
        );
    }
    if let Some(mode) = &update.mode {
        let parsed = DisplayMode::parse(mode);
        if !mode.eq_ignore_ascii_case(parsed.key()) {
            return Err(format!("unknown mode '{mode}'"));
        }
        settings.set("focusDimOtherDisplaysMode", json!(parsed.key()));
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
    Ok(report(&FocusDimSettings::read(&settings), can_apply))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_write_the_keys_the_shell_reads() {
        let mut settings = Settings::default();
        let update = Update {
            enabled: Some(true),
            intensity_percent: Some(60.0),
            fade_seconds: Some(0.5),
            mode: Some("dimUnfocused".into()),
        };
        apply(&mut settings, &update).unwrap();
        let resolved = FocusDimSettings::read(&settings);
        assert!(resolved.enabled);
        assert_eq!(resolved.intensity_percent(), 60);
        assert_eq!(resolved.mode, DisplayMode::DimUnfocused);
        let wrong = Update {
            mode: Some("sideways".into()),
            ..Update::default()
        };
        assert!(apply(&mut settings, &wrong).is_err());
    }
}
