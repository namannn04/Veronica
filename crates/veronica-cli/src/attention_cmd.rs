//! `vr attention` — durable focus timers, usable with the desktop app closed.

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use veronica_core::{AppDirectories, AttentionPrivacy, AttentionRepository, Settings};

use crate::format::{countdown, table, Output};

#[derive(clap::Subcommand)]
pub enum AttentionCommand {
    /// Show the active timer and completed focus total.
    Status,
    /// Start, stop or inspect a focus session.
    #[command(subcommand)]
    Focus(FocusCommand),
    /// List completed focus sessions, newest first.
    History {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Summarise application activity for recent days.
    Summary {
        #[arg(long, default_value_t = 1)]
        days: i64,
    },
    /// Enable/disable tracking and choose its privacy level.
    Configure {
        #[arg(long)]
        enabled: Option<bool>,
        #[arg(long, value_parser = ["applications", "detailed"])]
        privacy: Option<String>,
        #[arg(long)]
        idle_seconds: Option<i64>,
    },
    /// Record one compositor pulse. Used by Veronica's GNOME extension.
    #[command(hide = true)]
    Record {
        #[arg(long)]
        application: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        idle: bool,
        #[arg(long, default_value_t = 10)]
        seconds: i64,
    },
}

#[derive(clap::Subcommand)]
pub enum FocusCommand {
    /// Show the current focus session.
    Status,
    /// Start a timer such as `--for 45m` or `--for 2h`.
    Start {
        #[arg(long, default_value = "Focus")]
        name: String,
        #[arg(long = "for", default_value = "45m", value_parser = parse_duration)]
        duration_seconds: i64,
    },
    /// Finish the active session and preserve it in history.
    Stop,
}

pub fn run(directories: &AppDirectories, command: &AttentionCommand, output: Output) -> Result<()> {
    let repository = AttentionRepository::new(directories.attention_dir());
    let now = Utc::now();
    match command {
        AttentionCommand::Status => {
            let status = repository.status(now)?;
            output.emit(&status, || match &status.active {
                Some(active) => format!(
                    "focusing on {} · {} remaining\n{} completed sessions · {} total",
                    active.name,
                    signed_remaining(active.remaining_seconds(now)),
                    status.completed_sessions,
                    countdown(status.total_focus_seconds)
                ),
                None => format!(
                    "no active focus session\n{} completed sessions · {} total",
                    status.completed_sessions,
                    countdown(status.total_focus_seconds)
                ),
            })
        }
        AttentionCommand::Focus(FocusCommand::Status) => {
            let active = repository.active_focus()?;
            output.emit(&active, || match &active {
                Some(active) => format!(
                    "{} · {} remaining (started {})",
                    active.name,
                    signed_remaining(active.remaining_seconds(now)),
                    active.started_at.to_rfc3339()
                ),
                None => "no focus session is active".to_string(),
            })
        }
        AttentionCommand::Focus(FocusCommand::Start {
            name,
            duration_seconds,
        }) => {
            let session = repository.start_focus(name, *duration_seconds, now)?;
            output.emit(&session, || {
                format!(
                    "started {} for {}",
                    session.name,
                    countdown(*duration_seconds)
                )
            })
        }
        AttentionCommand::Focus(FocusCommand::Stop) => {
            let session = repository.stop_focus(now)?;
            output.emit(&session, || {
                format!(
                    "finished {} after {}",
                    session.name,
                    countdown(session.actual_duration_seconds(now))
                )
            })
        }
        AttentionCommand::History { limit } => {
            let mut history = repository.history()?;
            history.reverse();
            history.truncate(*limit);
            output.emit(&history, || {
                let rows = history
                    .iter()
                    .map(|session| {
                        vec![
                            session.started_at.format("%Y-%m-%d %H:%M").to_string(),
                            session.name.clone(),
                            countdown(session.actual_duration_seconds(now)),
                        ]
                    })
                    .collect::<Vec<_>>();
                if rows.is_empty() {
                    "no completed focus sessions".to_string()
                } else {
                    table(&["started", "name", "duration"], &rows)
                }
            })
        }
        AttentionCommand::Summary { days } => {
            let from = now - Duration::days((*days).clamp(1, 365));
            let overview = repository.overview(from, now)?;
            output.emit(&overview, || {
                format!(
                    "{} active · {} focused · {} idle · {} context switches",
                    countdown(overview.active_seconds),
                    countdown(overview.focused_seconds),
                    countdown(overview.idle_seconds),
                    overview.context_switches
                )
            })
        }
        AttentionCommand::Configure {
            enabled,
            privacy,
            idle_seconds,
        } => {
            let mut settings = repository.load_settings()?;
            if let Some(enabled) = enabled {
                settings.enabled = *enabled;
            }
            if let Some(privacy) = privacy {
                settings.privacy = if privacy == "detailed" {
                    AttentionPrivacy::Detailed
                } else {
                    AttentionPrivacy::Applications
                };
            }
            if let Some(seconds) = idle_seconds {
                settings.idle_threshold_seconds = *seconds;
            }
            repository.save_settings(&settings)?;
            let mut shared = Settings::load(&directories.settings_file())?;
            shared.set("tabAttentionEnabled", serde_json::json!(settings.enabled));
            shared.set(
                "attentionPrivacy",
                serde_json::json!(if settings.privacy == AttentionPrivacy::Detailed {
                    "detailed"
                } else {
                    "applications"
                }),
            );
            shared.set(
                "attentionIdleSeconds",
                serde_json::json!(settings.idle_threshold_seconds),
            );
            shared.save(&directories.settings_file())?;
            output.emit(&settings, || {
                format!(
                    "Attention tracking {} · {:?} privacy · idle after {}",
                    if settings.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    },
                    settings.privacy,
                    countdown(settings.idle_threshold_seconds)
                )
            })
        }
        AttentionCommand::Record {
            application,
            title,
            idle,
            seconds,
        } => {
            let event = repository.record(application, title.as_deref(), *idle, *seconds, now)?;
            output.emit(&event, || String::new())
        }
    }
}

fn signed_remaining(seconds: i64) -> String {
    if seconds >= 0 {
        countdown(seconds)
    } else {
        format!("{} overtime", countdown(seconds.saturating_abs()))
    }
}

fn parse_duration(raw: &str) -> Result<i64, String> {
    let raw = raw.trim();
    let split = raw
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(raw.len());
    let (number, unit) = raw.split_at(split);
    let value: i64 = number
        .parse()
        .map_err(|_| "use a duration like 25m, 2h, or 3600s".to_string())?;
    let multiplier = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        _ => return Err("duration unit must be s, m, or h".to_string()),
    };
    value
        .checked_mul(multiplier)
        .context("duration is too large")
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_match_ediths_cli_shape() {
        assert_eq!(parse_duration("45m").unwrap(), 2700);
        assert_eq!(parse_duration("2h").unwrap(), 7200);
        assert_eq!(parse_duration("90s").unwrap(), 90);
        assert!(parse_duration("45").is_err());
        assert!(parse_duration("soon").is_err());
    }

    #[test]
    fn overtime_is_named_instead_of_becoming_now() {
        assert_eq!(signed_remaining(-120), "2m overtime");
    }
}
