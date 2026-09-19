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
    /// The local music library under ~/Music, searchable.
    #[command(alias = "lib")]
    Library {
        /// Match the title, artist or album.
        #[arg(default_value = "")]
        query: String,
        #[arg(long, default_value_t = 40)]
        limit: usize,
    },
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

/// The library folder Veronica indexes. The same one the app scans, so the two
/// never disagree about what is in it.
fn music_root() -> Result<std::path::PathBuf> {
    let home = veronica_core::paths::home_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot resolve the home directory"))?;
    Ok(home.join("Music"))
}

pub async fn run(command: &MediaCommand, output: Output) -> Result<()> {
    // The library is files on disk, not a player on the bus, so it answers
    // before any D-Bus connection is attempted — and works with no session bus
    // at all, over SSH.
    if let MediaCommand::Library { query, limit } = command {
        let root = music_root()?;
        if !root.is_dir() {
            anyhow::bail!(
                "no music library: {} does not exist. Put audio files there and try again.",
                root.display()
            );
        }
        let needle = query.to_lowercase();
        let tracks: Vec<_> = veronica_media::scan_library(&root)?
            .into_iter()
            .filter(|track| {
                needle.is_empty()
                    || format!("{} {} {}", track.title, track.artist, track.album)
                        .to_lowercase()
                        .contains(&needle)
            })
            .take(*limit)
            .collect();
        return output.emit(&tracks, || {
            if tracks.is_empty() {
                return if query.is_empty() {
                    format!("no audio files under {}", root.display())
                } else {
                    format!("nothing matches {query:?}")
                };
            }
            let rows: Vec<Vec<String>> = tracks
                .iter()
                .map(|track| {
                    vec![
                        track.title.clone(),
                        track.artist.clone(),
                        track.album.clone(),
                    ]
                })
                .collect();
            format::table(&["title", "artist", "album"], &rows)
        });
    }

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
                // Status, Players and Library are all handled above.
                MediaCommand::Status | MediaCommand::Players | MediaCommand::Library { .. } => {
                    unreachable!()
                }
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
                            if p.title.is_empty() {
                                "untitled"
                            } else {
                                &p.title
                            },
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
        assert_eq!(
            timeline(Some(102_000_000), Some(243_000_000)),
            "1:42 / 4:03"
        );
        assert_eq!(timeline(Some(5_000_000), None), "0:05");
        assert_eq!(timeline(None, Some(60_000_000)), "? / 1:00");
        assert_eq!(timeline(None, None), "—");
    }

    #[test]
    fn timeline_pads_seconds_so_columns_line_up() {
        assert_eq!(
            timeline(Some(61_000_000), Some(600_000_000)),
            "1:01 / 10:00"
        );
    }
}
