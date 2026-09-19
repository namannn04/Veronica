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

    let rest = line.trim_start().trim_start_matches("Volume:").trim();

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

/// One application's audio stream, as PipeWire sees it.
///
/// Edith reads these from CoreAudio's per-process volume API. PipeWire has no
/// per-*process* volume either — it has per-*stream* volume, which is the same
/// thing in practice, since a stream is what an application opens to play or
/// record. A Chrome tab and a Chrome notification sound are two streams, so the
/// mixer lists them separately rather than pretending Chrome has one volume.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStream {
    /// PipeWire node id, which is what `wpctl` takes as a target.
    pub id: u32,
    /// The application's own name, e.g. `Spotify`.
    pub application: String,
    /// What the stream is, e.g. `Playback` or `RecordStream`. Distinguishes two
    /// streams from the same application.
    pub media_name: Option<String>,
    pub direction: StreamDirection,
    pub volume: f32,
    pub muted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamDirection {
    /// The application is playing audio out.
    Playback,
    /// The application is recording, i.e. it is listening to a microphone.
    Capture,
}

impl AudioStream {
    pub fn percent(&self) -> u8 {
        (self.volume * 100.0).round().clamp(0.0, 255.0) as u8
    }

    /// What to show as the row's title: the application, with the stream name
    /// appended only when it adds something.
    pub fn label(&self) -> String {
        match self.media_name.as_deref() {
            Some(name)
                if !name.is_empty()
                    && !self.application.eq_ignore_ascii_case(name)
                    && !matches!(name, "Playback" | "playback" | "RecordStream" | "record") =>
            {
                format!("{} — {name}", self.application)
            }
            _ => self.application.clone(),
        }
    }
}

/// PipeWire stores a channel volume as the cube of the value every mixer shows,
/// so `wpctl get-volume` and `pw-dump` disagree by that factor. Converting here
/// means one `pw-dump` call describes every stream, instead of one `wpctl` call
/// per stream, and the numbers still match what the user sees elsewhere.
fn display_volume(channel_volumes: &[f64]) -> f32 {
    if channel_volumes.is_empty() {
        return 0.0;
    }
    // The loudest channel is the one a single slider should represent, so
    // moving it never quietly attenuates a channel that was already louder.
    let peak = channel_volumes
        .iter()
        .copied()
        .fold(0.0_f64, |peak, value| peak.max(value.max(0.0)));
    peak.cbrt() as f32
}

/// The inverse, which only the round-trip test needs: `wpctl` takes the display
/// scale, so nothing in the mixer path converts in this direction.
#[cfg(test)]
fn storage_volume(display: f32) -> f64 {
    let clamped = display.clamp(0.0, 1.0) as f64;
    clamped * clamped * clamped
}

/// Parse `pw-dump` output into the audio streams applications currently hold.
///
/// Nodes that are not application streams — devices, sinks, sources, filters —
/// are skipped, and so is Veronica's own stream, which nobody wants to see in a
/// mixer they opened from Veronica.
pub fn parse_streams(dump: &str) -> Result<Vec<AudioStream>> {
    let objects: serde_json::Value =
        serde_json::from_str(dump).context("pw-dump did not emit JSON")?;
    let objects = objects
        .as_array()
        .context("pw-dump emitted JSON that is not an array of objects")?;

    let mut streams = Vec::new();
    for object in objects {
        if object.get("type").and_then(serde_json::Value::as_str) != Some("PipeWire:Interface:Node")
        {
            continue;
        }
        let info = match object.get("info") {
            Some(info) => info,
            None => continue,
        };
        let props = info.get("props").unwrap_or(&serde_json::Value::Null);
        let class = props
            .get("media.class")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let direction = match class {
            "Stream/Output/Audio" => StreamDirection::Playback,
            "Stream/Input/Audio" => StreamDirection::Capture,
            _ => continue,
        };

        let application = props
            .get("application.name")
            .or_else(|| props.get("node.description"))
            .or_else(|| props.get("node.name"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Unknown application")
            .to_string();
        if application.eq_ignore_ascii_case("Veronica") {
            continue;
        }

        let id = match object.get("id").and_then(serde_json::Value::as_u64) {
            Some(id) => id as u32,
            None => continue,
        };

        // Props is one entry in the node's parameter list; a node that has not
        // published it yet still belongs in the mixer, at its default volume.
        let params = info
            .get("params")
            .and_then(|params| params.get("Props"))
            .and_then(serde_json::Value::as_array)
            .and_then(|entries| entries.first());

        let channel_volumes: Vec<f64> = params
            .and_then(|entry| entry.get("channelVolumes"))
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(serde_json::Value::as_f64)
                    .collect()
            })
            .unwrap_or_default();

        streams.push(AudioStream {
            id,
            application,
            media_name: props
                .get("media.name")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            direction,
            volume: if channel_volumes.is_empty() {
                1.0
            } else {
                display_volume(&channel_volumes)
            },
            muted: params
                .and_then(|entry| entry.get("mute"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        });
    }

    // A stable order keeps rows from jumping around as the mixer refreshes.
    streams.sort_by(|left, right| {
        left.application
            .to_lowercase()
            .cmp(&right.application.to_lowercase())
            .then(left.id.cmp(&right.id))
    });
    Ok(streams)
}

/// Every application stream currently on the graph.
pub async fn streams() -> Result<Vec<AudioStream>> {
    let output = Command::new("pw-dump")
        .output()
        .await
        .context("pw-dump is not installed; PipeWire tools are required for the audio mixer")?;
    if !output.status.success() {
        bail!(
            "pw-dump failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    parse_streams(&String::from_utf8_lossy(&output.stdout))
}

/// Set one application stream's volume, as the fraction a mixer displays.
pub async fn set_stream_volume(id: u32, volume: f32) -> Result<()> {
    let clamped = volume.clamp(0.0, 1.0);
    wpctl(&["set-volume", &id.to_string(), &format!("{clamped:.2}")]).await?;
    Ok(())
}

/// Mute or unmute one application stream.
pub async fn set_stream_muted(id: u32, muted: bool) -> Result<()> {
    wpctl(&["set-mute", &id.to_string(), if muted { "1" } else { "0" }]).await?;
    Ok(())
}

/// Flip one application stream's mute and report what it became, so the caller
/// does not have to re-read the whole graph to update one row.
pub async fn toggle_stream_muted(id: u32) -> Result<bool> {
    wpctl(&["set-mute", &id.to_string(), "toggle"]).await?;
    Ok(streams()
        .await?
        .into_iter()
        .find(|stream| stream.id == id)
        .map(|stream| stream.muted)
        // A stream that ended while we toggled it is not muted, it is gone.
        .unwrap_or(false))
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

    const DUMP: &str = r#"[
      {"id": 59, "type": "PipeWire:Interface:Node",
       "info": {"props": {"media.class": "Audio/Sink", "node.name": "alsa_output"},
                "params": {"Props": [{"channelVolumes": [1.0, 1.0], "mute": false}]}}},
      {"id": 84, "type": "PipeWire:Interface:Node",
       "info": {"props": {"media.class": "Stream/Output/Audio",
                          "application.name": "Google Chrome", "media.name": "Playback"},
                "params": {"Props": [{"channelVolumes": [0.125, 0.125], "mute": false}]}}},
      {"id": 81, "type": "PipeWire:Interface:Node",
       "info": {"props": {"media.class": "Stream/Input/Audio",
                          "application.name": "Slack", "media.name": "RecordStream"},
                "params": {"Props": [{"channelVolumes": [1.0, 1.0], "mute": true}]}}},
      {"id": 90, "type": "PipeWire:Interface:Node",
       "info": {"props": {"media.class": "Stream/Output/Audio",
                          "application.name": "Veronica", "media.name": "Playback"},
                "params": {"Props": [{"channelVolumes": [1.0], "mute": false}]}}},
      {"id": 12, "type": "PipeWire:Interface:Client", "info": {}}
    ]"#;

    #[test]
    fn only_application_streams_appear_in_the_mixer() {
        let streams = parse_streams(DUMP).unwrap();
        let ids: Vec<u32> = streams.iter().map(|stream| stream.id).collect();
        // 59 is a sink, 12 is a client, 90 is Veronica's own stream.
        assert_eq!(ids, vec![84, 81], "got {streams:#?}");
    }

    #[test]
    fn a_channel_volume_is_reported_on_the_scale_every_mixer_shows() {
        let streams = parse_streams(DUMP).unwrap();
        let chrome = streams.iter().find(|s| s.id == 84).unwrap();
        // PipeWire stores 0.5^3; showing 0.125 would understate it badly.
        assert!((chrome.volume - 0.5).abs() < 0.001, "got {}", chrome.volume);
        assert_eq!(chrome.percent(), 50);
    }

    #[test]
    fn direction_and_mute_come_through_per_stream() {
        let streams = parse_streams(DUMP).unwrap();
        let slack = streams.iter().find(|s| s.id == 81).unwrap();
        assert_eq!(slack.direction, StreamDirection::Capture);
        assert!(slack.muted);
        let chrome = streams.iter().find(|s| s.id == 84).unwrap();
        assert_eq!(chrome.direction, StreamDirection::Playback);
        assert!(!chrome.muted);
    }

    #[test]
    fn a_generic_stream_name_is_not_appended_to_the_application() {
        let streams = parse_streams(DUMP).unwrap();
        assert_eq!(
            streams.iter().find(|s| s.id == 84).unwrap().label(),
            "Google Chrome"
        );
    }

    #[test]
    fn a_meaningful_stream_name_is_appended_so_two_rows_can_be_told_apart() {
        let dump = DUMP.replace(
            "\"media.name\": \"Playback\"",
            "\"media.name\": \"Tab: Radio\"",
        );
        let streams = parse_streams(&dump).unwrap();
        assert_eq!(
            streams.iter().find(|s| s.id == 84).unwrap().label(),
            "Google Chrome — Tab: Radio"
        );
    }

    #[test]
    fn a_node_that_has_not_published_props_yet_still_lists_at_full_volume() {
        let dump = r#"[{"id": 7, "type": "PipeWire:Interface:Node",
          "info": {"props": {"media.class": "Stream/Output/Audio", "application.name": "VLC"}}}]"#;
        let streams = parse_streams(dump).unwrap();
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].volume, 1.0);
        assert!(!streams[0].muted);
    }

    #[test]
    fn the_loudest_channel_drives_the_single_slider() {
        // Averaging would report a quieter stream than the user can hear.
        assert!((display_volume(&[0.125, 1.0]) - 1.0).abs() < 0.001);
    }

    #[test]
    fn the_display_scale_round_trips_back_to_storage() {
        for value in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            let stored = storage_volume(value);
            assert!((display_volume(&[stored]) - value).abs() < 0.001, "{value}");
        }
    }

    #[test]
    fn malformed_pw_dump_output_is_an_error_not_an_empty_mixer() {
        assert!(parse_streams("").is_err());
        assert!(parse_streams("{}").is_err());
    }
}
