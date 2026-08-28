//! Focus Dim.
//!
//! Dims everything behind the window you are working in.
//!
//! Ported from Edith's `FocusDimMath` and `FocusDimDisplayMode`: the same
//! ranges, the same defaults, the same two display modes and the same settings
//! keys.
//!
//! What differs is where the dimming happens. Edith stacks its own windows
//! underneath the focused one, which a Wayland client cannot do — a client
//! cannot position itself, raise itself, or learn what else is on screen. The
//! compositor can, so on Ubuntu the overlay is drawn by Veronica's GNOME Shell
//! extension. That also makes it work on Wayland, which the equivalent X11
//! approach would not.
//!
//! This module is the part both sides agree on: what a valid intensity is, what
//! the modes mean, and which keys hold them. The extension clamps again on read,
//! because a settings file can be edited by hand and a compositor overlay at
//! full opacity would leave the user unable to see anything.

use serde::{Deserialize, Serialize};

use crate::Settings;

/// How dark the dimming may go. Edith's ceiling is 0.9 rather than 1.0: a fully
/// opaque overlay would hide the desktop completely, which is a lock screen
/// rather than a focus aid.
pub const MIN_INTENSITY: f64 = 0.0;
pub const MAX_INTENSITY: f64 = 0.9;
pub const DEFAULT_INTENSITY: f64 = 0.45;

/// Fade length in seconds. The floor is not zero because an instant change reads
/// as a flicker when focus moves between windows.
pub const MIN_ANIMATION_SECS: f64 = 0.05;
pub const MAX_ANIMATION_SECS: f64 = 1.0;
pub const DEFAULT_ANIMATION_SECS: f64 = 0.25;

/// What to dim on displays other than the one holding the focused window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayMode {
    /// Each display keeps its own frontmost window bright. Better for a
    /// multi-monitor setup where the other screens hold reference material.
    #[default]
    PerScreenFront,
    /// Only the focused window anywhere is bright; every other display dims
    /// entirely.
    DimUnfocused,
}

impl DisplayMode {
    pub const ALL: [DisplayMode; 2] = [DisplayMode::PerScreenFront, DisplayMode::DimUnfocused];

    pub fn title(self) -> &'static str {
        match self {
            DisplayMode::PerScreenFront => "Keep each display's front window bright",
            DisplayMode::DimUnfocused => "Dim every display but the focused window",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            DisplayMode::PerScreenFront => "perScreenFront",
            DisplayMode::DimUnfocused => "dimUnfocused",
        }
    }

    /// Unknown values fall back to the default, as Edith's `from` does.
    pub fn parse(raw: &str) -> Self {
        DisplayMode::ALL
            .into_iter()
            .find(|mode| mode.key().eq_ignore_ascii_case(raw))
            .unwrap_or_default()
    }
}

/// A NaN is not a smaller or larger number, it is an absent one, so it falls
/// back to the default rather than clamping to a bound.
pub fn clamp_intensity(value: f64) -> f64 {
    if value.is_nan() {
        return DEFAULT_INTENSITY;
    }
    value.clamp(MIN_INTENSITY, MAX_INTENSITY)
}

pub fn clamp_animation_secs(value: f64) -> f64 {
    if value.is_nan() {
        return DEFAULT_ANIMATION_SECS;
    }
    value.clamp(MIN_ANIMATION_SECS, MAX_ANIMATION_SECS)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusDimSettings {
    pub enabled: bool,
    pub intensity: f64,
    pub animation_secs: f64,
    pub mode: DisplayMode,
}

impl Default for FocusDimSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            intensity: DEFAULT_INTENSITY,
            animation_secs: DEFAULT_ANIMATION_SECS,
            mode: DisplayMode::default(),
        }
    }
}

impl FocusDimSettings {
    pub fn read(settings: &Settings) -> Self {
        let default = Self::default();
        Self {
            enabled: settings.bool_or("focusDimEnabled", default.enabled),
            intensity: clamp_intensity(settings.f64_or("focusDimIntensity", default.intensity)),
            animation_secs: clamp_animation_secs(
                settings.f64_or("focusDimAnimationDuration", default.animation_secs),
            ),
            mode: DisplayMode::parse(settings.string("focusDimOtherDisplaysMode").unwrap_or("")),
        }
    }

    /// Intensity as a percentage, for an interface that talks in whole numbers.
    pub fn intensity_percent(&self) -> i64 {
        (self.intensity * 100.0).round() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stored(pairs: &[(&str, serde_json::Value)]) -> Settings {
        let mut settings = Settings::default();
        for (key, value) in pairs {
            settings.set(key, value.clone());
        }
        settings
    }

    #[test]
    fn defaults_match_ediths() {
        let settings = FocusDimSettings::default();
        assert!(
            !settings.enabled,
            "an unfeatured extension is off by default"
        );
        assert_eq!(settings.intensity, 0.45);
        assert_eq!(settings.animation_secs, 0.25);
        assert_eq!(settings.mode, DisplayMode::PerScreenFront);
    }

    #[test]
    fn intensity_never_reaches_fully_opaque() {
        // A dim of 1.0 hides the desktop entirely, which is a lock screen rather
        // than a focus aid, so Edith caps it below that and so does this.
        assert_eq!(clamp_intensity(1.0), MAX_INTENSITY);
        assert_eq!(clamp_intensity(50.0), MAX_INTENSITY);
        assert!(MAX_INTENSITY < 1.0);
    }

    #[test]
    fn a_negative_intensity_clamps_to_transparent() {
        assert_eq!(clamp_intensity(-3.0), MIN_INTENSITY);
    }

    #[test]
    fn a_nan_falls_back_rather_than_clamping() {
        // Clamping a NaN would silently pick a bound; it is an absent value, so
        // the default is the honest answer.
        assert_eq!(clamp_intensity(f64::NAN), DEFAULT_INTENSITY);
        assert_eq!(clamp_animation_secs(f64::NAN), DEFAULT_ANIMATION_SECS);
        // Infinities are numbers, so they clamp.
        assert_eq!(clamp_intensity(f64::INFINITY), MAX_INTENSITY);
        assert_eq!(clamp_intensity(f64::NEG_INFINITY), MIN_INTENSITY);
    }

    #[test]
    fn an_instant_fade_is_clamped_up_because_it_reads_as_a_flicker() {
        assert_eq!(clamp_animation_secs(0.0), MIN_ANIMATION_SECS);
        assert_eq!(clamp_animation_secs(30.0), MAX_ANIMATION_SECS);
        assert_eq!(clamp_animation_secs(0.3), 0.3);
    }

    #[test]
    fn a_hand_edited_settings_file_cannot_black_out_the_screen() {
        let settings = FocusDimSettings::read(&stored(&[
            ("focusDimEnabled", json!(true)),
            ("focusDimIntensity", json!(1.0)),
            ("focusDimAnimationDuration", json!(0)),
        ]));
        assert_eq!(settings.intensity, MAX_INTENSITY);
        assert_eq!(settings.animation_secs, MIN_ANIMATION_SECS);
    }

    #[test]
    fn an_unknown_mode_falls_back_to_the_default() {
        assert_eq!(DisplayMode::parse("nonsense"), DisplayMode::PerScreenFront);
        assert_eq!(DisplayMode::parse(""), DisplayMode::PerScreenFront);
        assert_eq!(
            DisplayMode::parse("dimUnfocused"),
            DisplayMode::DimUnfocused
        );
        assert_eq!(
            DisplayMode::parse("DIMUNFOCUSED"),
            DisplayMode::DimUnfocused,
            "matching is case-insensitive, as elsewhere"
        );
    }

    #[test]
    fn stored_values_are_read_back() {
        let settings = FocusDimSettings::read(&stored(&[
            ("focusDimEnabled", json!(true)),
            ("focusDimIntensity", json!(0.7)),
            ("focusDimAnimationDuration", json!(0.4)),
            ("focusDimOtherDisplaysMode", json!("dimUnfocused")),
        ]));
        assert!(settings.enabled);
        assert_eq!(settings.intensity, 0.7);
        assert_eq!(settings.animation_secs, 0.4);
        assert_eq!(settings.mode, DisplayMode::DimUnfocused);
        assert_eq!(settings.intensity_percent(), 70);
    }

    #[test]
    fn an_integer_intensity_is_accepted_because_json_has_one_number_type() {
        // `vr config set focusDimIntensity 0` stores an integer, not a float.
        let settings = FocusDimSettings::read(&stored(&[("focusDimIntensity", json!(0))]));
        assert_eq!(settings.intensity, 0.0);
    }

    #[test]
    fn mode_keys_are_unique() {
        let keys: Vec<&str> = DisplayMode::ALL.iter().map(|m| m.key()).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len());
    }
}
