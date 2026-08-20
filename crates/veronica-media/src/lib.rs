//! Media control for Veronica.
//!
//! MPRIS2 over D-Bus, which covers every mainstream Linux player, so the notch
//! island and the media keys drive Spotify, a browser tab or a local player
//! through one interface.

pub mod mpris;

pub use mpris::{control, now_playing, players, NowPlaying, PlaybackStatus, Transport};
