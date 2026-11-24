use serde::{Deserialize, Serialize};

use crate::Settings;

pub const MIN_INTENSITY: f64 = 0.0;
pub const MAX_INTENSITY: f64 = 0.9;
pub const DEFAULT_INTENSITY: f64 = 0.45;

pub const MIN_ANIMATION_SECS: f64 = 0.05;
pub const MAX_ANIMATION_SECS: f64 = 1.0;
pub const DEFAULT_ANIMATION_SECS: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayMode {
    #[default]
    PerScreenFront,
    DimUnfocused,
}

impl DisplayMode {
    pub const ALL: [Self; 2] = [Self::PerScreenFront, Self::DimUnfocused];

    pub fn title(self) -> &'static str {
        match self {
            Self::PerScreenFront => "Keep each display's front window bright",
            Self::DimUnfocused => "Dim every display but the focused window",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::PerScreenFront => "perScreenFront",
            Self::DimUnfocused => "dimUnfocused",
        }
    }

    pub fn parse(raw: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|mode| mode.key().eq_ignore_ascii_case(raw))
            .unwrap_or_default()
    }
}

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

    #[expect(
        clippy::cast_possible_truncation,
        reason = "the intensity is clamped to a fraction, so the rounded percentage always fits; std has no checked float-to-int conversion"
    )]
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
        assert_eq!(clamp_intensity(1.0), MAX_INTENSITY);
        assert_eq!(clamp_intensity(50.0), MAX_INTENSITY);
        const { assert!(MAX_INTENSITY < 1.0) };
    }

    #[test]
    fn a_negative_intensity_clamps_to_transparent() {
        assert_eq!(clamp_intensity(-3.0), MIN_INTENSITY);
    }

    #[test]
    fn a_nan_falls_back_rather_than_clamping() {
        assert_eq!(clamp_intensity(f64::NAN), DEFAULT_INTENSITY);
        assert_eq!(clamp_animation_secs(f64::NAN), DEFAULT_ANIMATION_SECS);
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
