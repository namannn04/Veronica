//! Keep Awake and Lid Awake, including the timed sessions.
//!
//! Ported from Edith's Lid Awake: the same two switches, and the same idea that
//! a session can be given a deadline rather than being left on until somebody
//! remembers to turn it off. `ed lid-awake on --for 30m` is the front-page
//! example, and `vr power lid-awake on --for 30m` is the same thing.
//!
//! The lock itself is a systemd-logind inhibitor held by the running desktop
//! app, which is why the CLI writes the setting rather than taking the lock: a
//! command that exits cannot hold a file descriptor open. The app watches the
//! settings file and acquires or releases to match — the same route the notch's
//! quick actions already take.
//!
//! The deadline lives here rather than in a timer, so it survives a restart. An
//! expiry that had passed while the app was closed is honest about it: the
//! session is over, not silently extended to a fresh half hour.

use serde::{Deserialize, Serialize};

use crate::Settings;

pub const KEEP_AWAKE_KEY: &str = "preventSleep";
pub const KEEP_AWAKE_UNTIL_KEY: &str = "preventSleepUntil";
pub const LID_AWAKE_KEY: &str = "lidAwakeEnabled";
pub const LID_AWAKE_UNTIL_KEY: &str = "lidAwakeUntil";

/// The longest session that can be asked for. A day is already far past what
/// anyone means by "keep it awake for a bit", and an unbounded value would let
/// a typo pin a laptop awake in a bag.
pub const MAX_DURATION_SECS: i64 = 24 * 60 * 60;

/// Which switch a command is talking about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Awake {
    /// Blocks the idle sleep timer only.
    KeepAwake,
    /// Blocks the idle timer *and* the lid switch, so a closed laptop keeps
    /// running.
    LidAwake,
}

impl Awake {
    pub fn title(self) -> &'static str {
        match self {
            Awake::KeepAwake => "Keep Awake",
            Awake::LidAwake => "Lid Awake",
        }
    }

    pub fn enabled_key(self) -> &'static str {
        match self {
            Awake::KeepAwake => KEEP_AWAKE_KEY,
            Awake::LidAwake => LID_AWAKE_KEY,
        }
    }

    pub fn until_key(self) -> &'static str {
        match self {
            Awake::KeepAwake => KEEP_AWAKE_UNTIL_KEY,
            Awake::LidAwake => LID_AWAKE_UNTIL_KEY,
        }
    }
}

/// One switch's state, as stored.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AwakeState {
    pub enabled: bool,
    /// When the session ends, in milliseconds since the Unix epoch. `None` is
    /// an open-ended session, which is what a plain switch means.
    pub until_ms: Option<i64>,
}

impl AwakeState {
    pub fn read(settings: &Settings, switch: Awake) -> Self {
        Self {
            enabled: settings.bool_or(switch.enabled_key(), false),
            until_ms: settings
                .get(switch.until_key())
                .and_then(serde_json::Value::as_i64)
                // A deadline in the past is not a deadline; treating it as one
                // would leave a stale timestamp reported as a live countdown.
                .filter(|until| *until > 0),
        }
    }

    /// Whether the lock should be held right now.
    ///
    /// A timed session that has run out is off, even though `enabled` is still
    /// true in the file: the app clears the pair on its next tick, and until
    /// then this is the answer that matters.
    pub fn is_active(&self, now_ms: i64) -> bool {
        self.enabled && self.until_ms.is_none_or(|until| until > now_ms)
    }

    /// Whether the stored pair should be cleared, i.e. a timed session whose
    /// deadline has passed.
    pub fn has_expired(&self, now_ms: i64) -> bool {
        self.enabled && self.until_ms.is_some_and(|until| until <= now_ms)
    }

    /// Seconds left, or `None` for an open-ended or inactive session.
    pub fn remaining_secs(&self, now_ms: i64) -> Option<i64> {
        let until = self.until_ms?;
        if !self.enabled || until <= now_ms {
            return None;
        }
        Some((until - now_ms) / 1000)
    }
}

/// Parse `30m`, `2h`, `90s`, `1h30m`, or a bare number of minutes.
///
/// A bare number means minutes because that is what `--for 30` means to anyone
/// typing it; seconds would make `--for 30` half a minute of keeping awake,
/// which is not a thing anybody wants.
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
        // The enabled keys are the ones the app already watches.
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
        // An open-ended session never expires, so it is never cleared.
        let open = AwakeState {
            enabled: true,
            until_ms: None,
        };
        assert!(!open.has_expired(i64::MAX));
        // Nor does a switch that is already off.
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
        // A hand-edited file, or a value written before the epoch: reading it as
        // a live countdown would report a session ending in 1970.
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
        // `--for 30` meaning thirty seconds would be useless and surprising.
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
        // A typo must not pin a laptop awake in a bag indefinitely.
        assert_eq!(parse_duration("24h").unwrap(), MAX_DURATION_SECS);
        assert!(parse_duration("25h").is_err());
        assert!(parse_duration("7d").is_err());
    }
}
