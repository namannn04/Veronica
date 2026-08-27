//! Picked-colour history.
//!
//! Ported from Edith's `ColorSwatch`, `ColorFormatting` and
//! `ColorHistoryStore`: the same fields, the same formatting maths and the same
//! prepend-and-cap history, so a colour picked on either platform reads the
//! same way.
//!
//! Two of Edith's five copy formats are AppKit literals (`Color(...)` and
//! `NSColor(...)`), which mean nothing on Ubuntu. Those two slots hold the
//! toolkit form Ubuntu actually uses — a `GdkRGBA` literal — plus CSS
//! `rgba()`. The three portable formats are byte-for-byte Edith's.
//!
//! The history is a plain JSON file the user can read or delete. Nothing
//! leaves the machine.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Edith's default and hard ceiling for how many swatches are kept.
pub const DEFAULT_HISTORY_SIZE: usize = 100;
pub const MAX_HISTORY_SIZE: usize = 100;

/// Colour space a swatch is recorded in.
///
/// The compositor hands back sRGB, so `DisplayP3` is a conversion rather than a
/// different sample. It is offered because many laptop panels are P3 and a
/// designer sampling one wants the P3 coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColorProfile {
    #[default]
    #[serde(alias = "sRGB")]
    Srgb,
    #[serde(alias = "displayP3")]
    DisplayP3,
}

impl ColorProfile {
    pub fn title(self) -> &'static str {
        match self {
            ColorProfile::Srgb => "sRGB",
            ColorProfile::DisplayP3 => "Display P3",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "displayP3" | "display-p3" | "DisplayP3" | "p3" => ColorProfile::DisplayP3,
            _ => ColorProfile::Srgb,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            ColorProfile::Srgb => "srgb",
            ColorProfile::DisplayP3 => "displayP3",
        }
    }
}

/// How a swatch is written to the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CopyFormat {
    #[default]
    Hex,
    Rgb,
    Rgba,
    Hsl,
    GdkRgba,
}

impl CopyFormat {
    pub const ALL: [CopyFormat; 5] = [
        CopyFormat::Hex,
        CopyFormat::Rgb,
        CopyFormat::Rgba,
        CopyFormat::Hsl,
        CopyFormat::GdkRgba,
    ];

    pub fn title(self) -> &'static str {
        match self {
            CopyFormat::Hex => "Hex",
            CopyFormat::Rgb => "rgb()",
            CopyFormat::Rgba => "rgba()",
            CopyFormat::Hsl => "hsl()",
            CopyFormat::GdkRgba => "GdkRGBA",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            CopyFormat::Hex => "hex",
            CopyFormat::Rgb => "rgb",
            CopyFormat::Rgba => "rgba",
            CopyFormat::Hsl => "hsl",
            CopyFormat::GdkRgba => "gdkRgba",
        }
    }

    /// Unknown values fall back to hex rather than failing, matching how Edith
    /// reads the same setting.
    pub fn parse(raw: &str) -> Self {
        CopyFormat::ALL
            .into_iter()
            .find(|format| format.key().eq_ignore_ascii_case(raw))
            .unwrap_or(CopyFormat::Hex)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Swatch {
    /// Stable id, assigned when recorded. Matches how the clipboard history
    /// identifies its rows, so both lists behave the same in the interface.
    pub id: u64,
    /// Components in 0..=1, in `profile`'s space.
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub profile: ColorProfile,
    pub picked_at: DateTime<Utc>,
}

impl Swatch {
    pub fn format(&self, format: CopyFormat) -> String {
        formatting::string(self.red, self.green, self.blue, format)
    }

    /// Always available regardless of the configured format, because the
    /// interface shows the hex under every swatch.
    pub fn hex(&self) -> String {
        formatting::hex(self.red, self.green, self.blue)
    }

    /// Whether white or black text is legible on this colour, by WCAG relative
    /// luminance. The swatch grid prints its hex on top of the colour.
    pub fn prefers_dark_text(&self) -> bool {
        formatting::relative_luminance(self.red, self.green, self.blue) > 0.45
    }
}

pub mod formatting {
    //! The conversions, kept as free functions so the CLI can format a colour
    //! it was handed without building a `Swatch` first.

    use super::CopyFormat;

    pub fn string(red: f64, green: f64, blue: f64, format: CopyFormat) -> String {
        match format {
            CopyFormat::Hex => hex(red, green, blue),
            CopyFormat::Rgb => rgb(red, green, blue),
            CopyFormat::Rgba => rgba(red, green, blue),
            CopyFormat::Hsl => hsl(red, green, blue),
            CopyFormat::GdkRgba => gdk_rgba(red, green, blue),
        }
    }

    pub fn hex(red: f64, green: f64, blue: f64) -> String {
        format!(
            "#{:02X}{:02X}{:02X}",
            byte(red),
            byte(green),
            byte(blue)
        )
    }

    pub fn rgb(red: f64, green: f64, blue: f64) -> String {
        format!("rgb({}, {}, {})", byte(red), byte(green), byte(blue))
    }

    pub fn rgba(red: f64, green: f64, blue: f64) -> String {
        format!("rgba({}, {}, {}, 1)", byte(red), byte(green), byte(blue))
    }

    pub fn hsl(red: f64, green: f64, blue: f64) -> String {
        let (h, s, l) = rgb_to_hsl(red, green, blue);
        format!(
            "hsl({}, {}%, {}%)",
            (h * 360.0).round() as i64,
            (s * 100.0).round() as i64,
            (l * 100.0).round() as i64
        )
    }

    pub fn gdk_rgba(red: f64, green: f64, blue: f64) -> String {
        format!(
            "GdkRGBA {{ red: {}, green: {}, blue: {}, alpha: 1.0 }}",
            decimal(red),
            decimal(green),
            decimal(blue)
        )
    }

    /// Ported verbatim from Edith's `rgbToHSL`, hue normalised to 0..1.
    pub fn rgb_to_hsl(red: f64, green: f64, blue: f64) -> (f64, f64, f64) {
        let max_v = red.max(green).max(blue);
        let min_v = red.min(green).min(blue);
        let l = (max_v + min_v) / 2.0;
        if (max_v - min_v).abs() < f64::EPSILON {
            return (0.0, 0.0, l);
        }
        let d = max_v - min_v;
        let s = if l > 0.5 {
            d / (2.0 - max_v - min_v)
        } else {
            d / (max_v + min_v)
        };
        let h = if max_v == red {
            ((green - blue) / d + if green < blue { 6.0 } else { 0.0 }) / 6.0
        } else if max_v == green {
            ((blue - red) / d + 2.0) / 6.0
        } else {
            ((red - green) / d + 4.0) / 6.0
        };
        (h, s, l)
    }

    /// WCAG relative luminance, used only to choose a legible label colour.
    pub fn relative_luminance(red: f64, green: f64, blue: f64) -> f64 {
        let channel = |c: f64| {
            let c = c.clamp(0.0, 1.0);
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue)
    }

    pub fn byte(value: f64) -> u8 {
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    }

    fn decimal(value: f64) -> String {
        format!("{:.4}", value.clamp(0.0, 1.0))
    }
}

/// Convert an sRGB triple to Display P3, both gamma-encoded and in 0..=1.
///
/// The compositor only ever reports sRGB, so this is what makes the profile
/// setting mean something rather than relabel the same numbers. sRGB and P3
/// share the D65 white point, so this is decode, one 3×3 matrix, re-encode,
/// with no chromatic adaptation needed.
pub fn srgb_to_display_p3(red: f64, green: f64, blue: f64) -> (f64, f64, f64) {
    // sRGB primaries to XYZ, then XYZ to P3 primaries, pre-multiplied.
    const M: [[f64; 3]; 3] = [
        [0.822_461_969_1, 0.177_538_030_9, 0.0],
        [0.033_194_199_2, 0.966_805_800_8, 0.0],
        [0.017_082_631_0, 0.072_397_402_1, 0.910_519_967_0],
    ];
    let linear = [
        srgb_decode(red),
        srgb_decode(green),
        srgb_decode(blue),
    ];
    let mut out = [0.0f64; 3];
    for (row, coefficients) in M.iter().enumerate() {
        out[row] = coefficients
            .iter()
            .zip(linear.iter())
            .map(|(m, v)| m * v)
            .sum();
    }
    (
        srgb_encode(out[0]),
        srgb_encode(out[1]),
        srgb_encode(out[2]),
    )
}

/// Display P3 uses the sRGB transfer function, so both spaces share these.
fn srgb_decode(value: f64) -> f64 {
    let v = value.clamp(0.0, 1.0);
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb_encode(value: f64) -> f64 {
    let v = value.clamp(0.0, 1.0);
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SwatchHistory {
    #[serde(default)]
    swatches: Vec<Swatch>,
    #[serde(default)]
    next_id: u64,
}

impl SwatchHistory {
    /// A missing file is an empty history, not an error: that is a first pick.
    /// A corrupt file is also empty rather than fatal, because losing a colour
    /// history must never stop the app from starting.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    /// Written atomically, so a crash mid-write cannot truncate the file.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_vec_pretty(self)?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, &body)?;
        std::fs::rename(&temp, path)
            .with_context(|| format!("cannot replace {}", path.display()))?;
        Ok(())
    }

    pub fn swatches(&self) -> &[Swatch] {
        &self.swatches
    }

    pub fn len(&self) -> usize {
        self.swatches.len()
    }

    pub fn is_empty(&self) -> bool {
        self.swatches.is_empty()
    }

    pub fn get(&self, id: u64) -> Option<&Swatch> {
        self.swatches.iter().find(|swatch| swatch.id == id)
    }

    /// Clamp a configured history size the way Edith does, so a hand-edited
    /// settings file cannot ask for an unbounded history.
    pub fn clamp_limit(requested: usize) -> usize {
        requested.clamp(1, MAX_HISTORY_SIZE)
    }

    /// Record a pick. Newest first, capped, and — like Edith — every pick is
    /// kept even if the same colour was sampled before, because the history is
    /// a record of picks rather than a set of colours.
    pub fn record(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        profile: ColorProfile,
        now: DateTime<Utc>,
        limit: usize,
    ) -> Swatch {
        self.next_id = self.next_id.max(
            self.swatches
                .iter()
                .map(|swatch| swatch.id)
                .max()
                .unwrap_or(0),
        ) + 1;
        let swatch = Swatch {
            id: self.next_id,
            red: red.clamp(0.0, 1.0),
            green: green.clamp(0.0, 1.0),
            blue: blue.clamp(0.0, 1.0),
            profile,
            picked_at: now,
        };
        self.swatches.insert(0, swatch.clone());
        self.swatches.truncate(Self::clamp_limit(limit));
        swatch
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.swatches.len();
        self.swatches.retain(|swatch| swatch.id != id);
        self.swatches.len() != before
    }

    pub fn clear(&mut self) {
        self.swatches.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_787_000_000 + secs, 0).unwrap()
    }

    #[test]
    fn hex_matches_ediths_uppercase_two_digit_form() {
        assert_eq!(formatting::hex(1.0, 1.0, 1.0), "#FFFFFF");
        assert_eq!(formatting::hex(0.0, 0.0, 0.0), "#000000");
        // 0.5 * 255 = 127.5, and Edith rounds rather than truncates.
        assert_eq!(formatting::hex(0.5, 0.5, 0.5), "#808080");
    }

    #[test]
    fn every_format_renders_the_same_colour() {
        let red = 0.2;
        let green = 0.4;
        let blue = 0.6;
        assert_eq!(formatting::string(red, green, blue, CopyFormat::Hex), "#336699");
        assert_eq!(
            formatting::string(red, green, blue, CopyFormat::Rgb),
            "rgb(51, 102, 153)"
        );
        assert_eq!(
            formatting::string(red, green, blue, CopyFormat::Rgba),
            "rgba(51, 102, 153, 1)"
        );
        assert_eq!(
            formatting::string(red, green, blue, CopyFormat::Hsl),
            "hsl(210, 50%, 40%)"
        );
        assert_eq!(
            formatting::string(red, green, blue, CopyFormat::GdkRgba),
            "GdkRGBA { red: 0.2000, green: 0.4000, blue: 0.6000, alpha: 1.0 }"
        );
    }

    #[test]
    fn a_grey_has_no_hue_or_saturation() {
        let (h, s, l) = formatting::rgb_to_hsl(0.5, 0.5, 0.5);
        assert_eq!(h, 0.0);
        assert_eq!(s, 0.0);
        assert_eq!(l, 0.5);
    }

    #[test]
    fn hue_lands_on_the_right_sextant_for_each_primary() {
        for (rgb, degrees) in [
            ((1.0, 0.0, 0.0), 0),
            ((0.0, 1.0, 0.0), 120),
            ((0.0, 0.0, 1.0), 240),
        ] {
            let (h, s, _) = formatting::rgb_to_hsl(rgb.0, rgb.1, rgb.2);
            assert_eq!((h * 360.0).round() as i64, degrees, "{rgb:?}");
            assert_eq!(s, 1.0, "a primary is fully saturated");
        }
    }

    #[test]
    fn a_blue_below_green_wraps_the_hue_forward_rather_than_negative() {
        // Edith's `+6` branch: red is the maximum and blue exceeds green.
        let (h, _, _) = formatting::rgb_to_hsl(1.0, 0.0, 0.5);
        assert!(h > 0.5, "hue should wrap to the magenta side, got {h}");
    }

    #[test]
    fn out_of_range_components_are_clamped_not_wrapped() {
        assert_eq!(formatting::hex(2.0, -1.0, 0.5), "#FF0080");
    }

    #[test]
    fn unknown_format_and_profile_names_fall_back_to_the_defaults() {
        assert_eq!(CopyFormat::parse("nsColor"), CopyFormat::Hex);
        assert_eq!(CopyFormat::parse("HSL"), CopyFormat::Hsl);
        assert_eq!(ColorProfile::parse("whatever"), ColorProfile::Srgb);
        assert_eq!(ColorProfile::parse("displayP3"), ColorProfile::DisplayP3);
    }

    #[test]
    fn p3_conversion_keeps_greys_and_pulls_saturated_colours_in() {
        // Both spaces share a white point and transfer function, so the
        // extremes and the greys must survive the round trip untouched.
        for level in [0.0, 0.5, 1.0] {
            let (r, g, b) = srgb_to_display_p3(level, level, level);
            for channel in [r, g, b] {
                assert!(
                    (channel - level).abs() < 1e-6,
                    "grey {level} moved to {channel}"
                );
            }
        }
        // P3 is the wider gamut, so pure sRGB red needs less of the P3 red
        // primary and picks up a little green.
        let (r, g, b) = srgb_to_display_p3(1.0, 0.0, 0.0);
        assert!(r < 1.0 && r > 0.9, "P3 red was {r}");
        assert!(g > 0.0, "P3 red should not be pure, green was {g}");
        assert!(b > 0.0, "P3 red should not be pure, blue was {b}");
    }

    #[test]
    fn newest_pick_comes_first_and_gets_a_fresh_id() {
        let mut history = SwatchHistory::default();
        let first = history.record(1.0, 0.0, 0.0, ColorProfile::Srgb, at(0), 10);
        let second = history.record(0.0, 1.0, 0.0, ColorProfile::Srgb, at(10), 10);
        assert_ne!(first.id, second.id);
        assert_eq!(history.swatches()[0].id, second.id);
        assert_eq!(history.len(), 2);
        assert_eq!(history.get(first.id).map(Swatch::hex).unwrap(), "#FF0000");
    }

    #[test]
    fn the_same_colour_picked_twice_is_two_entries_as_in_edith() {
        let mut history = SwatchHistory::default();
        history.record(0.1, 0.2, 0.3, ColorProfile::Srgb, at(0), 10);
        history.record(0.1, 0.2, 0.3, ColorProfile::Srgb, at(5), 10);
        assert_eq!(history.len(), 2, "the history records picks, not a colour set");
    }

    #[test]
    fn the_history_is_capped_and_the_cap_is_clamped() {
        let mut history = SwatchHistory::default();
        for index in 0..5 {
            history.record(0.0, 0.0, 0.0, ColorProfile::Srgb, at(index), 3);
        }
        assert_eq!(history.len(), 3);

        // A settings file asking for zero or a million still gets a usable cap.
        assert_eq!(SwatchHistory::clamp_limit(0), 1);
        assert_eq!(SwatchHistory::clamp_limit(1_000_000), MAX_HISTORY_SIZE);
        assert_eq!(SwatchHistory::clamp_limit(50), 50);
    }

    #[test]
    fn ids_stay_unique_after_the_newest_entries_are_removed() {
        // Reusing an id would make the interface act on the wrong swatch.
        let mut history = SwatchHistory::default();
        let a = history.record(0.0, 0.0, 0.0, ColorProfile::Srgb, at(0), 10);
        let b = history.record(0.1, 0.1, 0.1, ColorProfile::Srgb, at(1), 10);
        assert!(history.remove(b.id));
        let c = history.record(0.2, 0.2, 0.2, ColorProfile::Srgb, at(2), 10);
        assert_ne!(c.id, b.id);
        assert_ne!(c.id, a.id);
        assert!(!history.remove(b.id), "removing twice reports nothing removed");
    }

    #[test]
    fn a_missing_or_corrupt_file_reads_as_an_empty_history() {
        let dir = std::env::temp_dir().join(format!("veronica-swatch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let missing = dir.join("absent.json");
        assert!(SwatchHistory::load(&missing).unwrap().is_empty());

        let corrupt = dir.join("corrupt.json");
        std::fs::write(&corrupt, b"{ not json").unwrap();
        assert!(
            SwatchHistory::load(&corrupt).unwrap().is_empty(),
            "a damaged history must not stop the app from starting"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_saved_history_round_trips() {
        let dir = std::env::temp_dir().join(format!("veronica-swatch-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("swatches.json");

        let mut history = SwatchHistory::default();
        history.record(0.2, 0.4, 0.6, ColorProfile::DisplayP3, at(0), 10);
        history.save(&path).unwrap();

        let reloaded = SwatchHistory::load(&path).unwrap();
        assert_eq!(reloaded, history);
        assert_eq!(reloaded.swatches()[0].profile, ColorProfile::DisplayP3);
        assert_eq!(reloaded.swatches()[0].hex(), "#336699");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clearing_empties_the_list_but_keeps_ids_moving_forward() {
        let mut history = SwatchHistory::default();
        let first = history.record(0.0, 0.0, 0.0, ColorProfile::Srgb, at(0), 10);
        history.clear();
        assert!(history.is_empty());
        let next = history.record(0.0, 0.0, 0.0, ColorProfile::Srgb, at(1), 10);
        assert!(next.id > first.id, "an id must not be handed out twice");
    }

    #[test]
    fn label_colour_follows_the_luminance_of_the_swatch() {
        let light = Swatch {
            id: 1,
            red: 1.0,
            green: 1.0,
            blue: 0.9,
            profile: ColorProfile::Srgb,
            picked_at: at(0),
        };
        let dark = Swatch { red: 0.05, green: 0.05, blue: 0.1, ..light.clone() };
        assert!(light.prefers_dark_text());
        assert!(!dark.prefers_dark_text());
    }
}
