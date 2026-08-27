//! Presenter mode.
//!
//! Blurs figures that should not be on a shared screen: spend, token counts,
//! rate-limit percentages, calendar entries and track names.
//!
//! Ported from Edith's `PresenterState`, `PresenterRuntimeOperation` and
//! `FeatureGates.presenterActive`, with the same keys and the same gate:
//!
//! ```text
//! active = enabled && (manual || auto_active)
//! ```
//!
//! Three switches rather than one, because they answer different questions.
//! `enabled` is whether the extension exists at all; `manual` is the user
//! flipping it; `auto_active` is Veronica noticing a screen share. Collapsing
//! them would mean a detected share could not be dismissed without also turning
//! the feature off.
//!
//! What differs from macOS is only the detection: Edith watches for a display
//! being captured or mirrored, and on Ubuntu the equivalent signal is an active
//! compositor screencast session. That lives in `veronica-system`; everything
//! here is the shared model both platforms agree on.

use serde::{Deserialize, Serialize};

use crate::Settings;

/// The categories that can be blurred independently, so a demo can show the
/// calendar while hiding spend, or the reverse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlurCategory {
    /// Money: spend totals and per-day cost.
    Money,
    /// Token counts and the usage charts.
    Usage,
    /// Rate-limit percentages and countdowns.
    Agents,
    /// Event titles and attendees.
    Calendar,
    /// Track, artist and album.
    Music,
}

impl BlurCategory {
    pub const ALL: [BlurCategory; 5] = [
        BlurCategory::Money,
        BlurCategory::Usage,
        BlurCategory::Agents,
        BlurCategory::Calendar,
        BlurCategory::Music,
    ];

    pub fn title(self) -> &'static str {
        match self {
            BlurCategory::Money => "Spend",
            BlurCategory::Usage => "Tokens and charts",
            BlurCategory::Agents => "Rate limits",
            BlurCategory::Calendar => "Calendar entries",
            BlurCategory::Music => "Track names",
        }
    }

    /// The settings key, matching Edith's.
    pub fn key(self) -> &'static str {
        match self {
            BlurCategory::Money => "presenterBlurMoney",
            BlurCategory::Usage => "presenterBlurUsage",
            BlurCategory::Agents => "presenterBlurAgents",
            BlurCategory::Calendar => "presenterBlurCalendar",
            BlurCategory::Music => "presenterBlurMusic",
        }
    }

    /// The class the interface puts on an element of this category.
    pub fn css_class(self) -> &'static str {
        match self {
            BlurCategory::Money => "blur-money",
            BlurCategory::Usage => "blur-usage",
            BlurCategory::Agents => "blur-agents",
            BlurCategory::Calendar => "blur-calendar",
            BlurCategory::Music => "blur-music",
        }
    }
}

/// Everything the interface needs to decide what to blur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenterState {
    /// The extension switch. Off means the feature does nothing at all, and no
    /// detection runs.
    pub enabled: bool,
    /// The user's own toggle.
    pub manual: bool,
    /// Automatic detection is allowed.
    pub auto_enabled: bool,
    /// A share is happening now.
    pub auto_active: bool,
    /// The user dismissed the current automatic activation. Cleared when the
    /// share stops, so the next one activates again.
    pub auto_paused: bool,
    /// What was detected, e.g. "Screen sharing is active".
    pub auto_reason: Option<String>,
    /// Which categories are blurred while active.
    pub categories: Vec<BlurCategory>,
}

impl Default for PresenterState {
    fn default() -> Self {
        Self {
            enabled: false,
            manual: false,
            auto_enabled: true,
            auto_active: false,
            auto_paused: false,
            auto_reason: None,
            // Every category on by default: someone turning presenter mode on
            // wants everything sensitive covered, and can then reveal a category
            // deliberately.
            categories: BlurCategory::ALL.to_vec(),
        }
    }
}

impl PresenterState {
    /// Edith's gate, unchanged: the feature must be enabled, and either the user
    /// or the detector must have asked for it.
    pub fn active(&self) -> bool {
        self.enabled && (self.manual || self.effective_auto())
    }

    /// Whether automatic detection is currently asserting. A dismissed
    /// activation does not count, which is what makes the dismissal useful.
    pub fn effective_auto(&self) -> bool {
        self.auto_active && self.auto_enabled && !self.auto_paused
    }

    /// Whether this category is blurred right now.
    pub fn blurs(&self, category: BlurCategory) -> bool {
        self.active() && self.categories.contains(&category)
    }

    pub fn read(settings: &Settings) -> Self {
        let default = Self::default();
        let enabled = settings.bool_or("presenterEnabled", default.enabled);
        // Every dependent value is gated on `enabled`, as Edith's `refresh`
        // does, so a stale manual flag cannot blur anything once the extension
        // is switched off.
        Self {
            enabled,
            manual: enabled && settings.bool_or("presenterMode", default.manual),
            auto_enabled: settings.bool_or("presenterAutoEnabled", default.auto_enabled),
            auto_active: enabled && settings.bool_or("presenterAutoActive", default.auto_active),
            auto_paused: settings.bool_or("presenterAutoPaused", default.auto_paused),
            auto_reason: if enabled {
                settings
                    .string("presenterAutoReason")
                    .filter(|reason| !reason.trim().is_empty())
                    .map(str::to_string)
            } else {
                None
            },
            categories: BlurCategory::ALL
                .into_iter()
                .filter(|category| settings.bool_or(category.key(), true))
                .collect(),
        }
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
    fn presenter_is_off_until_the_extension_is_enabled() {
        let state = PresenterState::read(&Settings::default());
        assert!(!state.enabled);
        assert!(!state.active());

        // A manual flag left in the file must not blur anything on its own.
        let stale = PresenterState::read(&stored(&[("presenterMode", json!(true))]));
        assert!(!stale.manual, "manual is gated on enabled, as in Edith");
        assert!(!stale.active());
    }

    #[test]
    fn the_manual_switch_activates_it() {
        let state = PresenterState::read(&stored(&[
            ("presenterEnabled", json!(true)),
            ("presenterMode", json!(true)),
        ]));
        assert!(state.active());
        assert!(state.auto_reason.is_none());
    }

    #[test]
    fn a_detected_share_activates_it_without_the_manual_switch() {
        let state = PresenterState::read(&stored(&[
            ("presenterEnabled", json!(true)),
            ("presenterAutoActive", json!(true)),
            ("presenterAutoReason", json!("Screen sharing is active")),
        ]));
        assert!(!state.manual);
        assert!(state.active());
        assert_eq!(state.auto_reason.as_deref(), Some("Screen sharing is active"));
    }

    #[test]
    fn detection_can_be_switched_off_without_disabling_the_feature() {
        let state = PresenterState::read(&stored(&[
            ("presenterEnabled", json!(true)),
            ("presenterAutoEnabled", json!(false)),
            ("presenterAutoActive", json!(true)),
        ]));
        assert!(!state.effective_auto());
        assert!(!state.active(), "the manual switch is still off");
    }

    #[test]
    fn dismissing_a_detected_share_stops_the_blur_but_keeps_the_feature() {
        let state = PresenterState::read(&stored(&[
            ("presenterEnabled", json!(true)),
            ("presenterAutoActive", json!(true)),
            ("presenterAutoPaused", json!(true)),
        ]));
        assert!(state.auto_active, "the share is still happening");
        assert!(!state.effective_auto(), "but it has been dismissed");
        assert!(!state.active());

        // The manual switch overrides a dismissal, since that is the user
        // asking for it directly.
        let mut manual = state.clone();
        manual.manual = true;
        assert!(manual.active());
    }

    #[test]
    fn every_category_is_blurred_by_default() {
        let state = PresenterState::read(&stored(&[
            ("presenterEnabled", json!(true)),
            ("presenterMode", json!(true)),
        ]));
        for category in BlurCategory::ALL {
            assert!(state.blurs(category), "{category:?} should blur by default");
        }
    }

    #[test]
    fn a_category_can_be_revealed_deliberately() {
        let state = PresenterState::read(&stored(&[
            ("presenterEnabled", json!(true)),
            ("presenterMode", json!(true)),
            ("presenterBlurCalendar", json!(false)),
        ]));
        assert!(!state.blurs(BlurCategory::Calendar));
        assert!(state.blurs(BlurCategory::Money));
    }

    #[test]
    fn nothing_blurs_while_inactive_even_with_every_category_selected() {
        let state = PresenterState::read(&stored(&[("presenterEnabled", json!(true))]));
        assert_eq!(state.categories.len(), BlurCategory::ALL.len());
        for category in BlurCategory::ALL {
            assert!(!state.blurs(category), "{category:?} blurred while inactive");
        }
    }

    #[test]
    fn a_blank_reason_reads_as_no_reason() {
        let state = PresenterState::read(&stored(&[
            ("presenterEnabled", json!(true)),
            ("presenterAutoReason", json!("   ")),
        ]));
        assert_eq!(state.auto_reason, None);
    }

    #[test]
    fn keys_and_classes_are_unique_and_match_ediths_names() {
        let keys: Vec<&str> = BlurCategory::ALL.iter().map(|c| c.key()).collect();
        let classes: Vec<&str> = BlurCategory::ALL.iter().map(|c| c.css_class()).collect();
        for list in [keys.clone(), classes] {
            let mut sorted = list.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), list.len(), "duplicate in {list:?}");
        }
        assert!(keys.contains(&"presenterBlurMoney"));
        assert!(keys.contains(&"presenterBlurAgents"));
    }
}
