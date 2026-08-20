//! Audio, through PipeWire's WirePlumber CLI.
//!
//! Edith reads per-app volume from CoreAudio and mutes the mic through
//! `AudioObjectSetPropertyData`. On Ubuntu the equivalent is PipeWire. Veronica
//! shells out to `wpctl` rather than linking libpipewire: the CLI is part of the
//! base install, its output is stable, and it keeps mic-mute working the same
//! way whether the session runs PipeWire or the PulseAudio shim.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use tokio::process::Command;

/// The default source, i.e. every microphone at once, which is what a
/// system-wide kill switch has to target.
pub const DEFAULT_SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";
pub const DEFAULT_SINK: &str = "@DEFAULT_AUDIO_SINK@";

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeState {
    /// 0.0-1.0, as PipeWire reports it. Values above 1.0 are possible when the
    /// user has boosted a stream past unity.
    pub volume: f32,
    pub muted: bool,
}

impl VolumeState {
    pub fn percent(&self) -> u8 {
        (self.volume * 100.0).round().clamp(0.0, 255.0) as u8
    }
}

/// Parse `wpctl get-volume` output.
///
/// The command prints `Volume: 0.65` and appends `[MUTED]` when muted, so both
/// facts come from one call.
pub fn parse_volume(output: &str) -> Result<VolumeState> {
    let line = output
        .lines()
        .find(|line| line.trim_start().starts_with("Volume:"))
        .context("wpctl printed no Volume line")?;

    let rest = line
        .trim_start()
        .trim_start_matches("Volume:")
        .trim();

    let raw = rest
        .split_whitespace()
        .next()
        .context("wpctl printed no volume value")?;

    let volume: f32 = raw
        .parse()
        .with_context(|| format!("wpctl printed an unparseable volume: {raw:?}"))?;

    Ok(VolumeState {
        volume,
        muted: rest.contains("[MUTED]"),
    })
}

async fn wpctl(args: &[&str]) -> Result<String> {
    let output = Command::new("wpctl")
        .args(args)
        .output()
        .await
        .context("wpctl is not installed; PipeWire tools are required for audio control")?;

    if !output.status.success() {
        bail!(
            "wpctl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Read the microphone's volume and mute state.
pub async fn microphone() -> Result<VolumeState> {
    parse_volume(&wpctl(&["get-volume", DEFAULT_SOURCE]).await?)
}

/// Read the speaker's volume and mute state.
pub async fn speaker() -> Result<VolumeState> {
    parse_volume(&wpctl(&["get-volume", DEFAULT_SINK]).await?)
}

/// Mute or unmute every microphone.
pub async fn set_microphone_muted(muted: bool) -> Result<()> {
    wpctl(&["set-mute", DEFAULT_SOURCE, if muted { "1" } else { "0" }]).await?;
    Ok(())
}

/// Flip the microphone mute and report the new state, which is what the tray
/// toggle and the global shortcut both need.
pub async fn toggle_microphone() -> Result<VolumeState> {
    wpctl(&["set-mute", DEFAULT_SOURCE, "toggle"]).await?;
    microphone().await
}

pub async fn set_speaker_muted(muted: bool) -> Result<()> {
    wpctl(&["set-mute", DEFAULT_SINK, if muted { "1" } else { "0" }]).await?;
    Ok(())
}

/// Set a volume as a fraction. Clamped to unity so the mixer cannot be driven
/// into distortion by a stray value.
pub async fn set_speaker_volume(volume: f32) -> Result<()> {
    let clamped = volume.clamp(0.0, 1.0);
    wpctl(&["set-volume", DEFAULT_SINK, &format!("{clamped:.2}")]).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_unmuted_volume() {
        let state = parse_volume("Volume: 0.65\n").unwrap();
        assert_eq!(state.volume, 0.65);
        assert!(!state.muted);
        assert_eq!(state.percent(), 65);
    }

    #[test]
    fn parses_the_muted_marker_wpctl_appends() {
        let state = parse_volume("Volume: 0.40 [MUTED]\n").unwrap();
        assert_eq!(state.volume, 0.40);
        assert!(state.muted, "the [MUTED] marker must be recognised");
    }

    #[test]
    fn tolerates_leading_whitespace_and_extra_lines() {
        let state = parse_volume("Node 51\n   Volume: 1.00\n").unwrap();
        assert_eq!(state.volume, 1.0);
        assert_eq!(state.percent(), 100);
    }

    #[test]
    fn a_boosted_volume_above_unity_is_preserved_not_clamped_on_read() {
        // Reporting 100% for a stream boosted to 140% would misrepresent it.
        let state = parse_volume("Volume: 1.40\n").unwrap();
        assert_eq!(state.volume, 1.40);
        assert_eq!(state.percent(), 140);
    }

    #[test]
    fn missing_or_unparseable_output_is_an_error_not_a_silent_zero() {
        assert!(parse_volume("").is_err());
        assert!(parse_volume("Sink 51. Built-in Audio\n").is_err());
        assert!(parse_volume("Volume: loud\n").is_err());
    }
}
