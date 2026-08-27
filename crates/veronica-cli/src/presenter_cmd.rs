//! `vr presenter` — the blur that hides figures on a shared screen.
//!
//! Mirrors Edith's `presenter status|start|stop`, plus the two actions Linux
//! needs: enabling the feature, and dismissing one detected share.
//!
//! `status` also reports what detection currently sees, which is the thing to
//! look at when presenter mode did not activate during a call.

use anyhow::Result;
use serde_json::json;
use veronica_core::{AppDirectories, BlurCategory, PresenterState, Settings};

use crate::format::Output;

#[derive(clap::Subcommand)]
pub enum PresenterCommand {
    /// Report the resolved state and what detection sees.
    Status,
    /// Blur now.
    Start,
    /// Stop blurring. Does not disable detection.
    Stop,
    /// Turn the feature on.
    Enable,
    /// Turn the feature off, so nothing blurs and no detection runs.
    Disable,
    /// Ignore the share happening now. Cleared when it ends.
    Dismiss,
    /// Undo a dismissal.
    Resume,
    /// Choose which categories are blurred.
    Blur {
        /// money, usage, agents, calendar or music.
        category: String,
        /// `on` or `off`.
        ///
        /// The explicit action is required: clap would otherwise treat a
        /// positional bool as a flag and refuse to take a value.
        #[arg(value_parser = parse_switch, action = clap::ArgAction::Set)]
        value: bool,
    },
}

fn parse_switch(raw: &str) -> Result<bool, String> {
    match raw.to_lowercase().as_str() {
        "on" | "true" | "yes" | "1" => Ok(true),
        "off" | "false" | "no" | "0" => Ok(false),
        other => Err(format!("expected on or off, got '{other}'")),
    }
}

fn category(raw: &str) -> Result<BlurCategory> {
    BlurCategory::ALL
        .into_iter()
        .find(|category| {
            format!("{category:?}").eq_ignore_ascii_case(raw)
                || category.key().eq_ignore_ascii_case(raw)
        })
        .ok_or_else(|| {
            let known: Vec<String> = BlurCategory::ALL
                .iter()
                .map(|c| format!("{c:?}").to_lowercase())
                .collect();
            anyhow::anyhow!("unknown category '{raw}'; try one of {}", known.join(", "))
        })
}

pub async fn run(
    directories: &AppDirectories,
    command: &PresenterCommand,
    output: Output,
) -> Result<()> {
    let path = directories.settings_file();
    let mut settings = Settings::load(&path)?;

    // The write commands all set one key. Reporting happens afterwards from the
    // re-read state, so the output is always what is now stored rather than what
    // was asked for.
    let write = match command {
        PresenterCommand::Status => None,
        PresenterCommand::Start => Some(("presenterMode", json!(true))),
        PresenterCommand::Stop => Some(("presenterMode", json!(false))),
        PresenterCommand::Enable => Some(("presenterEnabled", json!(true))),
        PresenterCommand::Disable => Some(("presenterEnabled", json!(false))),
        PresenterCommand::Dismiss => Some(("presenterAutoPaused", json!(true))),
        PresenterCommand::Resume => Some(("presenterAutoPaused", json!(false))),
        PresenterCommand::Blur { category: raw, value } => {
            Some((category(raw)?.key(), json!(*value)))
        }
    };

    if let Some((key, value)) = write {
        // Edith refuses start/stop while the feature is disabled rather than
        // storing a flag that does nothing. Same here, with a message that says
        // what to do about it.
        if matches!(command, PresenterCommand::Start | PresenterCommand::Stop)
            && !settings.bool_or("presenterEnabled", false)
        {
            anyhow::bail!(
                "presenter mode is not enabled; run `vr presenter enable` first"
            );
        }
        settings.set(key, value);
        settings.save(&path)?;
    }

    let state = PresenterState::read(&settings);
    let share = if state.enabled {
        veronica_system::screencast::detect().await
    } else {
        veronica_system::screencast::ScreenShareState::default()
    };

    let document = json!({
        "enabled": state.enabled,
        "manual": state.manual,
        "autoEnabled": state.auto_enabled,
        "autoActive": state.auto_active,
        "autoPaused": state.auto_paused,
        "autoReason": state.auto_reason,
        "active": state.active(),
        "blurred": BlurCategory::ALL
            .into_iter()
            .filter(|category| state.blurs(*category))
            .map(|category| format!("{category:?}").to_lowercase())
            .collect::<Vec<_>>(),
        "share": share,
    });

    output.emit(&document, || {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "presenter is {}",
            if !state.enabled {
                "disabled (vr presenter enable)"
            } else if state.active() {
                "ON"
            } else {
                "enabled but not blurring"
            }
        );
        if state.enabled {
            let _ = writeln!(out, "manual     {}", on_off(state.manual));
            let _ = writeln!(
                out,
                "detection  {}{}",
                on_off(state.auto_enabled),
                if state.auto_paused { " (this share dismissed)" } else { "" }
            );
            let _ = writeln!(
                out,
                "screen     {}",
                share
                    .unavailable
                    .clone()
                    .or_else(|| share.reason.clone())
                    .unwrap_or_else(|| "nothing is capturing the screen".into())
            );
            let blurred: Vec<&str> = BlurCategory::ALL
                .into_iter()
                .filter(|category| state.categories.contains(category))
                .map(|category| category.title())
                .collect();
            let _ = write!(out, "covers     {}", blurred.join(", "));
        }
        out.trim_end().to_string()
    })
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}
