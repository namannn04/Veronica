use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use veronica_core::swatches::{srgb_to_display_p3, ColorProfile, CopyFormat, Swatch};
use veronica_sdk::protocol::rpc::{error_code, ErrorObject};
use veronica_sdk::rpc::{failure, params, reply};

use crate::preferences::Preferences;

pub(crate) const METHODS: [&str; 6] = ["formats", "swatches", "pick", "copy", "forget", "clear"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SwatchRow {
    pub id: u64,
    pub hex: String,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub profile: &'static str,
    pub profile_label: &'static str,
    pub picked_at: String,
    pub formats: BTreeMap<&'static str, String>,
    pub prefers_dark_text: bool,
}

pub(crate) fn swatch_row(swatch: &Swatch) -> SwatchRow {
    SwatchRow {
        id: swatch.id,
        hex: swatch.hex(),
        red: swatch.red,
        green: swatch.green,
        blue: swatch.blue,
        profile: swatch.profile.key(),
        profile_label: swatch.profile.title(),
        picked_at: swatch.picked_at.to_rfc3339(),
        formats: CopyFormat::ALL
            .iter()
            .map(|format| (format.key(), swatch.format(*format)))
            .collect(),
        prefers_dark_text: swatch.prefers_dark_text(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FormatOption {
    id: &'static str,
    label: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PickResult {
    pub swatch: SwatchRow,
    pub value: String,
    pub format: &'static str,
    pub source: &'static str,
    pub copied_via: Option<&'static str>,
    pub copy_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CopyResult {
    value: String,
    format: &'static str,
    copied_via: &'static str,
}

#[derive(Deserialize)]
struct CopyParams {
    id: u64,
    #[serde(default)]
    format: Option<String>,
}

#[derive(Deserialize)]
struct SwatchParams {
    id: u64,
}

fn missing(id: u64) -> ErrorObject {
    ErrorObject::new(error_code::INVALID_PARAMS, format!("no swatch {id}"))
}

async fn pick() -> anyhow::Result<PickResult> {
    let preferences = Preferences::load()?;
    let picked = veronica_system::color::pick().await?;
    let (red, green, blue) = match preferences.profile {
        ColorProfile::Srgb => (picked.red, picked.green, picked.blue),
        ColorProfile::DisplayP3 => srgb_to_display_p3(picked.red, picked.green, picked.blue),
    };
    let mut history = Preferences::history(&preferences.path)?;
    let swatch = history.record(
        red,
        green,
        blue,
        preferences.profile,
        chrono::Utc::now(),
        preferences.limit,
    );
    history.save(&preferences.path)?;
    let value = swatch.format(preferences.format);
    let (copied_via, copy_error) = match veronica_system::selection::write(&value).await {
        Ok(writer) => (Some(writer.title()), None),
        Err(error) => (None, Some(format!("{error:#}"))),
    };
    Ok(PickResult {
        swatch: swatch_row(&swatch),
        value,
        format: preferences.format.key(),
        source: picked.source.title(),
        copied_via,
        copy_error,
    })
}

pub(crate) async fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    let fail = |error: anyhow::Error| failure(&error);
    match method {
        "formats" => reply(
            &CopyFormat::ALL
                .iter()
                .map(|format| FormatOption {
                    id: format.key(),
                    label: format.title(),
                })
                .collect::<Vec<_>>(),
        ),
        "swatches" => {
            let preferences = Preferences::load().map_err(fail)?;
            let history = Preferences::history(&preferences.path).map_err(fail)?;
            reply(
                &history
                    .swatches()
                    .iter()
                    .map(swatch_row)
                    .collect::<Vec<_>>(),
            )
        }
        "pick" => reply(&pick().await.map_err(fail)?),
        "copy" => {
            let CopyParams { id, format } = params(arguments)?;
            let preferences = Preferences::load().map_err(fail)?;
            let chosen = format
                .as_deref()
                .map_or(preferences.format, CopyFormat::parse);
            let history = Preferences::history(&preferences.path).map_err(fail)?;
            let swatch = history.get(id).ok_or_else(|| missing(id))?;
            let value = swatch.format(chosen);
            let writer = veronica_system::selection::write(&value)
                .await
                .map_err(fail)?;
            reply(&CopyResult {
                value,
                format: chosen.key(),
                copied_via: writer.title(),
            })
        }
        "forget" => {
            let SwatchParams { id } = params(arguments)?;
            let preferences = Preferences::load().map_err(fail)?;
            let mut history = Preferences::history(&preferences.path).map_err(fail)?;
            if !history.remove(id) {
                return Err(missing(id));
            }
            history.save(&preferences.path).map_err(fail)?;
            Ok(Value::Null)
        }
        "clear" => {
            let preferences = Preferences::load().map_err(fail)?;
            let mut history = Preferences::history(&preferences.path).map_err(fail)?;
            history.clear();
            history.save(&preferences.path).map_err(fail)?;
            Ok(Value::Null)
        }
        other => Err(ErrorObject::method_not_found(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swatch_rows_are_camel_case() {
        let swatch = Swatch {
            id: 1,
            red: 0.2,
            green: 0.4,
            blue: 0.6,
            profile: ColorProfile::DisplayP3,
            picked_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(swatch_row(&swatch)).unwrap();
        assert_eq!(json["hex"], "#336699");
        assert_eq!(json["profile"], "displayP3");
        assert!(json["prefersDarkText"].is_boolean());
        assert!(json["profileLabel"].is_string());
        assert!(json["pickedAt"].is_string());
        assert!(json["formats"]["gdkRgba"].is_string());
    }
}
