//! MPRIS2 media control.
//!
//! Edith drives Spotify and Apple Music through AppleScript. On Linux the
//! standard is MPRIS2 over D-Bus, which every mainstream player implements, so
//! one code path controls Spotify, Rhythmbox, VLC, a browser tab and Veronica's
//! own player.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use zbus::zvariant::{OwnedValue, Value};
use zbus::Connection;

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";

/// Read a D-Bus string, whatever string flavour the player used.
fn as_text(value: &OwnedValue) -> Option<String> {
    match &**value {
        Value::Str(s) => Some(s.as_str().to_string()),
        Value::ObjectPath(p) => Some(p.as_str().to_string()),
        _ => None,
    }
}

/// Read a D-Bus integer.
///
/// MPRIS declares `mpris:length` and `Position` as `x` (i64), but players ship
/// `u64` and `i32` in practice, so all three are accepted rather than silently
/// losing the track length.
fn as_integer(value: &OwnedValue) -> Option<i64> {
    match &**value {
        Value::I64(v) => Some(*v),
        Value::U64(v) => i64::try_from(*v).ok(),
        Value::I32(v) => Some(i64::from(*v)),
        Value::U32(v) => Some(i64::from(*v)),
        Value::F64(v) => Some(*v as i64),
        _ => None,
    }
}

fn as_bool(value: &OwnedValue) -> Option<bool> {
    match &**value {
        Value::Bool(v) => Some(*v),
        _ => None,
    }
}

/// Longest duration accepted as a real track length, in microseconds.
///
/// Players signal "unknown" or "live stream" in two ways: zero, and a huge
/// sentinel. Chrome reports `i64::MAX` for a live video, which rendered as a
/// 153-million-hour track. Anything past this bound is treated as unknown; it is
/// far beyond any single track while still allowing a long audiobook file.
pub const MAX_PLAUSIBLE_LENGTH_US: i64 = 100 * 60 * 60 * 1_000_000;

/// Whether a reported length is a real duration rather than an unknown marker.
fn is_real_length(length: &i64) -> bool {
    *length > 0 && *length < MAX_PLAUSIBLE_LENGTH_US
}

/// Read an array of strings, skipping any non-string entries.
fn as_text_list(value: &OwnedValue) -> Vec<String> {
    match &**value {
        Value::Array(array) => array
            .iter()
            .filter_map(|entry| match entry {
                Value::Str(s) => Some(s.as_str().to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

impl PlaybackStatus {
    fn parse(raw: &str) -> Self {
        match raw {
            "Playing" => PlaybackStatus::Playing,
            "Paused" => PlaybackStatus::Paused,
            _ => PlaybackStatus::Stopped,
        }
    }

    pub fn is_playing(self) -> bool {
        self == PlaybackStatus::Playing
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    /// The player's bus name suffix, e.g. "spotify".
    pub player: String,
    /// Friendly name from the player's Identity property.
    pub identity: String,
    pub status: Option<PlaybackStatus>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
    /// Track length in microseconds, as MPRIS reports it.
    pub length_us: Option<i64>,
    pub position_us: Option<i64>,
    pub can_go_next: bool,
    pub can_go_previous: bool,
}

impl NowPlaying {
    /// Fraction played, 0.0-1.0, for a progress bar.
    pub fn progress(&self) -> Option<f64> {
        match (self.position_us, self.length_us) {
            (Some(position), Some(length)) if length > 0 => {
                Some((position as f64 / length as f64).clamp(0.0, 1.0))
            }
            _ => None,
        }
    }
}

/// The metadata keys MPRIS defines, extracted defensively: players disagree on
/// which they populate, and several omit the album or the art entirely.
pub fn read_metadata(metadata: &std::collections::HashMap<String, OwnedValue>) -> (String, String, String, Option<String>, Option<i64>) {
    let text = |key: &str| -> String {
        metadata.get(key).and_then(as_text).unwrap_or_default()
    };

    // xesam:artist is a list; the first non-empty entry is the primary artist.
    let artist = metadata
        .get("xesam:artist")
        .map(as_text_list)
        .and_then(|list| list.into_iter().find(|a| !a.is_empty()))
        .unwrap_or_default();

    let art = metadata
        .get("mpris:artUrl")
        .and_then(as_text)
        .filter(|url| !url.is_empty());

    let length = metadata.get("mpris:length").and_then(as_integer).filter(is_real_length);

    (text("xesam:title"), artist, text("xesam:album"), art, length)
}

/// Every MPRIS player currently on the session bus.
pub async fn players(connection: &Connection) -> Result<Vec<String>> {
    let proxy = zbus::fdo::DBusProxy::new(connection)
        .await
        .context("cannot reach the session bus")?;
    let names = proxy.list_names().await.context("cannot list bus names")?;

    let mut found: Vec<String> = names
        .into_iter()
        .map(|name| name.as_str().to_string())
        .filter(|name| name.starts_with(MPRIS_PREFIX))
        .collect();
    found.sort();
    Ok(found)
}

/// The player Veronica should show and control.
///
/// A machine often has several registered players, most of them idle. Preferring
/// one that is actually playing stops the island showing a paused browser tab
/// while music comes out of Spotify.
pub async fn active_player(connection: &Connection) -> Result<Option<String>> {
    let candidates = players(connection).await?;
    let mut first = None;
    for name in candidates {
        let status = status_of(connection, &name).await.ok().flatten();
        if status.map(PlaybackStatus::is_playing).unwrap_or(false) {
            return Ok(Some(name));
        }
        if first.is_none() {
            first = Some(name);
        }
    }
    Ok(first)
}

async fn property(connection: &Connection, bus: &str, interface: &str, name: &str) -> Result<OwnedValue> {
    let reply = connection
        .call_method(
            Some(bus),
            MPRIS_PATH,
            Some(PROPERTIES_INTERFACE),
            "Get",
            &(interface, name),
        )
        .await?;
    // The body must outlive the borrow the deserialised value holds.
    let body = reply.body();
    let value: Value = body.deserialize()?;
    Ok(OwnedValue::try_from(value)?)
}

async fn status_of(connection: &Connection, bus: &str) -> Result<Option<PlaybackStatus>> {
    let value = property(connection, bus, PLAYER_INTERFACE, "PlaybackStatus").await?;
    Ok(as_text(&value).map(|raw| PlaybackStatus::parse(&raw)))
}

/// Read what is playing. Returns `None` when no player is registered at all.
pub async fn now_playing(connection: &Connection) -> Result<Option<NowPlaying>> {
    let Some(bus) = active_player(connection).await? else {
        return Ok(None);
    };

    let mut playing = NowPlaying {
        player: bus.trim_start_matches(MPRIS_PREFIX).to_string(),
        ..Default::default()
    };

    playing.status = status_of(connection, &bus).await.ok().flatten();

    if let Ok(value) = property(connection, &bus, "org.mpris.MediaPlayer2", "Identity").await {
        playing.identity = as_text(&value).unwrap_or_default();
    }
    if playing.identity.is_empty() {
        playing.identity = playing.player.clone();
    }

    if let Ok(value) = property(connection, &bus, PLAYER_INTERFACE, "Metadata").await {
        if let Ok(metadata) = std::collections::HashMap::<String, OwnedValue>::try_from(value) {
            let (title, artist, album, art, length) = read_metadata(&metadata);
            playing.title = title;
            playing.artist = artist;
            playing.album = album;
            playing.art_url = art;
            playing.length_us = length;
        }
    }

    // Position is optional; several players refuse to report it.
    if let Ok(value) = property(connection, &bus, PLAYER_INTERFACE, "Position").await {
        playing.position_us = as_integer(&value);
    }
    for (name, field) in [
        ("CanGoNext", &mut playing.can_go_next),
        ("CanGoPrevious", &mut playing.can_go_previous),
    ] {
        if let Ok(value) = property(connection, &bus, PLAYER_INTERFACE, name).await {
            *field = as_bool(&value).unwrap_or(false);
        }
    }

    Ok(Some(playing))
}

/// A transport command the island's buttons and the media keys both use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Play,
    Pause,
    PlayPause,
    Next,
    Previous,
    Stop,
}

impl Transport {
    fn method(self) -> &'static str {
        match self {
            Transport::Play => "Play",
            Transport::Pause => "Pause",
            Transport::PlayPause => "PlayPause",
            Transport::Next => "Next",
            Transport::Previous => "Previous",
            Transport::Stop => "Stop",
        }
    }
}

/// Send a transport command to the active player.
pub async fn control(connection: &Connection, transport: Transport) -> Result<()> {
    let Some(bus) = active_player(connection).await? else {
        bail!("no media player is running");
    };
    connection
        .call_method(
            Some(bus.as_str()),
            MPRIS_PATH,
            Some(PLAYER_INTERFACE),
            transport.method(),
            &(),
        )
        .await
        .with_context(|| format!("{} refused {}", bus, transport.method()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn playback_status_parses_the_three_spec_values() {
        assert_eq!(PlaybackStatus::parse("Playing"), PlaybackStatus::Playing);
        assert_eq!(PlaybackStatus::parse("Paused"), PlaybackStatus::Paused);
        assert_eq!(PlaybackStatus::parse("Stopped"), PlaybackStatus::Stopped);
        // Anything unexpected is treated as stopped rather than assumed playing.
        assert_eq!(PlaybackStatus::parse("garbage"), PlaybackStatus::Stopped);
    }

    #[test]
    fn transport_methods_match_the_mpris_names() {
        assert_eq!(Transport::PlayPause.method(), "PlayPause");
        assert_eq!(Transport::Previous.method(), "Previous");
    }

    #[test]
    fn progress_needs_both_a_position_and_a_length() {
        let mut playing = NowPlaying::default();
        assert_eq!(playing.progress(), None);
        playing.position_us = Some(30_000_000);
        assert_eq!(playing.progress(), None, "a position alone is not progress");
        playing.length_us = Some(120_000_000);
        assert_eq!(playing.progress(), Some(0.25));
    }

    #[test]
    fn progress_is_clamped_when_a_player_reports_past_the_end() {
        let playing = NowPlaying {
            position_us: Some(200),
            length_us: Some(100),
            ..Default::default()
        };
        assert_eq!(playing.progress(), Some(1.0));
    }

    #[test]
    fn a_zero_length_track_has_no_progress_rather_than_dividing_by_zero() {
        let playing = NowPlaying {
            position_us: Some(5),
            length_us: Some(0),
            ..Default::default()
        };
        assert_eq!(playing.progress(), None);
    }

    #[test]
    fn metadata_extraction_takes_the_first_artist_from_the_list() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "xesam:title".to_string(),
            OwnedValue::try_from(Value::from("Midnight City")).unwrap(),
        );
        metadata.insert(
            "xesam:artist".to_string(),
            OwnedValue::try_from(Value::from(vec!["M83".to_string(), "Other".to_string()]))
                .unwrap(),
        );
        metadata.insert(
            "mpris:length".to_string(),
            OwnedValue::try_from(Value::from(243_000_000i64)).unwrap(),
        );

        let (title, artist, album, art, length) = read_metadata(&metadata);
        assert_eq!(title, "Midnight City");
        assert_eq!(artist, "M83");
        assert_eq!(album, "", "a missing album is empty, not an error");
        assert_eq!(art, None);
        assert_eq!(length, Some(243_000_000));
    }

    #[test]
    fn metadata_extraction_survives_a_player_that_populates_nothing() {
        let (title, artist, album, art, length) = read_metadata(&HashMap::new());
        assert!(title.is_empty() && artist.is_empty() && album.is_empty());
        assert_eq!(art, None);
        assert_eq!(length, None);
    }

    #[test]
    fn length_is_read_from_whichever_integer_type_the_player_used() {
        // MPRIS declares i64, but real players ship u64 and i32.
        for value in [
            Value::from(243_000_000i64),
            Value::from(243_000_000u64),
            Value::from(243_000_000i32),
        ] {
            let mut metadata = HashMap::new();
            metadata.insert(
                "mpris:length".to_string(),
                OwnedValue::try_from(value).unwrap(),
            );
            let (_, _, _, _, length) = read_metadata(&metadata);
            assert_eq!(length, Some(243_000_000));
        }
    }

    #[test]
    fn a_non_string_artist_entry_is_skipped_rather_than_failing_the_whole_read() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "xesam:title".to_string(),
            OwnedValue::try_from(Value::from("Track")).unwrap(),
        );
        metadata.insert(
            "xesam:artist".to_string(),
            OwnedValue::try_from(Value::from(vec!["".to_string(), "Real".to_string()])).unwrap(),
        );
        let (title, artist, _, _, _) = read_metadata(&metadata);
        assert_eq!(title, "Track");
        assert_eq!(artist, "Real", "empty entries are skipped");
    }

    #[test]
    fn a_sentinel_length_is_treated_as_unknown() {
        // Chrome reports i64::MAX for a live stream; passing it through rendered
        // a 153-million-hour track length.
        for sentinel in [i64::MAX, MAX_PLAUSIBLE_LENGTH_US, i64::MAX / 2] {
            let mut metadata = HashMap::new();
            metadata.insert(
                "mpris:length".to_string(),
                OwnedValue::try_from(Value::from(sentinel)).unwrap(),
            );
            let (_, _, _, _, length) = read_metadata(&metadata);
            assert_eq!(length, None, "{sentinel} should read as unknown");
        }
    }

    #[test]
    fn a_long_but_plausible_length_is_kept() {
        // A six-hour audiobook file is real and must not be discarded.
        let mut metadata = HashMap::new();
        let six_hours = 6 * 60 * 60 * 1_000_000i64;
        metadata.insert(
            "mpris:length".to_string(),
            OwnedValue::try_from(Value::from(six_hours)).unwrap(),
        );
        let (_, _, _, _, length) = read_metadata(&metadata);
        assert_eq!(length, Some(six_hours));
    }

    #[test]
    fn a_negative_length_is_rejected() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "mpris:length".to_string(),
            OwnedValue::try_from(Value::from(-5i64)).unwrap(),
        );
        let (_, _, _, _, length) = read_metadata(&metadata);
        assert_eq!(length, None);
    }

    #[test]
    fn a_zero_length_is_discarded_because_players_use_it_for_unknown() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "mpris:length".to_string(),
            OwnedValue::try_from(Value::from(0i64)).unwrap(),
        );
        let (_, _, _, _, length) = read_metadata(&metadata);
        assert_eq!(length, None);
    }
}
