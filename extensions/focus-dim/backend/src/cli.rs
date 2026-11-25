use std::fmt::Write as _;

use anyhow::Result;
use serde_json::json;
use veronica_core::focus_dim::{
    clamp_animation_secs, clamp_intensity, DisplayMode, FocusDimSettings, MAX_INTENSITY,
};
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::settings::Settings;
use veronica_sdk::Output;

#[derive(clap::Subcommand)]
pub(crate) enum FocusDimCommand {
    #[command(about = "Report the current settings and whether the shell can apply them")]
    Status,
    #[command(about = "Turn dimming on")]
    On,
    #[command(about = "Turn dimming off")]
    Off,
    #[command(about = "How dark to go, 0 to 90 percent")]
    Intensity {
        #[arg(help = "A percentage. Values outside the range are clamped, not rejected")]
        percent: f64,
    },
    #[command(about = "How long the fade takes, in seconds")]
    Fade { seconds: f64 },
    #[command(about = "What to do with displays other than the focused one")]
    Mode {
        #[arg(help = "perScreenFront or dimUnfocused")]
        mode: String,
    },
}

pub(crate) async fn run(command: &FocusDimCommand, output: Output<'_>) -> Result<()> {
    let path = AppDirectories::current()?.settings_file();
    let mut settings = Settings::load(&path)?;

    let write = match command {
        FocusDimCommand::Status => None,
        FocusDimCommand::On => Some(("focusDimEnabled", json!(true))),
        FocusDimCommand::Off => Some(("focusDimEnabled", json!(false))),
        FocusDimCommand::Intensity { percent } => {
            Some(("focusDimIntensity", json!(clamp_intensity(percent / 100.0))))
        }
        FocusDimCommand::Fade { seconds } => Some((
            "focusDimAnimationDuration",
            json!(clamp_animation_secs(*seconds)),
        )),
        FocusDimCommand::Mode { mode } => {
            let parsed = DisplayMode::parse(mode);
            if parsed == DisplayMode::default() && !mode.eq_ignore_ascii_case(parsed.key()) {
                let known: Vec<&str> = DisplayMode::ALL.iter().map(|m| m.key()).collect();
                anyhow::bail!("unknown mode '{mode}'; try one of {}", known.join(", "));
            }
            Some(("focusDimOtherDisplaysMode", json!(parsed.key())))
        }
    };

    if let Some((key, value)) = write {
        settings.set(key, value);
        settings.save(&path)?;
    }

    let resolved = FocusDimSettings::read(&settings);
    let can_apply = veronica_system::detect_session().await.is_gnome;
    let document = report(&resolved, can_apply);

    output.emit(&document, || {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "focus dim is {}",
            if resolved.enabled { "on" } else { "off" }
        );
        let _ = writeln!(
            out,
            "intensity  {}%  (max {:.0}%)",
            resolved.intensity_percent(),
            (MAX_INTENSITY * 100.0).round()
        );
        let _ = writeln!(out, "fade       {}s", resolved.animation_secs);
        let _ = writeln!(out, "displays   {}", resolved.mode.title());
        if !can_apply {
            let _ = write!(
                out,
                "\nThis desktop is not GNOME, so nothing draws the overlay: only \
                 the compositor can place a dim behind another app's window."
            );
        }
        out.trim_end().to_string()
    })
}

pub(crate) fn report(resolved: &FocusDimSettings, can_apply: bool) -> serde_json::Value {
    json!({
        "enabled": resolved.enabled,
        "intensityPercent": resolved.intensity_percent(),
        "fadeSeconds": resolved.animation_secs,
        "mode": resolved.mode.key(),
        "canApply": can_apply,
        "drawnBy": "the Veronica GNOME Shell extension",
    })
}
