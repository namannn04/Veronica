//! Keystroke Highlight.
//!
//! Shows each key press on screen as a keycap, for demos and screen recordings
//! where the viewer has to follow the keyboard without watching the presenter's
//! hands.
//!
//! Ported from Edith's `KeystrokeHighlightSettings` and `KeystrokeHighlightQueue`:
//! the same duration range and default, the same six-keycap ceiling, the same
//! two positions, the same settings keys.
//!
//! What differs is where the keys come from. Edith installs a listen-only
//! `CGEventTap`, which macOS grants after the user approves Input Monitoring.
//! Wayland has no such grant to give: a client cannot observe key presses meant
//! for another window, by design and with no permission that changes it. The
//! compositor can, so on Ubuntu the overlay is drawn — and the presses are read
//! — inside Veronica's GNOME Shell extension. It stays listen-only there too:
//! the extension watches events on their way past and never consumes one, so
//! typing is unaffected whether the overlay is running or not.
//!
//! This module is the part both sides agree on: what a valid duration is, what
//! the positions mean, which keys hold them, and how the queue of visible caps
//! behaves. The extension re-clamps on read, because a settings file can be
//! edited by hand.

use serde::{Deserialize, Serialize};

use crate::Settings;

/// How long one keycap stays on screen. Edith's range exactly: below half a
/// second a cap is gone before a viewer's eye reaches it, and past three the
/// row is still showing keys from the previous sentence.
pub const MIN_DURATION_SECS: f64 = 0.5;
pub const MAX_DURATION_SECS: f64 = 3.0;
pub const DEFAULT_DURATION_SECS: f64 = 1.5;

/// How many caps may be visible at once. More than this and the row is wider
/// than the content it is meant to annotate.
pub const MAXIMUM_VISIBLE: usize = 6;

/// Where the row of keycaps sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Position {
    Top,
    #[default]
    Bottom,
}

impl Position {
    pub const ALL: [Position; 2] = [Position::Top, Position::Bottom];

    pub fn title(self) -> &'static str {
        match self {
            Position::Top => "Top",
            Position::Bottom => "Bottom",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Position::Top => "top",
            Position::Bottom => "bottom",
        }
    }

    /// Unknown values fall back to the default, as Edith's `init(rawValue:)`
    /// callers do.
    pub fn parse(raw: &str) -> Self {
        Position::ALL
            .into_iter()
            .find(|position| position.key().eq_ignore_ascii_case(raw))
            .unwrap_or_default()
    }
}

/// A NaN is an absent value rather than a small or large one, so it falls back
/// to the default rather than clamping to a bound.
pub fn clamp_duration(value: f64) -> f64 {
    if value.is_nan() {
        return DEFAULT_DURATION_SECS;
    }
    value.clamp(MIN_DURATION_SECS, MAX_DURATION_SECS)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeystrokeHighlightSettings {
    /// Whether the extension is switched on at all.
    pub enabled: bool,
    /// Whether the overlay is running. Pausing removes the key monitor and
    /// hides visible caps without disabling the extension, so the shortcut
    /// keeps working — Edith's distinction, kept.
    pub active: bool,
    pub duration_secs: f64,
    pub position: Position,
}

impl Default for KeystrokeHighlightSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            active: false,
            duration_secs: DEFAULT_DURATION_SECS,
            position: Position::default(),
        }
    }
}

impl KeystrokeHighlightSettings {
    pub fn read(settings: &Settings) -> Self {
        let default = Self::default();
        Self {
            enabled: settings.bool_or("keystrokeHighlightEnabled", default.enabled),
            active: settings.bool_or("keystrokeHighlightActive", default.active),
            duration_secs: clamp_duration(
                settings.f64_or("keystrokeHighlightDuration", default.duration_secs),
            ),
            position: Position::parse(settings.string("keystrokeHighlightPosition").unwrap_or("")),
        }
    }

    /// Whether the overlay should actually be drawing. Being active while the
    /// extension is off would show keycaps the user cannot turn off from the
    /// Extensions page.
    pub fn is_running(&self) -> bool {
        self.enabled && self.active
    }
}

/// One press, as a row of labels: the modifiers, then the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    pub keys: Vec<String>,
    /// Milliseconds since the queue's own epoch, so the model needs no clock.
    pub expires_at_ms: i64,
}

/// The visible keycaps, oldest first.
///
/// Edith's `KeystrokeHighlightQueue`, with the same two rules: a press with no
/// labels is not an entry, and the queue keeps only the newest `maximum_visible`
/// so a fast typist pushes old caps out rather than filling the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queue {
    entries: Vec<Entry>,
    maximum_visible: usize,
    next_id: u64,
}

impl Default for Queue {
    fn default() -> Self {
        Self::new(MAXIMUM_VISIBLE)
    }
}

impl Queue {
    pub fn new(maximum_visible: usize) -> Self {
        Self {
            entries: Vec::new(),
            // A queue that can hold nothing would drop every press silently.
            maximum_visible: maximum_visible.max(1),
            next_id: 1,
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn maximum_visible(&self) -> usize {
        self.maximum_visible
    }

    /// Record a press. Returns the entry, or `None` when there was nothing to
    /// show — a modifier held on its own resolves to no labels.
    pub fn append(&mut self, keys: Vec<String>, now_ms: i64, duration_secs: f64) -> Option<&Entry> {
        if keys.is_empty() {
            return None;
        }
        let entry = Entry {
            id: self.next_id,
            keys,
            expires_at_ms: now_ms + (clamp_duration(duration_secs) * 1000.0).round() as i64,
        };
        self.next_id += 1;
        self.entries.push(entry);
        if self.entries.len() > self.maximum_visible {
            let excess = self.entries.len() - self.maximum_visible;
            self.entries.drain(..excess);
        }
        self.entries.last()
    }

    pub fn remove(&mut self, id: u64) {
        self.entries.retain(|entry| entry.id != id);
    }

    /// Drop everything whose time is up. `<=` rather than `<` so an entry with
    /// a zero-length life does not linger for one tick.
    pub fn remove_expired(&mut self, now_ms: i64) {
        self.entries.retain(|entry| entry.expires_at_ms > now_ms);
    }

    /// Pausing hides the row without disabling anything.
    pub fn clear(&mut self) {
        self.entries.clear();
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

    fn keys(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn defaults_match_ediths() {
        let settings = KeystrokeHighlightSettings::default();
        assert!(
            !settings.enabled,
            "an unfeatured extension is off by default"
        );
        assert!(!settings.active);
        assert_eq!(settings.duration_secs, 1.5);
        assert_eq!(settings.position, Position::Bottom);
        assert_eq!(MAXIMUM_VISIBLE, 6);
    }

    #[test]
    fn a_duration_outside_ediths_range_is_clamped_into_it() {
        assert_eq!(clamp_duration(0.0), MIN_DURATION_SECS);
        assert_eq!(clamp_duration(10.0), MAX_DURATION_SECS);
        assert_eq!(clamp_duration(2.0), 2.0);
    }

    #[test]
    fn a_nan_duration_falls_back_rather_than_clamping() {
        assert_eq!(clamp_duration(f64::NAN), DEFAULT_DURATION_SECS);
        assert_eq!(clamp_duration(f64::INFINITY), MAX_DURATION_SECS);
    }

    #[test]
    fn an_unknown_position_falls_back_to_the_default() {
        assert_eq!(Position::parse("sideways"), Position::Bottom);
        assert_eq!(Position::parse(""), Position::Bottom);
        assert_eq!(Position::parse("top"), Position::Top);
        assert_eq!(
            Position::parse("TOP"),
            Position::Top,
            "matching is case-insensitive"
        );
    }

    #[test]
    fn stored_values_are_read_back_and_a_hand_edit_cannot_pin_a_cap_on_screen() {
        let settings = KeystrokeHighlightSettings::read(&stored(&[
            ("keystrokeHighlightEnabled", json!(true)),
            ("keystrokeHighlightActive", json!(true)),
            ("keystrokeHighlightDuration", json!(600)),
            ("keystrokeHighlightPosition", json!("top")),
        ]));
        assert!(settings.enabled && settings.active);
        assert_eq!(settings.duration_secs, MAX_DURATION_SECS);
        assert_eq!(settings.position, Position::Top);
    }

    #[test]
    fn active_alone_does_not_draw_when_the_extension_is_off() {
        // Otherwise the Extensions switch would not actually stop the overlay.
        let settings =
            KeystrokeHighlightSettings::read(&stored(&[("keystrokeHighlightActive", json!(true))]));
        assert!(settings.active);
        assert!(!settings.is_running());
    }

    #[test]
    fn a_press_with_no_labels_is_not_queued() {
        // A modifier held on its own has nothing to show.
        let mut queue = Queue::default();
        assert!(queue.append(Vec::new(), 0, 1.5).is_none());
        assert!(queue.entries().is_empty());
    }

    #[test]
    fn the_queue_keeps_the_newest_six_and_drops_the_oldest() {
        let mut queue = Queue::default();
        for index in 0..9 {
            queue.append(keys(&[&index.to_string()]), 0, 1.5);
        }
        assert_eq!(queue.entries().len(), MAXIMUM_VISIBLE);
        let visible: Vec<&str> = queue
            .entries()
            .iter()
            .map(|entry| entry.keys[0].as_str())
            .collect();
        assert_eq!(visible, ["3", "4", "5", "6", "7", "8"]);
    }

    #[test]
    fn a_queue_asked_to_hold_nothing_still_holds_one() {
        let mut queue = Queue::new(0);
        assert_eq!(queue.maximum_visible(), 1);
        queue.append(keys(&["A"]), 0, 1.5);
        assert_eq!(queue.entries().len(), 1);
    }

    #[test]
    fn an_entry_expires_at_its_duration_and_not_before() {
        let mut queue = Queue::default();
        queue.append(keys(&["Ctrl", "C"]), 1_000, 1.5);
        queue.remove_expired(2_499);
        assert_eq!(queue.entries().len(), 1, "still inside its 1.5s");
        queue.remove_expired(2_500);
        assert!(queue.entries().is_empty(), "gone exactly at its deadline");
    }

    #[test]
    fn a_stored_duration_out_of_range_cannot_extend_an_entrys_life() {
        let mut queue = Queue::default();
        queue.append(keys(&["A"]), 0, 600.0);
        assert_eq!(queue.entries()[0].expires_at_ms, 3_000);
    }

    #[test]
    fn entries_carry_distinct_ids_so_one_can_be_removed_alone() {
        let mut queue = Queue::default();
        queue.append(keys(&["A"]), 0, 1.5);
        queue.append(keys(&["B"]), 0, 1.5);
        let first = queue.entries()[0].id;
        assert_ne!(first, queue.entries()[1].id);
        queue.remove(first);
        assert_eq!(queue.entries().len(), 1);
        assert_eq!(queue.entries()[0].keys, keys(&["B"]));
    }

    #[test]
    fn pausing_clears_what_is_on_screen() {
        let mut queue = Queue::default();
        queue.append(keys(&["A"]), 0, 1.5);
        queue.clear();
        assert!(queue.entries().is_empty());
    }
}
