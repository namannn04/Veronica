//! `vr color` — the screen colour sampler and its swatch history.
//!
//! Edith exposes `color pick` and `color copy`; this adds the list, forget and
//! clear that the history needs on a machine where the swatches are a file
//! rather than a user default.
//!
//! `pick` is interactive by nature: it opens the compositor's eyedropper and
//! waits for a click. Cancelling exits non-zero with a plain message, so a
//! script can tell "no colour chosen" from "no picker available".

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use veronica_core::swatches::{
    formatting, srgb_to_display_p3, ColorProfile, CopyFormat, SwatchHistory, DEFAULT_HISTORY_SIZE,
};
use veronica_core::{AppDirectories, Settings};

use crate::format::{self, Output};

#[derive(clap::Subcommand)]
pub enum ColorCommand {
    /// Open the screen colour sampler and record what is picked.
    Pick {
        /// Which representation to copy. Defaults to the configured format.
        #[arg(long, value_parser = parse_format)]
        format: Option<CopyFormat>,
        /// Record the colour without putting it on the clipboard.
        #[arg(long)]
        no_copy: bool,
    },
    /// Print the swatch history, newest first.
    #[command(alias = "ls")]
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Copy one swatch back to the clipboard.
    Copy {
        id: u64,
        #[arg(long, value_parser = parse_format)]
        format: Option<CopyFormat>,
    },
    /// Print one swatch in every format, for piping onward.
    Show { id: u64 },
    /// Forget one swatch.
    #[command(alias = "rm")]
    Remove { id: u64 },
    /// Forget every swatch.
    Clear,
}

fn parse_format(raw: &str) -> Result<CopyFormat, String> {
    let parsed = CopyFormat::parse(raw);
    // `parse` falls back to hex, which is right for a stored setting but wrong
    // for an explicit flag: a typo should be reported, not silently ignored.
    if parsed == CopyFormat::Hex && !raw.eq_ignore_ascii_case("hex") {
        let known: Vec<&str> = CopyFormat::ALL.iter().map(|f| f.key()).collect();
        return Err(format!(
            "unknown format '{raw}'; try one of {}",
            known.join(", ")
        ));
    }
    Ok(parsed)
}

pub async fn run(
    directories: &AppDirectories,
    settings: &Settings,
    command: &ColorCommand,
    output: Output,
) -> Result<()> {
    let path = directories.swatches_file();
    let mut history = SwatchHistory::load(&path)?;
    let configured = CopyFormat::parse(settings.string("colorPickerCopyFormat").unwrap_or("hex"));
    let profile = ColorProfile::parse(settings.string("colorPickerProfile").unwrap_or("srgb"));
    let limit = SwatchHistory::clamp_limit(
        settings
            .get("colorPickerHistorySize")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_HISTORY_SIZE as u64) as usize,
    );

    match command {
        ColorCommand::Pick { format, no_copy } => {
            let format = format.unwrap_or(configured);
            let picked = veronica_system::color::pick()
                .await
                .context("no colour was sampled")?;

            // The compositor always reports sRGB; converting is what makes the
            // profile setting mean something rather than relabel the numbers.
            let (red, green, blue) = match profile {
                ColorProfile::Srgb => (picked.red, picked.green, picked.blue),
                ColorProfile::DisplayP3 => {
                    srgb_to_display_p3(picked.red, picked.green, picked.blue)
                }
            };

            let swatch = history.record(red, green, blue, profile, Utc::now(), limit);
            history.save(&path)?;

            let value = swatch.format(format);
            let copied = if *no_copy {
                None
            } else {
                match veronica_system::selection::write(&value).await {
                    Ok(writer) => Some(writer.title()),
                    Err(error) => {
                        // The swatch is already recorded, so a clipboard failure
                        // must not lose the colour the user just picked.
                        tracing::warn!(target: "veronica", "cannot copy the colour: {error:#}");
                        None
                    }
                }
            };

            output.emit(
                &json!({
                    "id": swatch.id,
                    "hex": swatch.hex(),
                    "value": value,
                    "format": format.key(),
                    "profile": profile.key(),
                    "red": swatch.red,
                    "green": swatch.green,
                    "blue": swatch.blue,
                    "source": picked.source.title(),
                    "copiedVia": copied,
                }),
                || match copied {
                    Some(via) => format!("{value}  (copied via {via})"),
                    None => value.clone(),
                },
            )
        }

        ColorCommand::List { limit } => {
            let rows: Vec<_> = history.swatches().iter().take(*limit).cloned().collect();
            output.emit(&rows, || {
                if rows.is_empty() {
                    return "no colours picked yet".to_string();
                }
                let table: Vec<Vec<String>> = rows
                    .iter()
                    .map(|swatch| {
                        vec![
                            swatch.id.to_string(),
                            swatch.hex(),
                            formatting::rgb(swatch.red, swatch.green, swatch.blue),
                            swatch.profile.title().to_string(),
                            swatch.picked_at.to_rfc3339(),
                        ]
                    })
                    .collect();
                format::table(&["id", "hex", "rgb", "profile", "picked"], &table)
            })
        }

        ColorCommand::Copy { id, format } => {
            let format = format.unwrap_or(configured);
            let swatch = history
                .get(*id)
                .with_context(|| format!("no swatch {id}"))?;
            let value = swatch.format(format);
            let writer = veronica_system::selection::write(&value).await?;
            output.emit(
                &json!({ "id": id, "value": value, "copiedVia": writer.title() }),
                || format!("{value}  (copied via {})", writer.title()),
            )
        }

        ColorCommand::Show { id } => {
            let swatch = history
                .get(*id)
                .with_context(|| format!("no swatch {id}"))?;
            let every: Vec<(&str, String)> = CopyFormat::ALL
                .iter()
                .map(|format| (format.key(), swatch.format(*format)))
                .collect();
            let document = json!({
                "id": swatch.id,
                "profile": swatch.profile.key(),
                "pickedAt": swatch.picked_at.to_rfc3339(),
                "red": swatch.red,
                "green": swatch.green,
                "blue": swatch.blue,
                "formats": every
                    .iter()
                    .map(|(key, value)| {
                        ((*key).to_string(), serde_json::Value::String(value.clone()))
                    })
                    .collect::<serde_json::Map<String, serde_json::Value>>(),
            });
            output.emit(&document, || {
                every
                    .iter()
                    .map(|(key, value)| format!("{key:>8}  {value}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }

        ColorCommand::Remove { id } => {
            if !history.remove(*id) {
                anyhow::bail!("no swatch {id}");
            }
            history.save(&path)?;
            output.emit(&json!({ "removed": id }), || format!("removed {id}"))
        }

        ColorCommand::Clear => {
            let count = history.len();
            history.clear();
            history.save(&path)?;
            output.emit(&json!({ "cleared": count }), || {
                format!("cleared {count} swatches")
            })
        }
    }
}
