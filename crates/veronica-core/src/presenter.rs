use serde::{Deserialize, Serialize};

use crate::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlurCategory {
    Money,
    Usage,
    Agents,
    Calendar,
    Music,
}

impl BlurCategory {
    pub const ALL: [Self; 5] = [
        Self::Money,
        Self::Usage,
        Self::Agents,
        Self::Calendar,
        Self::Music,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Money => "Spend",
            Self::Usage => "Tokens and charts",
            Self::Agents => "Rate limits",
            Self::Calendar => "Calendar entries",
            Self::Music => "Track names",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Money => "presenterBlurMoney",
            Self::Usage => "presenterBlurUsage",
            Self::Agents => "presenterBlurAgents",
            Self::Calendar => "presenterBlurCalendar",
            Self::Music => "presenterBlurMusic",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::Money => "blur-money",
            Self::Usage => "blur-usage",
            Self::Agents => "blur-agents",
            Self::Calendar => "blur-calendar",
            Self::Music => "blur-music",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenterState {
    pub enabled: bool,
    pub manual: bool,
    pub auto_enabled: bool,
    pub auto_active: bool,
    pub auto_paused: bool,
    pub auto_reason: Option<String>,
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
            categories: BlurCategory::ALL.to_vec(),
        }
    }
}

impl PresenterState {
    pub fn active(&self) -> bool {
        self.enabled && (self.manual || self.effective_auto())
    }

    pub fn effective_auto(&self) -> bool {
        self.auto_active && self.auto_enabled && !self.auto_paused
    }

    pub fn blurs(&self, category: BlurCategory) -> bool {
        self.active() && self.categories.contains(&category)
    }

    pub fn read(settings: &Settings) -> Self {
        let default = Self::default();
        let enabled = settings.bool_or("presenterEnabled", default.enabled);
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
        assert_eq!(
            state.auto_reason.as_deref(),
            Some("Screen sharing is active")
        );
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

        let mut manual = state;
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
            assert!(
                !state.blurs(category),
                "{category:?} blurred while inactive"
            );
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
