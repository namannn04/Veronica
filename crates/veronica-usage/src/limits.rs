//! Rate-limit maths.
//!
//! A line-for-line port of Edith's `LimitMath`. Every constant and threshold is
//! kept identical so a ring in Veronica reads exactly the same as the ring on
//! macOS for the same input. The parity tests below pin the values that the
//! Swift implementation produces.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Seconds in each window kind. Claude's session window is five hours and its
/// weekly window is seven days.
pub const SESSION_WINDOW_SECS: f64 = 5.0 * 3600.0;
pub const WEEKLY_WINDOW_SECS: f64 = 7.0 * 24.0 * 3600.0;

pub const K: f64 = 5.0;
pub const PROJ_UPPER: f64 = 1.4;
pub const ABSOLUTE_LOWER: f64 = 0.50;
pub const ABSOLUTE_UPPER: f64 = 1.00;
pub const RISING_CHILL: f64 = 0.30;
pub const RISING_WARNING: f64 = 0.55;
pub const RISING_HOT: f64 = 0.78;
pub const FALLING_CHILL: f64 = 0.25;
pub const FALLING_WARNING: f64 = 0.50;
pub const FALLING_HOT: f64 = 0.73;

pub const DEFAULT_WARN_PERCENT: i64 = 60;
pub const DEFAULT_CRITICAL_PERCENT: i64 = 85;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitWindow {
    pub percent: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LimitProvider {
    Codex,
    Claude,
}

impl LimitProvider {
    pub fn label(self) -> &'static str {
        match self {
            LimitProvider::Codex => "Codex",
            LimitProvider::Claude => "Claude",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LimitWindowSlot {
    Session,
    Week,
    /// Claude's model-scoped weekly window.
    Fable,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderLimits {
    pub provider: LimitProvider,
    pub session: Option<LimitWindow>,
    pub week: Option<LimitWindow>,
    pub fable: Option<LimitWindow>,
}

impl ProviderLimits {
    pub fn is_available(&self) -> bool {
        self.session.is_some() || self.week.is_some() || self.fable.is_some()
    }

    pub fn window(&self, slot: LimitWindowSlot) -> Option<LimitWindow> {
        match slot {
            LimitWindowSlot::Session => self.session,
            LimitWindowSlot::Week => self.week,
            LimitWindowSlot::Fable => self.fable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageThresholds {
    pub warning_percent: i64,
    pub critical_percent: i64,
}

impl Default for UsageThresholds {
    fn default() -> Self {
        Self {
            warning_percent: DEFAULT_WARN_PERCENT,
            critical_percent: DEFAULT_CRITICAL_PERCENT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageLevel {
    Green = 0,
    Orange = 1,
    Red = 2,
}

impl UsageLevel {
    pub fn from_percent(pct: f64, thresholds: UsageThresholds) -> Self {
        if pct >= thresholds.critical_percent as f64 {
            UsageLevel::Red
        } else if pct >= thresholds.warning_percent as f64 {
            UsageLevel::Orange
        } else {
            UsageLevel::Green
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PacingZone {
    Chill,
    OnTrack,
    Warning,
    Hot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LimitWindowKind {
    Session,
    Weekly,
}

impl LimitWindowKind {
    pub fn duration_secs(self) -> f64 {
        match self {
            LimitWindowKind::Session => SESSION_WINDOW_SECS,
            LimitWindowKind::Weekly => WEEKLY_WINDOW_SECS,
        }
    }
}

/// Cubic smoothstep, clamped. Returns a hard step when the range is empty,
/// matching the Swift guard rather than dividing by zero.
pub fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    if a >= b {
        return if x >= b { 1.0 } else { 0.0 };
    }
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much to trust a projection: near zero early in a window, approaching one
/// as the window elapses.
pub fn confidence(e: f64) -> f64 {
    1.0 - (-K * e.max(0.0)).exp()
}

/// Blend of three risk signals: absolute usage, projected overrun, and how far
/// ahead of pace the user is. The maximum wins, so any one signal can raise the
/// alarm on its own.
pub fn combined_risk(u: f64, e: f64, m: f64) -> f64 {
    if u >= 1.0 {
        return 1.0;
    }
    let a_raw = smoothstep(ABSOLUTE_LOWER, ABSOLUTE_UPPER, u);
    let projection_health = if e > 0.0001 {
        smoothstep(0.7, 1.0, u / e)
    } else {
        1.0
    };
    let a = a_raw * projection_health;

    let b = if u > 0.0001 && e > 0.0001 {
        smoothstep(1.0, PROJ_UPPER, u / e) * confidence(e)
    } else {
        0.0
    };

    let c = smoothstep(m, m + 0.15, u - e) * confidence(e);

    a.max(b).max(c)
}

/// Risk for one window. `utilization` is a percentage, `pacing_margin` is a
/// percentage, and the result is 0...1.
pub fn smart_risk(
    utilization: f64,
    resets_at: Option<DateTime<Utc>>,
    window_duration_secs: f64,
    pacing_margin: f64,
    now: DateTime<Utc>,
) -> f64 {
    if utilization >= 100.0 {
        return 1.0;
    }
    let u = utilization.max(0.0) / 100.0;
    let Some(resets_at) = resets_at else {
        return smoothstep(ABSOLUTE_LOWER, ABSOLUTE_UPPER, u);
    };
    if window_duration_secs <= 0.0 {
        return smoothstep(ABSOLUTE_LOWER, ABSOLUTE_UPPER, u);
    }
    let remaining = (resets_at - now).num_milliseconds() as f64 / 1000.0;
    let remaining = remaining.max(0.0);
    let e = (1.0 - (remaining / window_duration_secs).min(1.0)).max(0.0);
    combined_risk(u, e, pacing_margin / 100.0)
}

pub fn level_for_risk(risk: f64) -> UsageLevel {
    if risk >= 0.78 {
        UsageLevel::Red
    } else if risk >= 0.50 {
        UsageLevel::Orange
    } else {
        UsageLevel::Green
    }
}

/// Pacing zone with hysteresis: once a zone is entered it takes a larger move
/// to leave it, so the menu bar tint does not flicker between two states.
pub fn zone_for_risk(risk: f64, previous: Option<PacingZone>) -> PacingZone {
    let r = risk.clamp(0.0, 1.0);
    let rising = |r: f64| {
        if r >= RISING_HOT {
            PacingZone::Hot
        } else if r >= RISING_WARNING {
            PacingZone::Warning
        } else if r >= RISING_CHILL {
            PacingZone::OnTrack
        } else {
            PacingZone::Chill
        }
    };

    let Some(previous) = previous else {
        return rising(r);
    };

    match previous {
        PacingZone::Chill => rising(r),
        PacingZone::OnTrack => {
            if r >= RISING_HOT {
                PacingZone::Hot
            } else if r >= RISING_WARNING {
                PacingZone::Warning
            } else if r < FALLING_CHILL {
                PacingZone::Chill
            } else {
                PacingZone::OnTrack
            }
        }
        PacingZone::Warning => {
            if r >= RISING_HOT {
                PacingZone::Hot
            } else if r < FALLING_CHILL {
                PacingZone::Chill
            } else if r < FALLING_WARNING {
                PacingZone::OnTrack
            } else {
                PacingZone::Warning
            }
        }
        PacingZone::Hot => {
            if r < FALLING_CHILL {
                PacingZone::Chill
            } else if r < FALLING_WARNING {
                PacingZone::OnTrack
            } else if r < FALLING_HOT {
                PacingZone::Warning
            } else {
                PacingZone::Hot
            }
        }
    }
}

/// How far ahead of a linear burn the user is, in percentage points.
pub fn pacing_delta(
    utilization: f64,
    resets_at: DateTime<Utc>,
    window_duration_secs: f64,
    now: DateTime<Utc>,
) -> f64 {
    let start = resets_at - chrono::Duration::milliseconds((window_duration_secs * 1000.0) as i64);
    let elapsed_secs = (now - start).num_milliseconds() as f64 / 1000.0;
    let elapsed = (elapsed_secs / window_duration_secs).clamp(0.0, 1.0);
    utilization - elapsed * 100.0
}

pub fn pacing_zone(delta: f64, margin: f64) -> PacingZone {
    if delta < -margin {
        PacingZone::Chill
    } else if delta <= margin {
        PacingZone::OnTrack
    } else if delta <= margin * 2.0 {
        PacingZone::Warning
    } else {
        PacingZone::Hot
    }
}

// -- Budgets ---------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BudgetMode {
    Cap,
    Pace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BudgetState {
    OnPace,
    Under,
    Over,
    Exceeded,
    NoData,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetStatus {
    pub state: BudgetState,
    pub target_percent: f64,
    pub actual_percent: f64,
    pub cap_percent: f64,
    pub daily_budget_percent: Option<f64>,
}

/// Where a linear spend of `cap_percent` should have reached by `now`.
pub fn budget_target(
    cap_percent: f64,
    start: DateTime<Utc>,
    deadline: DateTime<Utc>,
    now: DateTime<Utc>,
) -> f64 {
    let span = (deadline - start).num_milliseconds() as f64;
    if span <= 0.0 {
        return cap_percent;
    }
    let t = (((now - start).num_milliseconds() as f64) / span).clamp(0.0, 1.0);
    t * cap_percent
}

/// Remaining allowance spread over the whole days left, so the figure never
/// divides by a fraction of a day and spike as a reset approaches.
pub fn daily_budget(
    actual: f64,
    cap_percent: f64,
    resets_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> f64 {
    let remaining = (cap_percent - actual).max(0.0);
    let secs_left = ((resets_at - now).num_milliseconds() as f64 / 1000.0).max(0.0);
    let days_left = (secs_left / 86400.0).ceil().max(1.0);
    remaining / days_left
}

pub fn budget_status(
    actual: f64,
    cap_percent: f64,
    start: DateTime<Utc>,
    deadline: DateTime<Utc>,
    now: DateTime<Utc>,
    margin: f64,
    resets_at: Option<DateTime<Utc>>,
) -> BudgetStatus {
    let target = budget_target(cap_percent, start, deadline, now);
    let delta = actual - target;
    let state = if actual >= cap_percent {
        BudgetState::Exceeded
    } else if delta > margin {
        BudgetState::Over
    } else if delta < -margin {
        BudgetState::Under
    } else {
        BudgetState::OnPace
    };
    BudgetStatus {
        state,
        target_percent: target,
        actual_percent: actual,
        cap_percent,
        daily_budget_percent: resets_at.map(|r| daily_budget(actual, cap_percent, r, now)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap()
    }

    #[test]
    fn smoothstep_clamps_and_is_symmetric_about_its_midpoint() {
        assert_eq!(smoothstep(0.0, 1.0, -1.0), 0.0);
        assert_eq!(smoothstep(0.0, 1.0, 2.0), 1.0);
        assert_eq!(smoothstep(0.0, 1.0, 0.5), 0.5);
        // Empty range degenerates to a step, as the Swift guard does.
        assert_eq!(smoothstep(1.0, 1.0, 1.0), 1.0);
        assert_eq!(smoothstep(1.0, 1.0, 0.9), 0.0);
    }

    #[test]
    fn a_fully_consumed_window_is_maximum_risk_regardless_of_time_left() {
        assert_eq!(smart_risk(100.0, Some(at(3600)), SESSION_WINDOW_SECS, 10.0, at(0)), 1.0);
        assert_eq!(smart_risk(140.0, None, SESSION_WINDOW_SECS, 10.0, at(0)), 1.0);
    }

    #[test]
    fn without_a_reset_time_risk_falls_back_to_absolute_usage_only() {
        // 50% is the bottom of the absolute ramp, so risk is zero there and
        // rises to one at 100%.
        assert_eq!(smart_risk(50.0, None, SESSION_WINDOW_SECS, 10.0, at(0)), 0.0);
        assert!((smart_risk(75.0, None, SESSION_WINDOW_SECS, 10.0, at(0)) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn burning_exactly_on_pace_is_low_risk_but_double_pace_is_not() {
        // Half the window elapsed: resets in 2.5h of a 5h window.
        let now = at(0);
        let resets = at((SESSION_WINDOW_SECS / 2.0) as i64);
        let on_pace = smart_risk(50.0, Some(resets), SESSION_WINDOW_SECS, 10.0, now);
        let double = smart_risk(95.0, Some(resets), SESSION_WINDOW_SECS, 10.0, now);
        assert!(on_pace < 0.5, "on-pace risk was {on_pace}");
        assert!(double > on_pace, "burning faster must raise risk");
    }

    #[test]
    fn risk_levels_use_the_same_cutoffs_as_edith() {
        assert_eq!(level_for_risk(0.0), UsageLevel::Green);
        assert_eq!(level_for_risk(0.49), UsageLevel::Green);
        assert_eq!(level_for_risk(0.50), UsageLevel::Orange);
        assert_eq!(level_for_risk(0.77), UsageLevel::Orange);
        assert_eq!(level_for_risk(0.78), UsageLevel::Red);
    }

    #[test]
    fn zone_hysteresis_holds_a_zone_until_the_falling_threshold() {
        // 0.52 is above FALLING_WARNING (0.50) but below RISING_WARNING (0.55),
        // so a warning stays a warning while a chill would not become one.
        assert_eq!(zone_for_risk(0.52, Some(PacingZone::Warning)), PacingZone::Warning);
        assert_eq!(zone_for_risk(0.52, Some(PacingZone::Chill)), PacingZone::OnTrack);
        assert_eq!(zone_for_risk(0.52, None), PacingZone::OnTrack);
    }

    #[test]
    fn zone_falls_all_the_way_from_hot_when_risk_collapses() {
        assert_eq!(zone_for_risk(0.10, Some(PacingZone::Hot)), PacingZone::Chill);
        assert_eq!(zone_for_risk(0.60, Some(PacingZone::Hot)), PacingZone::Warning);
        assert_eq!(zone_for_risk(0.80, Some(PacingZone::Hot)), PacingZone::Hot);
    }

    #[test]
    fn pacing_delta_is_zero_when_usage_tracks_elapsed_time() {
        // Half of a 5h window gone, half the quota used.
        let resets = at((SESSION_WINDOW_SECS / 2.0) as i64);
        let delta = pacing_delta(50.0, resets, SESSION_WINDOW_SECS, at(0));
        assert!(delta.abs() < 1e-9, "delta was {delta}");
    }

    #[test]
    fn pacing_delta_is_positive_when_ahead_of_the_burn() {
        let resets = at((SESSION_WINDOW_SECS / 2.0) as i64);
        assert!(pacing_delta(80.0, resets, SESSION_WINDOW_SECS, at(0)) > 0.0);
        assert!(pacing_delta(20.0, resets, SESSION_WINDOW_SECS, at(0)) < 0.0);
    }

    #[test]
    fn pacing_zones_widen_with_the_margin() {
        assert_eq!(pacing_zone(-20.0, 10.0), PacingZone::Chill);
        assert_eq!(pacing_zone(5.0, 10.0), PacingZone::OnTrack);
        assert_eq!(pacing_zone(15.0, 10.0), PacingZone::Warning);
        assert_eq!(pacing_zone(25.0, 10.0), PacingZone::Hot);
    }

    #[test]
    fn budget_target_ramps_linearly_and_clamps_outside_the_window() {
        let start = at(0);
        let deadline = at(1000);
        assert_eq!(budget_target(80.0, start, deadline, at(500)), 40.0);
        assert_eq!(budget_target(80.0, start, deadline, at(-100)), 0.0);
        assert_eq!(budget_target(80.0, start, deadline, at(2000)), 80.0);
        // A collapsed window cannot be divided, so the cap is the answer.
        assert_eq!(budget_target(80.0, start, start, at(0)), 80.0);
    }

    #[test]
    fn budget_state_reflects_the_margin_around_the_target() {
        let start = at(0);
        let deadline = at(1000);
        let now = at(500);
        let state = |actual: f64| {
            budget_status(actual, 80.0, start, deadline, now, 5.0, None).state
        };
        assert_eq!(state(40.0), BudgetState::OnPace);
        assert_eq!(state(48.0), BudgetState::Over);
        assert_eq!(state(30.0), BudgetState::Under);
        assert_eq!(state(85.0), BudgetState::Exceeded);
    }

    #[test]
    fn daily_budget_rounds_up_to_whole_days_so_it_does_not_spike() {
        // 36 hours left rounds to two days, so half the remainder per day.
        let remaining = daily_budget(40.0, 80.0, at(36 * 3600), at(0));
        assert!((remaining - 20.0).abs() < 1e-9, "got {remaining}");
        // Never divides by less than one day.
        let last_hour = daily_budget(40.0, 80.0, at(600), at(0));
        assert!((last_hour - 40.0).abs() < 1e-9, "got {last_hour}");
    }

    #[test]
    fn exhausted_budget_never_reports_a_negative_daily_allowance() {
        assert_eq!(daily_budget(120.0, 80.0, at(86400), at(0)), 0.0);
    }
}
