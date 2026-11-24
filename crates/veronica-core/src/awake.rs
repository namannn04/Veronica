use serde::{Deserialize, Serialize};

use crate::Settings;

pub const KEEP_AWAKE_KEY: &str = "preventSleep";
pub const KEEP_AWAKE_UNTIL_KEY: &str = "preventSleepUntil";
pub const LID_AWAKE_KEY: &str = "lidAwakeEnabled";
pub const LID_AWAKE_UNTIL_KEY: &str = "lidAwakeUntil";

pub const MAX_DURATION_SECS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Awake {
    KeepAwake,
    LidAwake,
}

impl Awake {
    pub fn title(self) -> &'static str {
        match self {
            Self::KeepAwake => "Keep Awake",
            Self::LidAwake => "Lid Awake",
        }
    }

    pub fn enabled_key(self) -> &'static str {
        match self {
            Self::KeepAwake => KEEP_AWAKE_KEY,
            Self::LidAwake => LID_AWAKE_KEY,
        }
    }

    pub fn until_key(self) -> &'static str {
        match self {
            Self::KeepAwake => KEEP_AWAKE_UNTIL_KEY,
            Self::LidAwake => LID_AWAKE_UNTIL_KEY,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AwakeState {
    pub enabled: bool,
    pub until_ms: Option<i64>,
}

impl AwakeState {
    pub fn read(settings: &Settings, switch: Awake) -> Self {
        Self {
            enabled: settings.bool_or(switch.enabled_key(), false),
            until_ms: settings
                .get(switch.until_key())
                .and_then(serde_json::Value::as_i64)
                .filter(|until| *until > 0),
        }
    }

    pub fn is_active(&self, now_ms: i64) -> bool {
        self.enabled && self.until_ms.is_none_or(|until| until > now_ms)
    }

    pub fn has_expired(&self, now_ms: i64) -> bool {
        self.enabled && self.until_ms.is_some_and(|until| until <= now_ms)
    }

    pub fn remaining_secs(&self, now_ms: i64) -> Option<i64> {
        let until = self.until_ms?;
        if !self.enabled || until <= now_ms {
            return None;
        }
        Some((until - now_ms) / 1000)
    }
}

pub fn parse_duration(raw: &str) -> anyhow::Result<i64> {
    let text = raw.trim().to_lowercase();
    if text.is_empty() {
        anyhow::bail!("no duration given; try 30m, 2h or 90s");
    }

    if let Ok(minutes) = text.parse::<i64>() {
        return checked(minutes.saturating_mul(60), raw);
    }

    let mut total: i64 = 0;
    let mut digits = String::new();
    let mut saw_unit = false;
    for character in text.chars() {
        if character.is_ascii_digit() {
            digits.push(character);
            continue;
        }
        let value: i64 = digits
            .parse()
            .map_err(|_| anyhow::anyhow!("not a duration: {raw:?}; try 30m, 2h or 90s"))?;
        digits.clear();
        let scale = match character {
            's' => 1,
            'm' => 60,
            'h' => 3_600,
            'd' => 86_400,
            _ => anyhow::bail!("unknown unit {character:?} in {raw:?}; use s, m, h or d"),
        };
        total = total.saturating_add(value.saturating_mul(scale));
        saw_unit = true;
    }
    if !saw_unit || !digits.is_empty() {
        anyhow::bail!("not a duration: {raw:?}; try 30m, 2h or 90s");
    }
    checked(total, raw)
}

fn checked(seconds: i64, raw: &str) -> anyhow::Result<i64> {
    if seconds <= 0 {
        anyhow::bail!("a duration has to be longer than zero: {raw:?}");
    }
    if seconds > MAX_DURATION_SECS {
        anyhow::bail!(
            "{raw:?} is longer than the {} hour maximum",
            MAX_DURATION_SECS / 3_600
        );
    }
    Ok(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MINUTE: i64 = 60_000;

    fn stored(pairs: &[(&str, serde_json::Value)]) -> Settings {
        let mut settings = Settings::default();
        for (key, value) in pairs {
            settings.set(key, value.clone());
        }
        settings
    }

    #[test]
    fn the_two_switches_use_distinct_keys() {
        assert_ne!(
            Awake::KeepAwake.enabled_key(),
            Awake::LidAwake.enabled_key()
        );
        assert_ne!(Awake::KeepAwake.until_key(), Awake::LidAwake.until_key());
        assert_eq!(Awake::KeepAwake.enabled_key(), "preventSleep");
        assert_eq!(Awake::LidAwake.enabled_key(), "lidAwakeEnabled");
    }

    #[test]
    fn a_switch_with_no_deadline_stays_on() {
        let state = AwakeState::read(
            &stored(&[("lidAwakeEnabled", json!(true))]),
            Awake::LidAwake,
        );
        assert!(state.is_active(0));
        assert!(state.is_active(i64::MAX));
        assert_eq!(state.remaining_secs(0), None, "open-ended has no countdown");
        assert!(!state.has_expired(i64::MAX));
    }

    #[test]
    fn a_timed_session_is_active_until_its_deadline_and_not_after() {
        let state = AwakeState {
            enabled: true,
            until_ms: Some(30 * MINUTE),
        };
        assert!(state.is_active(29 * MINUTE));
        assert!(!state.is_active(30 * MINUTE), "the deadline is the end");
        assert!(!state.is_active(31 * MINUTE));
    }

    #[test]
    fn an_expired_session_is_reported_for_clearing_rather_than_left_stale() {
        let state = AwakeState {
            enabled: true,
            until_ms: Some(MINUTE),
        };
        assert!(!state.has_expired(0));
        assert!(state.has_expired(MINUTE));
        let open = AwakeState {
            enabled: true,
            until_ms: None,
        };
        assert!(!open.has_expired(i64::MAX));
        let off = AwakeState {
            enabled: false,
            until_ms: Some(MINUTE),
        };
        assert!(!off.has_expired(i64::MAX));
    }

    #[test]
    fn a_deadline_that_passed_while_the_app_was_closed_is_over_not_restarted() {
        let state = AwakeState::read(
            &stored(&[
                ("lidAwakeEnabled", json!(true)),
                ("lidAwakeUntil", json!(1_000)),
            ]),
            Awake::LidAwake,
        );
        assert!(state.has_expired(2_000));
        assert!(!state.is_active(2_000));
    }

    #[test]
    fn the_countdown_counts_down() {
        let state = AwakeState {
            enabled: true,
            until_ms: Some(90_000),
        };
        assert_eq!(state.remaining_secs(0), Some(90));
        assert_eq!(state.remaining_secs(60_000), Some(30));
        assert_eq!(
            state.remaining_secs(90_000),
            None,
            "nothing left is nothing"
        );
    }

    #[test]
    fn a_stored_deadline_of_zero_or_less_is_no_deadline() {
        for value in [json!(0), json!(-5)] {
            let state = AwakeState::read(
                &stored(&[("preventSleep", json!(true)), ("preventSleepUntil", value)]),
                Awake::KeepAwake,
            );
            assert_eq!(state.until_ms, None);
            assert!(state.is_active(i64::MAX));
        }
    }

    #[test]
    fn durations_parse_the_way_they_are_typed() {
        assert_eq!(parse_duration("90s").unwrap(), 90);
        assert_eq!(parse_duration("30m").unwrap(), 1_800);
        assert_eq!(parse_duration("2h").unwrap(), 7_200);
        assert_eq!(parse_duration("1h30m").unwrap(), 5_400);
        assert_eq!(
            parse_duration(" 45M ").unwrap(),
            2_700,
            "case and space are fine"
        );
    }

    #[test]
    fn a_bare_number_means_minutes() {
        assert_eq!(parse_duration("30").unwrap(), 1_800);
    }

    #[test]
    fn nonsense_is_refused_rather_than_read_as_something_else() {
        for raw in ["", "soon", "30x", "m", "30m20", "-5"] {
            assert!(parse_duration(raw).is_err(), "{raw:?} should not parse");
        }
    }

    #[test]
    fn a_session_cannot_be_longer_than_a_day() {
        assert_eq!(parse_duration("24h").unwrap(), MAX_DURATION_SECS);
        assert!(parse_duration("25h").is_err());
        assert!(parse_duration("7d").is_err());
    }
}
