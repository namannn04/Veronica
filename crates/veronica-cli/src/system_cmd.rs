//! `vr system` — what this computer is doing right now.
//!
//! Edith's `ed system` has `stats` and `disks`; Veronica keeps both names and
//! adds the reads Linux makes cheap and Edith gets from separate places: the
//! running processes, the per-application audio mixer and Bluetooth. Every
//! subcommand reads the machine directly, so none of them needs the app to be
//! running.

use anyhow::Result;
use clap::Subcommand;
use veronica_system::audio::StreamDirection;

use crate::format::{self, Output};

#[derive(Subcommand)]
pub enum SystemCommand {
    /// Read CPU, memory, disks, temperatures and battery state.
    Snapshot,
    /// Sample CPU, memory and load for this computer.
    Stats,
    /// Mounted volumes and their free space.
    Disks,
    /// Running processes, by CPU or memory.
    #[command(alias = "ps")]
    Processes {
        /// Sort by `cpu`, `memory` or `name`.
        #[arg(long, default_value = "cpu")]
        sort: ProcessSort,
        /// How many rows to print. Ignored with `--json`, which emits them all.
        #[arg(long, default_value_t = 15)]
        limit: usize,
    },
    /// The per-application audio mixer: every stream and its volume.
    #[command(subcommand)]
    Audio(AudioCommand),
    /// Bluetooth adapters and the devices BlueZ knows about.
    Bluetooth,
}

#[derive(Subcommand)]
pub enum AudioCommand {
    /// List every application stream, playing or recording.
    #[command(alias = "ls")]
    List,
    /// Set one stream's volume, as a percentage or a fraction.
    Volume {
        /// The PipeWire node id, as printed by `vr system audio list`.
        id: u32,
        /// `40`, `40%` or `0.4` — all the same volume.
        level: String,
    },
    /// Mute one stream, or every stream at once with `--all`.
    Mute {
        /// The PipeWire node id. Omit it with `--all`.
        id: Option<u32>,
        /// Apply to every application stream.
        #[arg(long)]
        all: bool,
    },
    /// Unmute one stream, or every stream at once with `--all`.
    Unmute {
        id: Option<u32>,
        #[arg(long)]
        all: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ProcessSort {
    Cpu,
    Memory,
    Name,
}

/// Accept `40`, `40%` and `0.4` as the same volume, since all three are what
/// somebody types, and a bare `40` meaning 4000% would be a nasty surprise.
fn parse_level(raw: &str) -> Result<f32> {
    let trimmed = raw.trim();
    let (number, percent) = match trimmed.strip_suffix('%') {
        Some(rest) => (rest.trim(), true),
        None => (trimmed, false),
    };
    let value: f32 = number
        .parse()
        .map_err(|_| anyhow::anyhow!("not a volume: {raw:?}; try 40, 40% or 0.4"))?;
    if value < 0.0 {
        anyhow::bail!("a volume cannot be negative: {raw:?}");
    }
    // Without a `%`, anything above 1 was meant as a percentage.
    Ok(if percent || value > 1.0 {
        value / 100.0
    } else {
        value
    })
}

pub async fn run(command: &SystemCommand, output: Output) -> Result<()> {
    match command {
        SystemCommand::Snapshot => {
            let snapshot = veronica_system::MetricsSampler::new().sample();
            output.emit(&snapshot, || {
                let memory = snapshot.memory.used_percent();
                format!(
                    "CPU {:.0}% · memory {:.0}% · load {:.2}",
                    snapshot.cpu.usage_percent, memory, snapshot.load_average[0]
                )
            })
        }
        SystemCommand::Stats => {
            let snapshot = veronica_system::MetricsSampler::new().sample();
            output.emit(&snapshot, || {
                use std::fmt::Write;
                let mut out = String::new();
                let _ = writeln!(
                    out,
                    "{}  ·  {}  ·  up {}",
                    snapshot.host_name.as_deref().unwrap_or("this computer"),
                    snapshot.distribution,
                    format::countdown(snapshot.uptime_secs as i64)
                );
                let _ = writeln!(
                    out,
                    "CPU      {:.1}%  ({} threads, {})",
                    snapshot.cpu.usage_percent,
                    snapshot.cpu.logical_cores,
                    if snapshot.cpu.brand.is_empty() {
                        "unknown model"
                    } else {
                        &snapshot.cpu.brand
                    }
                );
                let _ = writeln!(
                    out,
                    "Memory   {:.1}%  ({} of {})",
                    snapshot.memory.used_percent(),
                    veronica_system::metrics::human_bytes(snapshot.memory.used_bytes),
                    veronica_system::metrics::human_bytes(snapshot.memory.total_bytes)
                );
                if snapshot.memory.swap_total_bytes > 0 {
                    let _ = writeln!(
                        out,
                        "Swap     {} of {}",
                        veronica_system::metrics::human_bytes(snapshot.memory.swap_used_bytes),
                        veronica_system::metrics::human_bytes(snapshot.memory.swap_total_bytes)
                    );
                }
                let _ = write!(
                    out,
                    "Load     {:.2}  {:.2}  {:.2}",
                    snapshot.load_average[0], snapshot.load_average[1], snapshot.load_average[2]
                );
                for reading in &snapshot.temperatures {
                    let _ = write!(out, "\n{:<8} {:.1} °C", reading.label, reading.celsius);
                }
                if let Some(battery) = &snapshot.battery {
                    let _ = write!(
                        out,
                        "\nBattery  {:.0}%{}",
                        battery.percent,
                        if battery.charging { " · charging" } else { "" }
                    );
                }
                out
            })
        }
        SystemCommand::Disks => {
            let snapshot = veronica_system::MetricsSampler::new().sample();
            output.emit(&snapshot.disks, || {
                let rows: Vec<Vec<String>> = snapshot
                    .disks
                    .iter()
                    .map(|disk| {
                        vec![
                            disk.name.clone(),
                            disk.mount_point.clone(),
                            disk.file_system.clone(),
                            veronica_system::metrics::human_bytes(disk.total_bytes),
                            veronica_system::metrics::human_bytes(disk.available_bytes),
                            format!("{:.0}%", disk.used_percent()),
                        ]
                    })
                    .collect();
                format::table(&["volume", "mount", "fs", "size", "free", "used"], &rows)
            })
        }
        SystemCommand::Processes { sort, limit } => {
            let mut processes = veronica_system::metrics::running_processes();
            match sort {
                ProcessSort::Cpu => processes.sort_by(|a, b| {
                    b.cpu_percent
                        .partial_cmp(&a.cpu_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }),
                ProcessSort::Memory => {
                    processes.sort_by_key(|process| std::cmp::Reverse(process.memory_bytes))
                }
                ProcessSort::Name => processes.sort_by_key(|process| process.name.to_lowercase()),
            }
            output.emit(&processes, || {
                let rows: Vec<Vec<String>> = processes
                    .iter()
                    .take(*limit)
                    .map(|process| {
                        vec![
                            process.pid.to_string(),
                            process.name.clone(),
                            format!("{:.1}%", process.cpu_percent),
                            veronica_system::metrics::human_bytes(process.memory_bytes),
                        ]
                    })
                    .collect();
                format::table(&["pid", "name", "cpu", "memory"], &rows)
            })
        }
        SystemCommand::Audio(command) => audio(command, output).await,
        SystemCommand::Bluetooth => {
            let state = veronica_system::bluetooth::state().await;
            output.emit(&state, || {
                use std::fmt::Write;
                let mut out = state.summary();
                let rows: Vec<Vec<String>> = state
                    .devices
                    .iter()
                    .map(|device| {
                        vec![
                            device.name.clone(),
                            device.address.clone(),
                            if device.connected {
                                "connected"
                            } else if device.paired {
                                "paired"
                            } else {
                                "seen"
                            }
                            .to_string(),
                            device
                                .battery_percent
                                .map(|percent| format!("{percent}%"))
                                .unwrap_or_else(|| "—".to_string()),
                        ]
                    })
                    .collect();
                let table = format::table(&["device", "address", "state", "battery"], &rows);
                if !table.is_empty() {
                    let _ = write!(out, "\n\n{table}");
                }
                out
            })
        }
    }
}

async fn audio(command: &AudioCommand, output: Output) -> Result<()> {
    match command {
        AudioCommand::List => {
            let streams = veronica_system::audio::streams().await?;
            output.emit(&streams, || {
                if streams.is_empty() {
                    return "No application is playing or recording audio.".to_string();
                }
                let rows: Vec<Vec<String>> = streams
                    .iter()
                    .map(|stream| {
                        vec![
                            stream.id.to_string(),
                            stream.label(),
                            match stream.direction {
                                StreamDirection::Playback => "playing",
                                StreamDirection::Capture => "recording",
                            }
                            .to_string(),
                            format!("{}%", stream.percent()),
                            if stream.muted { "muted" } else { "" }.to_string(),
                        ]
                    })
                    .collect();
                format::table(&["id", "application", "direction", "volume", ""], &rows)
            })
        }
        AudioCommand::Volume { id, level } => {
            let volume = parse_level(level)?;
            veronica_system::audio::set_stream_volume(*id, volume).await?;
            report_one(*id, output).await
        }
        AudioCommand::Mute { id, all } => set_muted(*id, *all, true, output).await,
        AudioCommand::Unmute { id, all } => set_muted(*id, *all, false, output).await,
    }
}

async fn set_muted(id: Option<u32>, all: bool, muted: bool, output: Output) -> Result<()> {
    match (id, all) {
        (Some(_), true) => {
            anyhow::bail!("pass either a stream id or --all, not both")
        }
        (None, false) => {
            anyhow::bail!("which stream? pass an id from `vr system audio list`, or --all")
        }
        (Some(id), false) => {
            veronica_system::audio::set_stream_muted(id, muted).await?;
            report_one(id, output).await
        }
        (None, true) => {
            let streams = veronica_system::audio::streams().await?;
            for stream in &streams {
                veronica_system::audio::set_stream_muted(stream.id, muted).await?;
            }
            let updated = veronica_system::audio::streams().await?;
            output.emit(&updated, || {
                format!(
                    "{} {} stream{}",
                    if muted { "Muted" } else { "Unmuted" },
                    streams.len(),
                    if streams.len() == 1 { "" } else { "s" }
                )
            })
        }
    }
}

/// Re-read one stream after changing it, so the command reports what actually
/// happened rather than what was asked for.
async fn report_one(id: u32, output: Output) -> Result<()> {
    let stream = veronica_system::audio::streams()
        .await?
        .into_iter()
        .find(|stream| stream.id == id);
    match stream {
        Some(stream) => output.emit(&stream, || {
            format!(
                "{} · {}%{}",
                stream.label(),
                stream.percent(),
                if stream.muted { " · muted" } else { "" }
            )
        }),
        None => anyhow::bail!(
            "no audio stream with id {id}; run `vr system audio list` for the current ids"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_number_above_one_is_read_as_a_percentage() {
        assert_eq!(parse_level("40").unwrap(), 0.4);
        assert_eq!(parse_level("100").unwrap(), 1.0);
    }

    #[test]
    fn an_explicit_percent_sign_is_honoured_even_below_one() {
        assert!((parse_level("0.5%").unwrap() - 0.005).abs() < 1e-6);
    }

    #[test]
    fn a_fraction_is_taken_as_written() {
        assert_eq!(parse_level("0.4").unwrap(), 0.4);
        assert_eq!(parse_level("1").unwrap(), 1.0);
        assert_eq!(parse_level("0").unwrap(), 0.0);
    }

    #[test]
    fn whitespace_and_a_spaced_percent_sign_are_tolerated() {
        assert_eq!(parse_level("  60 % ").unwrap(), 0.6);
    }

    #[test]
    fn nonsense_and_negatives_are_rejected_rather_than_silently_clamped() {
        assert!(parse_level("loud").is_err());
        assert!(parse_level("").is_err());
        assert!(parse_level("-10").is_err());
    }
}
