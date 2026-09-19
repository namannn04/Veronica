//! `vr keystroke-highlight` — show each key press on screen for demos.
//!
//! The overlay is drawn by Veronica's GNOME Shell extension, because on Wayland
//! no client may observe key presses meant for another window. This writes the
//! settings the extension watches and reports whether the extension is there to
//! act on them — a switch that appears to work while nothing happens is worse
//! than one that says why.
//!
//! `on`/`off` map to Edith's `keystrokeHighlightActive`, and `enable`/`disable`
//! to the extension switch. Pausing keeps the extension enabled, which is what
//! lets the shortcut stay live between takes.

use anyhow::Result;
use serde_json::json;
use veronica_core::keystroke::{
    clamp_duration, KeystrokeHighlightSettings, Position, MAXIMUM_VISIBLE, MAX_DURATION_SECS,
    MIN_DURATION_SECS,
};
use veronica_core::{AppDirectories, Settings};

use crate::format::Output;

#[derive(clap::Subcommand)]
pub enum KeystrokeCommand {
    /// Report the current settings and whether the shell can apply them.
    Status,
    /// Start the overlay.
    On,
    /// Pause the overlay, keeping the extension enabled.
    Off,
    /// Enable the extension.
    Enable,
    /// Disable the extension, which also stops the overlay.
    Disable,
    /// How long a keycap stays on screen, in seconds.
    Duration {
        /// 0.5 to 3. Values outside the range are clamped, not rejected.
        seconds: f64,
    },
    /// Where the row of keycaps sits.
    Position {
        /// top or bottom.
        position: String,
    },
}

pub async fn run(
    directories: &AppDirectories,
    command: &KeystrokeCommand,
    output: Output,
) -> Result<()> {
    let path = directories.settings_file();
    let mut settings = Settings::load(&path)?;

    let write = match command {
        KeystrokeCommand::Status => None,
        KeystrokeCommand::On => Some(vec![
            // Starting the overlay from a paused-and-disabled state should
            // start it, not silently do nothing, so this enables too.
            ("keystrokeHighlightEnabled", json!(true)),
            ("keystrokeHighlightActive", json!(true)),
        ]),
        KeystrokeCommand::Off => Some(vec![("keystrokeHighlightActive", json!(false))]),
        KeystrokeCommand::Enable => Some(vec![("keystrokeHighlightEnabled", json!(true))]),
        KeystrokeCommand::Disable => Some(vec![("keystrokeHighlightEnabled", json!(false))]),
        KeystrokeCommand::Duration { seconds } => {
            // Clamped rather than refused: the useful reading of `duration 10`
            // is "as long as it goes", not an error.
            Some(vec![(
                "keystrokeHighlightDuration",
                json!(clamp_duration(*seconds)),
            )])
        }
        KeystrokeCommand::Position { position } => {
            let parsed = Position::parse(position);
            // `parse` falls back for an unknown value, which is right for a
            // stored setting but wrong for an explicit argument.
            if parsed == Position::default() && !position.eq_ignore_ascii_case(parsed.key()) {
                let known: Vec<&str> = Position::ALL.iter().map(|p| p.key()).collect();
                anyhow::bail!(
                    "unknown position '{position}'; try one of {}",
                    known.join(", ")
                );
            }
            Some(vec![("keystrokeHighlightPosition", json!(parsed.key()))])
        }
    };

    if let Some(pairs) = write {
        for (key, value) in pairs {
            settings.set(key, value);
        }
        settings.save(&path)?;
    }

    let resolved = KeystrokeHighlightSettings::read(&settings);
    let session = veronica_system::detect_session().await;
    // The extension is what reads the presses and draws the caps, so its
    // absence is the one thing worth reporting alongside the settings.
    let can_apply = session.is_gnome;

    let document = json!({
        "enabled": resolved.enabled,
        "active": resolved.active,
        "running": resolved.is_running(),
        "durationSeconds": resolved.duration_secs,
        "position": resolved.position.key(),
        "maximumVisible": MAXIMUM_VISIBLE,
        "canApply": can_apply,
        "drawnBy": "the Veronica GNOME Shell extension",
    });

    output.emit(&document, || {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "keystroke highlight is {}",
            match (resolved.enabled, resolved.active) {
                (false, _) => "disabled",
                (true, false) => "enabled, paused",
                (true, true) => "running",
            }
        );
        let _ = writeln!(
            out,
            "duration   {}s  ({MIN_DURATION_SECS} to {MAX_DURATION_SECS})",
            resolved.duration_secs
        );
        let _ = writeln!(out, "position   {}", resolved.position.title());
        let _ = write!(out, "visible    at most {MAXIMUM_VISIBLE} keycaps");
        if !can_apply {
            let _ = write!(
                out,
                "\n\nThis desktop is not GNOME, so nothing reads the presses: a Wayland \
                 compositor hands key presses only to the focused window."
            );
        }
        out.trim_end().to_string()
    })
}
