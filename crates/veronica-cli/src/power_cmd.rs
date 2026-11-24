use std::fmt::Write as _;

use anyhow::Result;
use chrono::Utc;
use serde_json::json;
use veronica_core::awake::{parse_duration, Awake, AwakeState};
use veronica_core::{AppDirectories, Settings};

use crate::format::{self, Output};

#[derive(clap::Subcommand)]
pub(crate) enum PowerCommand {
    #[command(about = "Report both switches, their countdowns and the battery")]
    Status,
    #[command(
        subcommand,
        name = "keep-awake",
        about = "Prevent the idle sleep timer"
    )]
    KeepAwake(SwitchCommand),
    #[command(
        subcommand,
        name = "lid-awake",
        about = "Keep running with the lid shut, which also prevents idle sleep"
    )]
    LidAwake(SwitchCommand),
}

#[derive(clap::Subcommand)]
pub(crate) enum SwitchCommand {
    #[command(about = "Turn it on, open-ended or for a while")]
    On {
        #[arg(
            long = "for",
            help = "How long to keep it on, e.g. `30m`, `2h`, `1h30m`. A bare number is minutes. Without this the session is open-ended"
        )]
        duration: Option<String>,
    },
    #[command(about = "Turn it off, cancelling any timed session")]
    Off,
}

pub(crate) fn run(
    directories: &AppDirectories,
    command: &PowerCommand,
    output: Output,
) -> Result<()> {
    let path = directories.settings_file();
    let mut settings = Settings::load(&path)?;
    let now = Utc::now().timestamp_millis();

    let switch = match command {
        PowerCommand::Status => None,
        PowerCommand::KeepAwake(action) => Some((Awake::KeepAwake, action)),
        PowerCommand::LidAwake(action) => Some((Awake::LidAwake, action)),
    };

    if let Some((switch, action)) = switch {
        if switch == Awake::LidAwake
            && matches!(action, SwitchCommand::On { .. })
            && !veronica_system::power::has_lid()
        {
            tracing::warn!(
                target: "veronica",
                "this computer has no laptop lid, so Lid Awake only prevents idle sleep"
            );
        }
        match action {
            SwitchCommand::On { duration } => {
                let until = match duration {
                    Some(raw) => Some(now + parse_duration(raw)? * 1000),
                    None => None,
                };
                settings.set(switch.enabled_key(), json!(true));
                settings.set(switch.until_key(), json!(until));
            }
            SwitchCommand::Off => {
                settings.set(switch.enabled_key(), json!(false));
                settings.set(switch.until_key(), json!(null));
            }
        }
        settings.save(&path)?;
    }

    let keep = AwakeState::read(&settings, Awake::KeepAwake);
    let lid = AwakeState::read(&settings, Awake::LidAwake);
    let snapshot = veronica_system::MetricsSampler::new().sample();

    let describe = |state: &AwakeState| -> serde_json::Value {
        json!({
            "enabled": state.enabled,
            "active": state.is_active(now),
            "untilMs": state.until_ms,
            "remainingSeconds": state.remaining_secs(now),
        })
    };

    let document = json!({
        "keepAwake": describe(&keep),
        "lidAwake": describe(&lid),
        "hasLid": veronica_system::power::has_lid(),
        "battery": snapshot.battery,
        "heldBy": "the running Veronica app, as a systemd-logind inhibitor",
    });

    output.emit(&document, || {
        let line = |state: &AwakeState| -> String {
            if !state.is_active(now) {
                return "off".to_string();
            }
            match state.remaining_secs(now) {
                Some(seconds) => format!("on, {} left", format::countdown(seconds)),
                None => "on".to_string(),
            }
        };
        let mut out = String::new();
        let _ = writeln!(out, "Keep Awake  {}", line(&keep));
        let _ = write!(out, "Lid Awake   {}", line(&lid));
        if !veronica_system::power::has_lid() {
            let _ = write!(out, "  (no laptop lid on this computer)");
        }
        if let Some(battery) = &snapshot.battery {
            let _ = write!(
                out,
                "\nBattery     {:.0}%{}",
                battery.percent,
                if battery.charging { " · charging" } else { "" }
            );
        }
        out
    })
}
