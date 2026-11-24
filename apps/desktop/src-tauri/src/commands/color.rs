use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::{fail, CommandResult};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SwatchRow {
    pub id: u64,
    pub hex: String,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub profile: String,
    pub profile_label: &'static str,
    pub picked_at: String,
    pub formats: std::collections::BTreeMap<&'static str, String>,
    pub prefers_dark_text: bool,
}

fn swatch_row(swatch: &veronica_core::Swatch) -> SwatchRow {
    use veronica_core::CopyFormat;
    SwatchRow {
        id: swatch.id,
        hex: swatch.hex(),
        red: swatch.red,
        green: swatch.green,
        blue: swatch.blue,
        profile: swatch.profile.key().to_string(),
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
pub(crate) struct FormatOption {
    pub id: &'static str,
    pub label: &'static str,
}

#[tauri::command]
pub(crate) fn color_formats() -> Vec<FormatOption> {
    veronica_core::CopyFormat::ALL
        .iter()
        .map(|format| FormatOption {
            id: format.key(),
            label: format.title(),
        })
        .collect()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn color_swatches(state: State<'_, AppState>) -> CommandResult<Vec<SwatchRow>> {
    let history =
        veronica_core::SwatchHistory::load(&state.directories.swatches_file()).map_err(fail)?;
    Ok(history.swatches().iter().map(swatch_row).collect())
}

#[tauri::command]
pub(crate) async fn color_pick(app: AppHandle) -> CommandResult<PickResult> {
    use veronica_core::swatches::{srgb_to_display_p3, ColorProfile, CopyFormat, SwatchHistory};

    let (path, format, profile, limit) = {
        let state = app.state::<AppState>();
        let settings = state.settings_snapshot();
        (
            state.directories.swatches_file(),
            CopyFormat::parse(settings.string("colorPickerCopyFormat").unwrap_or("hex")),
            ColorProfile::parse(settings.string("colorPickerProfile").unwrap_or("srgb")),
            SwatchHistory::clamp_limit(
                settings
                    .get("colorPickerHistorySize")
                    .and_then(serde_json::Value::as_u64)
                    .map_or(veronica_core::swatches::DEFAULT_HISTORY_SIZE, |size| {
                        usize::try_from(size).unwrap_or(usize::MAX)
                    }),
            ),
        )
    };

    let picked = veronica_system::color::pick().await.map_err(fail)?;
    let (red, green, blue) = match profile {
        ColorProfile::Srgb => (picked.red, picked.green, picked.blue),
        ColorProfile::DisplayP3 => srgb_to_display_p3(picked.red, picked.green, picked.blue),
    };

    let mut history = SwatchHistory::load(&path).map_err(fail)?;
    let swatch = history.record(red, green, blue, profile, chrono::Utc::now(), limit);
    history.save(&path).map_err(fail)?;

    let value = swatch.format(format);
    let (copied_via, copy_error) = match veronica_system::selection::write(&value).await {
        Ok(writer) => (Some(writer.title()), None),
        Err(error) => (None, Some(format!("{error:#}"))),
    };

    let _ = app.emit("swatches-updated", swatch.id);
    Ok(PickResult {
        swatch: swatch_row(&swatch),
        value,
        format: format.key(),
        source: picked.source.title(),
        copied_via,
        copy_error,
    })
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

#[tauri::command]
pub(crate) async fn color_copy(
    app: AppHandle,
    id: u64,
    format: Option<String>,
) -> CommandResult<CopyResult> {
    use veronica_core::CopyFormat;

    let (path, configured) = {
        let state = app.state::<AppState>();
        let settings = state.settings_snapshot();
        (
            state.directories.swatches_file(),
            CopyFormat::parse(settings.string("colorPickerCopyFormat").unwrap_or("hex")),
        )
    };
    let chosen = format.as_deref().map_or(configured, CopyFormat::parse);

    let history = veronica_core::SwatchHistory::load(&path).map_err(fail)?;
    let swatch = history.get(id).ok_or_else(|| format!("no swatch {id}"))?;
    let value = swatch.format(chosen);
    let writer = veronica_system::selection::write(&value)
        .await
        .map_err(fail)?;
    Ok(CopyResult {
        value,
        format: chosen.key(),
        copied_via: writer.title(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CopyResult {
    pub value: String,
    pub format: &'static str,
    pub copied_via: &'static str,
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn color_forget(state: State<'_, AppState>, id: u64) -> CommandResult<()> {
    let path = state.directories.swatches_file();
    let mut history = veronica_core::SwatchHistory::load(&path).map_err(fail)?;
    if !history.remove(id) {
        return Err(format!("no swatch {id}"));
    }
    history.save(&path).map_err(fail)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn color_clear(state: State<'_, AppState>) -> CommandResult<()> {
    let path = state.directories.swatches_file();
    let mut history = veronica_core::SwatchHistory::load(&path).map_err(fail)?;
    history.clear();
    history.save(&path).map_err(fail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::assert_camel_case;

    #[test]
    fn the_swatch_row_is_camel_case_throughout() {
        let swatch = veronica_core::Swatch {
            id: 1,
            red: 0.2,
            green: 0.4,
            blue: 0.6,
            profile: veronica_core::ColorProfile::DisplayP3,
            picked_at: chrono::Utc::now(),
        };
        let row = swatch_row(&swatch);
        assert_camel_case(&row, "SwatchRow");

        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(json["hex"], "#336699");
        assert!(json["prefersDarkText"].is_boolean());
        assert!(json["profileLabel"].is_string());
        assert!(json["pickedAt"].is_string());
        assert!(json["formats"]["gdkRgba"].is_string());
    }

    #[test]
    fn the_pick_result_is_camel_case_throughout() {
        let result = PickResult {
            swatch: swatch_row(&veronica_core::Swatch {
                id: 1,
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                profile: veronica_core::ColorProfile::Srgb,
                picked_at: chrono::Utc::now(),
            }),
            value: "#000000".into(),
            format: "hex",
            source: "GNOME Shell",
            copied_via: Some("wl-copy"),
            copy_error: None,
        };
        assert_camel_case(&result, "PickResult");
        let json = serde_json::to_value(&result).unwrap();
        assert!(json["copiedVia"].is_string());
        assert!(json["copyError"].is_null());
        assert!(json["swatch"]["prefersDarkText"].is_boolean());
    }
}
