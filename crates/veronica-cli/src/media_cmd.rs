//! `vr media` — control whatever is playing, through MPRIS.

use anyhow::{Context, Result};
use clap::Subcommand;
use veronica_media::{mpris, Transport};

use crate::format::{self, Output};

#[derive(Subcommand)]
pub enum MediaCommand {
    /// What is playing right now.
    Status,
    /// Every MPRIS player registered on the session bus.
    Players,
    Play,
    Pause,
    /// Play if paused, pause if playing.
    Toggle,
    Next,
    #[command(alias = "prev")]
    Previous,
    Stop,
}

/// Track position as `m:ss / m:ss`, from the microseconds MPRIS reports.
fn timeline(position_us: Option<i64>, length_us: Option<i64>) -> String {
    let clock = |us: i64| {
        let seconds = us / 1_000_000;
        format!("{}:{:02}", seconds / 60, seconds % 60)
    };
    match (position_us, length_us) {
        (Some(position), Some(length)) => format!("{} / {}", clock(position), clock(length)),
        (Some(position), None) => clock(position),
        (None, Some(length)) => format!("? / {}", clock(length)),
        (None, None) => "—".to_string(),
    }
}

pub async fn run(command: &MediaCommand, output: Output) -> Result<()> {
    let connection = zbus::Connection::session()
        .await
        .context("cannot reach the session bus; is this a desktop session?")?;

    match command {
        MediaCommand::Players => {
            let players = mpris::players(&connection).await?;
            output.emit(&players, || {
                if players.is_empty() {
                    return "no MPRIS players are running".to_string();
                }
                let rows: Vec<Vec<String>> = players
                    .iter()
                    .map(|bus| {
                        vec![bus
                            .trim_start_matches("org.mpris.MediaPlayer2.")
                            .to_string()]
                    })
                    .collect();
                format::table(&["player"], &rows)
            })
        }

        MediaCommand::Status => {
            let playing = mpris::now_playing(&connection).await?;
            output.emit(&playing, || {
                let Some(playing) = &playing else {
                    return "nothing is playing".to_string();
                };
                use std::fmt::Write;
                let mut out = String::new();
                let status = playing
                    .status
                    .map(|s| format!("{s:?}"))
                    .unwrap_or_else(|| "Unknown".into());
                let _ = writeln!(out, "Player     {} ({})", playing.identity, status);
                if !playing.title.is_empty() {
                    let _ = writeln!(out, "Title      {}", playing.title);
                }
                if !playing.artist.is_empty() {
                    let _ = writeln!(out, "Artist     {}", playing.artist);
                }
                if !playing.album.is_empty() {
                    let _ = writeln!(out, "Album      {}", playing.album);
                }
                let _ = write!(
                    out,
                    "Position   {}",
                    timeline(playing.position_us, playing.length_us)
                );
                out
            })
        }

        other => {
            let transport = match other {
                MediaCommand::Play => Transport::Play,
                MediaCommand::Pause => Transport::Pause,
                MediaCommand::Toggle => Transport::PlayPause,
                MediaCommand::Next => Transport::Next,
                MediaCommand::Previous => Transport::Previous,
                MediaCommand::Stop => Transport::Stop,
                // Status and Players are handled above.
                MediaCommand::Status | MediaCommand::Players => unreachable!(),
            };
            mpris::control(&connection, transport).await?;
            // Report the resulting state so a script does not have to poll.
            let playing = mpris::now_playing(&connection).await?;
            output.emit(&playing, || {
                playing
                    .as_ref()
                    .map(|p| {
                        format!(
                            "{:?} · {} — {}",
                            p.status.unwrap_or(veronica_media::PlaybackStatus::Stopped),
                            if p.title.is_empty() { "untitled" } else { &p.title },
                            p.identity
                        )
                    })
                    .unwrap_or_else(|| "no player".to_string())
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_formats_microseconds_as_minutes_and_seconds() {
        assert_eq!(timeline(Some(102_000_000), Some(243_000_000)), "1:42 / 4:03");
        assert_eq!(timeline(Some(5_000_000), None), "0:05");
        assert_eq!(timeline(None, Some(60_000_000)), "? / 1:00");
        assert_eq!(timeline(None, None), "—");
    }

    #[test]
    fn timeline_pads_seconds_so_columns_line_up() {
        assert_eq!(timeline(Some(61_000_000), Some(600_000_000)), "1:01 / 10:00");
    }
}
