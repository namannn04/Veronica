//! Rate-limit alerts.
//!
//! A port of Edith's `LimitNotifierLogic`: the same five kinds of alert, the
//! same edge-triggering, the same wording. Every alert fires on a *transition*,
//! never on a level, so a window sitting at 90% produces one banner rather than
//! one per poll.
//!
//! The five kinds:
//!
//! - **Escalation** — the window's level rose. Wording depends on whether it is
//!   the session or the weekly window, how much time is left, and whether the
//!   rise was driven by pace rather than by the absolute figure.
//! - **Recovery** — the window fell back to green, i.e. it reset.
//! - **Pacing** — the pacing zone entered warning or hot: ahead of a linear burn
//!   without necessarily being near the cap.
//! - **Reminder** — a fixed time before a window resets.
//! - **Token expired** — the provider's credentials need a fresh login.
//!
//! Two things differ from macOS by necessity:
//!
//! 1. `UNCalendarNotificationTrigger` lets Edith hand a future notification to
//!    the OS. freedesktop notifications cannot be scheduled, so the reminder is
//!    fired by Veronica's own poll when its moment arrives, and the state
//!    records which reset instant has already been covered so a poll every
//!    thirty seconds does not fire it repeatedly.
//! 2. macOS replaces a banner by reusing its string identifier. On freedesktop
//!    the server hands back a numeric id, so the state remembers the last id per
//!    alert so a rising threshold updates one banner instead of stacking five.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Local, Utc};
use serde::{Deserialize, Serialize};

use crate::limits::{
    level_for_risk, pacing_delta, pacing_zone, smart_risk, LimitWindow, LimitWindowKind, PacingZone,
    UsageLevel, UsageThresholds, DEFAULT_CRITICAL_PERCENT, DEFAULT_WARN_PERCENT,
};

/// How long after a token-expiry alert before another may fire. Matches Edith's
/// one-hour debounce: a signed-out provider would otherwise alert every poll.
pub const TOKEN_EXPIRED_DEBOUNCE: Duration = Duration::hours(1);

/// One alert, ready to be posted. `id` is stable per kind and surface, which is
/// what lets a later alert of the same kind replace the earlier banner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitAlert {
    pub id: String,
    pub title: String,
    pub body: String,
}

impl LimitAlert {
    fn new(id: impl Into<String>, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            body: body.into(),
        }
    }
}

/// Every switch that governs alerts, with Edith's defaults.
///
/// `master` is off by default: Veronica does not start posting banners until the
/// user turns alerts on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifySettings {
    pub master: bool,
    pub track_session: bool,
    pub track_weekly: bool,
    pub recovery: bool,
    pub pacing_warning: bool,
    pub pacing_hot: bool,
    pub reminder_session: bool,
    pub reminder_session_offset_min: i64,
    pub reminder_weekly: bool,
    pub reminder_weekly_offset_min: i64,
    pub token_expired: bool,
    /// When on, the level follows the blended risk rather than the raw
    /// percentage, so time remaining counts as much as the figure.
    pub smart_color: bool,
    pub pacing_margin: f64,
    pub thresholds: UsageThresholds,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self {
            master: false,
            track_session: true,
            track_weekly: true,
            recovery: true,
            pacing_warning: true,
            pacing_hot: true,
            reminder_session: false,
            reminder_session_offset_min: 30,
            reminder_weekly: false,
            reminder_weekly_offset_min: 120,
            token_expired: true,
            smart_color: true,
            pacing_margin: 10.0,
            thresholds: UsageThresholds::default(),
        }
    }
}

impl NotifySettings {
    /// Read from Veronica's settings file, under the same keys Edith uses so a
    /// backup taken on either platform describes the same behaviour.
    pub fn from_settings(settings: &veronica_core::Settings) -> Self {
        let default = Self::default();
        // An offset below one minute has no moment that is both past the
        // reminder and before the reset, so it would be silently dead; and a
        // negative one would fire after the reset. Both fall back.
        let minutes = |key: &str, fallback: i64| {
            settings
                .get(key)
                .and_then(|value| value.as_i64())
                .filter(|value| *value >= 1)
                .unwrap_or(fallback)
        };
        Self {
            master: settings.bool_or("notifyMaster", default.master),
            track_session: settings.bool_or("notifyTrackSession", default.track_session),
            track_weekly: settings.bool_or("notifyTrackWeekly", default.track_weekly),
            recovery: settings.bool_or("notifyRecovery", default.recovery),
            pacing_warning: settings.bool_or("notifyPacingWarning", default.pacing_warning),
            pacing_hot: settings.bool_or("notifyPacingHot", default.pacing_hot),
            reminder_session: settings.bool_or("notifyReminderSession", default.reminder_session),
            reminder_session_offset_min: minutes(
                "notifyReminderSessionOffsetMin",
                default.reminder_session_offset_min,
            ),
            reminder_weekly: settings.bool_or("notifyReminderWeekly", default.reminder_weekly),
            reminder_weekly_offset_min: minutes(
                "notifyReminderWeeklyOffsetMin",
                default.reminder_weekly_offset_min,
            ),
            token_expired: settings.bool_or("notifyTokenExpired", default.token_expired),
            smart_color: settings.bool_or("smartColor", default.smart_color),
            pacing_margin: settings.f64_or("limitsPacingMargin", default.pacing_margin),
            thresholds: UsageThresholds {
                warning_percent: settings
                    .get("limitsWarnPercent")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(DEFAULT_WARN_PERCENT),
                critical_percent: settings
                    .get("limitsCritPercent")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(DEFAULT_CRITICAL_PERCENT),
            },
        }
    }
}

/// What the notifier remembers between polls, so alerts stay edge-triggered
/// across a restart rather than re-firing at every launch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotifierState {
    pub session_level: UsageLevel,
    pub weekly_level: UsageLevel,
    pub session_pacing: PacingZone,
    pub weekly_pacing: PacingZone,
    /// The reset instant a reminder has already been fired for, so the poll
    /// that stands in for a scheduled notification fires it exactly once.
    pub session_reminder_for: Option<DateTime<Utc>>,
    pub weekly_reminder_for: Option<DateTime<Utc>>,
    pub token_expired_at: Option<DateTime<Utc>>,
    /// Server-assigned banner id per alert id, so an alert replaces its own
    /// previous banner instead of stacking.
    pub banners: BTreeMap<String, u32>,
}

impl Default for NotifierState {
    fn default() -> Self {
        Self {
            session_level: UsageLevel::Green,
            weekly_level: UsageLevel::Green,
            // Edith starts both zones on-track rather than chill, so merely
            // being relaxed at launch is not reported as a change.
            session_pacing: PacingZone::OnTrack,
            weekly_pacing: PacingZone::OnTrack,
            session_reminder_for: None,
            weekly_reminder_for: None,
            token_expired_at: None,
            banners: BTreeMap::new(),
        }
    }
}

impl NotifierState {
    /// Load, treating a missing or unreadable file as a fresh state: losing the
    /// alert history is harmless, and refusing to start is not.
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&temp, path)?;
        Ok(())
    }

    /// Whether there is any tracking left to forget.
    ///
    /// The runner clears while alerts are off, and it must do so on a fresh
    /// launch as well as when the switch is flipped — Veronica may well have been
    /// restarted in between. Asking the state rather than remembering "we were
    /// enabled a moment ago" is what makes that work, and it also makes the
    /// clear idempotent, so an idle loop does not rewrite the file every tick.
    pub fn has_tracking(&self) -> bool {
        let fresh = Self::default();
        self.session_level != fresh.session_level
            || self.weekly_level != fresh.weekly_level
            || self.session_pacing != fresh.session_pacing
            || self.weekly_pacing != fresh.weekly_pacing
            || self.session_reminder_for.is_some()
            || self.weekly_reminder_for.is_some()
            || self.token_expired_at.is_some()
    }

    /// Forget every level and zone, so turning alerts back on does not
    /// immediately fire for a state the user never saw. Edith clears the same
    /// four keys when the master switch goes off.
    pub fn reset_tracking(&mut self) {
        let banners = std::mem::take(&mut self.banners);
        *self = Self {
            banners,
            ..Self::default()
        };
    }

    pub fn banner(&self, alert_id: &str) -> Option<u32> {
        self.banners.get(alert_id).copied()
    }

    /// Remember the banner the server created, so the next alert of this kind
    /// updates it in place. A returned id of zero means the server declined to
    /// give one, which is not worth remembering.
    pub fn record_banner(&mut self, alert_id: &str, notification_id: u32) {
        if notification_id == 0 {
            self.banners.remove(alert_id);
        } else {
            self.banners.insert(alert_id.to_string(), notification_id);
        }
    }
}

/// Decide what to post, mutating `state` to reflect the new levels and zones.
///
/// Ported from Edith's `decide`: the master switch short-circuits, both windows
/// are evaluated for a level change, and both pacing zones for a zone change.
pub fn decide(
    session: Option<LimitWindow>,
    week: Option<LimitWindow>,
    settings: &NotifySettings,
    state: &mut NotifierState,
    now: DateTime<Utc>,
) -> Vec<LimitAlert> {
    if !settings.master {
        return Vec::new();
    }
    let mut alerts = Vec::new();

    let session_pacing = pacing_for(session, LimitWindowKind::Session, settings.pacing_margin, now);
    let weekly_pacing = pacing_for(week, LimitWindowKind::Weekly, settings.pacing_margin, now);

    if settings.track_session {
        if let Some(window) = session {
            alerts.extend(check_surface(
                LimitWindowKind::Session,
                window,
                session_pacing,
                settings,
                &mut state.session_level,
                now,
            ));
        }
    }
    if settings.track_weekly {
        if let Some(window) = week {
            alerts.extend(check_surface(
                LimitWindowKind::Weekly,
                window,
                weekly_pacing,
                settings,
                &mut state.weekly_level,
                now,
            ));
        }
    }
    if let Some(zone) = session_pacing {
        alerts.extend(check_pacing(
            zone,
            LimitWindowKind::Session,
            settings,
            &mut state.session_pacing,
        ));
    }
    if let Some(zone) = weekly_pacing {
        alerts.extend(check_pacing(
            zone,
            LimitWindowKind::Weekly,
            settings,
            &mut state.weekly_pacing,
        ));
    }

    alerts
}

/// Reminders that are due now, marking each as fired.
///
/// Separate from `decide` because these are time-triggered rather than
/// state-triggered: on macOS the OS holds them, and here the poll does.
pub fn due_reminders(
    session: Option<LimitWindow>,
    week: Option<LimitWindow>,
    settings: &NotifySettings,
    state: &mut NotifierState,
    now: DateTime<Utc>,
) -> Vec<LimitAlert> {
    if !settings.master {
        return Vec::new();
    }
    let mut alerts = Vec::new();

    let surfaces = [
        (
            LimitWindowKind::Session,
            session,
            settings.reminder_session,
            settings.reminder_session_offset_min,
        ),
        (
            LimitWindowKind::Weekly,
            week,
            settings.reminder_weekly,
            settings.reminder_weekly_offset_min,
        ),
    ];

    for (kind, window, enabled, offset) in surfaces {
        let already = match kind {
            LimitWindowKind::Session => state.session_reminder_for,
            LimitWindowKind::Weekly => state.weekly_reminder_for,
        };
        let fired_for = reminder_due(window, enabled, offset, already, now);
        if let Some(resets_at) = fired_for {
            match kind {
                LimitWindowKind::Session => state.session_reminder_for = Some(resets_at),
                LimitWindowKind::Weekly => state.weekly_reminder_for = Some(resets_at),
            }
            alerts.push(reminder_alert(kind, offset));
        }
    }

    alerts
}

/// Whether a reminder should fire now, and for which reset.
///
/// Fires once the offset has been reached and while the reset is still ahead. A
/// reset instant already covered is skipped, which is what stops a thirty-second
/// poll from re-firing for the same window.
fn reminder_due(
    window: Option<LimitWindow>,
    enabled: bool,
    offset_minutes: i64,
    already_fired_for: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    if !enabled {
        return None;
    }
    let resets_at = window?.resets_at?;
    if already_fired_for == Some(resets_at) {
        return None;
    }
    let fire_at = resets_at - Duration::minutes(offset_minutes.max(0));
    // Between the offset and the reset itself. Past the reset there is nothing
    // to warn about, and the recovery alert covers that moment instead.
    (now >= fire_at && now < resets_at).then_some(resets_at)
}

fn reminder_alert(kind: LimitWindowKind, offset_minutes: i64) -> LimitAlert {
    let label = offset_label(offset_minutes);
    match kind {
        LimitWindowKind::Session => LimitAlert::new(
            "reminder_session",
            format!("Session resets in {label}"),
            "Save your spot or send it",
        ),
        LimitWindowKind::Weekly => LimitAlert::new(
            "reminder_weekly",
            format!("Weekly resets in {label}"),
            "Last lap on the cycle",
        ),
    }
}

/// The token-expiry alert, debounced to once an hour.
pub fn token_expired(
    settings: &NotifySettings,
    state: &mut NotifierState,
    now: DateTime<Utc>,
) -> Option<LimitAlert> {
    if !settings.master || !settings.token_expired {
        return None;
    }
    if let Some(last) = state.token_expired_at {
        if now - last < TOKEN_EXPIRED_DEBOUNCE {
            return None;
        }
    }
    state.token_expired_at = Some(now);
    Some(LimitAlert::new(
        "token_expired",
        "Claude token expired",
        "Run claude to log in again",
    ))
}

fn pacing_for(
    window: Option<LimitWindow>,
    kind: LimitWindowKind,
    margin: f64,
    now: DateTime<Utc>,
) -> Option<PacingZone> {
    let window = window?;
    let resets_at = window.resets_at?;
    let delta = pacing_delta(window.percent, resets_at, kind.duration_secs(), now);
    Some(pacing_zone(delta, margin))
}

fn check_surface(
    kind: LimitWindowKind,
    window: LimitWindow,
    pacing: Option<PacingZone>,
    settings: &NotifySettings,
    previous: &mut UsageLevel,
    now: DateTime<Utc>,
) -> Vec<LimitAlert> {
    let absolute = UsageLevel::from_percent(window.percent, settings.thresholds);
    let current = if settings.smart_color {
        level_for_risk(smart_risk(
            window.percent,
            window.resets_at,
            kind.duration_secs(),
            settings.pacing_margin,
            now,
        ))
    } else {
        absolute
    };
    if current == *previous {
        return Vec::new();
    }
    let prior = *previous;
    *previous = current;

    // A weekly window can rise on pace alone while the figure is still low.
    // Saying "almost capped" there would be wrong, so it gets its own wording.
    let pace_driven =
        settings.smart_color && current > absolute && kind == LimitWindowKind::Weekly;

    if current > prior {
        return vec![escalation(kind, current, window, pacing, pace_driven, now)];
    }
    if current == UsageLevel::Green && prior > UsageLevel::Green && settings.recovery {
        return vec![recovery(kind, window, now)];
    }
    Vec::new()
}

fn check_pacing(
    zone: PacingZone,
    surface: LimitWindowKind,
    settings: &NotifySettings,
    last: &mut PacingZone,
) -> Vec<LimitAlert> {
    if zone == *last {
        return Vec::new();
    }
    *last = zone;
    let prefix = match surface {
        LimitWindowKind::Session => "Session",
        LimitWindowKind::Weekly => "Weekly",
    };
    let slug = match surface {
        LimitWindowKind::Session => "session",
        LimitWindowKind::Weekly => "weekly",
    };
    match zone {
        PacingZone::Hot if settings.pacing_hot => vec![LimitAlert::new(
            format!("pacing_{slug}_hot"),
            format!("{prefix}: burning hot"),
            "Way ahead of pace, pump the brakes",
        )],
        PacingZone::Warning if settings.pacing_warning => vec![LimitAlert::new(
            format!("pacing_{slug}_warning"),
            format!("{prefix}: drifting fast"),
            "A touch faster than ideal, keep an eye",
        )],
        _ => Vec::new(),
    }
}

fn escalation(
    kind: LimitWindowKind,
    level: UsageLevel,
    window: LimitWindow,
    pacing: Option<PacingZone>,
    pace_driven: bool,
    now: DateTime<Utc>,
) -> LimitAlert {
    let id = match kind {
        LimitWindowKind::Session => "escalation_session",
        LimitWindowKind::Weekly => "escalation_weekly",
    };
    if pace_driven {
        return LimitAlert::new(
            id,
            "Ahead of weekly pace",
            "You're ahead of an even weekly burn rate, not near the cap. Fine if intentional.",
        );
    }
    match kind {
        LimitWindowKind::Session => {
            let left = window
                .resets_at
                .filter(|resets| *resets > now)
                .map(|resets| countdown(now, resets));
            if level == UsageLevel::Red {
                return LimitAlert::new(
                    id,
                    "5h almost capped",
                    left.map(|left| format!("Easy until reset, {left} left"))
                        .unwrap_or_else(|| "Limit almost reached".to_string()),
                );
            }
            let zone = pacing.unwrap_or(PacingZone::OnTrack);
            let title = match zone {
                PacingZone::Chill => "Pace check",
                PacingZone::OnTrack => "Session getting heavy",
                PacingZone::Warning => "Drifting on the 5h",
                PacingZone::Hot => "Burning the 5h",
            };
            let body = match left {
                Some(left) => match zone {
                    PacingZone::Chill => format!("Pace is fine, resets in {left}"),
                    PacingZone::OnTrack => format!("On track, resets in {left}"),
                    PacingZone::Warning => format!("A touch fast, {left} left"),
                    PacingZone::Hot => format!("Way ahead of pace, {left} left"),
                },
                None => "Past the warning level".to_string(),
            };
            LimitAlert::new(id, title, body)
        }
        LimitWindowKind::Weekly => {
            let when = window
                .resets_at
                .filter(|resets| *resets > now)
                .map(date_time);
            if level == UsageLevel::Red {
                return LimitAlert::new(
                    id,
                    "Weekly almost capped",
                    when.map(|when| format!("Take it slow until {when}"))
                        .unwrap_or_else(|| "Weekly limit almost reached".to_string()),
                );
            }
            LimitAlert::new(
                id,
                "Weekly filling up",
                when.map(|when| format!("Resets {when}"))
                    .unwrap_or_else(|| "Past the weekly warning".to_string()),
            )
        }
    }
}

fn recovery(kind: LimitWindowKind, window: LimitWindow, now: DateTime<Utc>) -> LimitAlert {
    match kind {
        LimitWindowKind::Session => {
            let at = window.resets_at.filter(|r| *r > now).map(time);
            LimitAlert::new(
                "recovery_session",
                "5h cleared",
                at.map(|at| format!("Fresh slate at {at}"))
                    .unwrap_or_else(|| "Fresh slate, you're back".to_string()),
            )
        }
        LimitWindowKind::Weekly => {
            let at = window.resets_at.filter(|r| *r > now).map(date_time);
            LimitAlert::new(
                "recovery_weekly",
                "Weekly reset",
                at.map(|at| format!("New cycle, you're back at {at}"))
                    .unwrap_or_else(|| "New cycle, you're back".to_string()),
            )
        }
    }
}

/// "45 min", "2 h 14 min", "1 d 3 h" — Edith's thresholds exactly.
pub fn countdown(from: DateTime<Utc>, to: DateTime<Utc>) -> String {
    let minutes = ((to - from).num_seconds() / 60).max(0);
    let hours = minutes / 60;
    let rest = minutes % 60;
    if hours >= 24 {
        return format!("{} d {} h", hours / 24, hours % 24);
    }
    if hours > 0 {
        return if rest > 0 {
            format!("{hours} h {rest} min")
        } else {
            format!("{hours} h")
        };
    }
    format!("{minutes} min")
}

/// A reset moment in local time, e.g. "Thu, Aug 28, 3:00 PM". Local rather than
/// UTC because it is read by a person deciding whether to keep working.
pub fn date_time(value: DateTime<Utc>) -> String {
    value
        .with_timezone(&Local)
        .format("%a, %b %-d, %-I:%M %p")
        .to_string()
}

pub fn time(value: DateTime<Utc>) -> String {
    value.with_timezone(&Local).format("%-I:%M %p").to_string()
}

/// "30 min", "2 h" — whole hours read as hours, anything else as minutes.
pub fn offset_label(minutes: i64) -> String {
    if minutes >= 60 && minutes % 60 == 0 {
        format!("{} h", minutes / 60)
    } else {
        format!("{minutes} min")
    }
}

/// When a reminder for this reset would fire, or `None` when that is already
/// past. Kept public because the settings screen previews it.
pub fn reminder_fire_at(
    resets_at: Option<DateTime<Utc>>,
    offset_minutes: i64,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let fire = resets_at? - Duration::minutes(offset_minutes.max(0));
    (fire > now).then_some(fire)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_787_000_000 + secs, 0).unwrap()
    }

    fn window(percent: f64, resets_in_secs: i64) -> LimitWindow {
        LimitWindow {
            percent,
            resets_at: Some(at(resets_in_secs)),
        }
    }

    fn on() -> NotifySettings {
        NotifySettings {
            master: true,
            ..NotifySettings::default()
        }
    }

    #[test]
    fn nothing_fires_while_the_master_switch_is_off() {
        let mut state = NotifierState::default();
        let settings = NotifySettings::default();
        assert!(settings.master == false, "alerts must be opt-in");
        let alerts = decide(
            Some(window(99.0, 60)),
            Some(window(99.0, 60)),
            &settings,
            &mut state,
            at(0),
        );
        assert!(alerts.is_empty());
        assert_eq!(state, NotifierState::default(), "state must not move either");
    }

    #[test]
    fn a_rising_level_fires_once_and_not_again_at_the_same_level() {
        let mut state = NotifierState::default();
        let heavy = window(87.0, 7988);

        let first = decide(Some(heavy), None, &on(), &mut state, at(0));
        // A window this far ahead crosses a level *and* a pacing zone, so both
        // fire — one about the cap, one about the rate.
        let ids: Vec<&str> = first.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["escalation_session", "pacing_session_hot"]);

        // Same window a moment later: the level has not changed, so silence.
        let second = decide(Some(heavy), None, &on(), &mut state, at(1));
        assert!(
            second.is_empty(),
            "an unchanged level must not re-alert, got {second:?}"
        );
    }

    #[test]
    fn the_session_escalation_names_the_time_left_and_the_pace() {
        let mut state = NotifierState::default();
        // 87% with 2h13m left of five hours: well ahead of pace and past red.
        let alerts = decide(Some(window(87.0, 7988)), None, &on(), &mut state, at(0));
        assert_eq!(alerts[0].title, "5h almost capped");
        assert_eq!(alerts[0].body, "Easy until reset, 2 h 13 min left");
        assert_eq!(state.session_level, UsageLevel::Red);
    }

    #[test]
    fn falling_back_to_green_reports_a_recovery() {
        let mut state = NotifierState::default();
        decide(Some(window(90.0, 600)), None, &on(), &mut state, at(0));
        assert!(
            state.session_level > UsageLevel::Green,
            "90% with ten minutes left should not be green"
        );

        // The window reset: a fresh five hours, barely used.
        let alerts = decide(Some(window(2.0, 17_900)), None, &on(), &mut state, at(700));
        let recovery = alerts
            .iter()
            .find(|alert| alert.id == "recovery_session")
            .unwrap_or_else(|| panic!("no recovery in {alerts:?}"));
        assert_eq!(recovery.title, "5h cleared");
        assert_eq!(state.session_level, UsageLevel::Green);
    }

    #[test]
    fn recovery_can_be_switched_off_while_escalation_stays_on() {
        let settings = NotifySettings {
            recovery: false,
            ..on()
        };
        let mut state = NotifierState::default();
        let rising = decide(Some(window(90.0, 600)), None, &settings, &mut state, at(0));
        assert!(rising.iter().any(|a| a.id == "escalation_session"), "got {rising:?}");

        let quiet = decide(Some(window(2.0, 17_900)), None, &settings, &mut state, at(700));
        assert!(
            !quiet.iter().any(|alert| alert.id.starts_with("recovery_")),
            "recovery is switched off, got {quiet:?}"
        );
        // The level still moves, so the next rise is still an edge.
        assert_eq!(state.session_level, UsageLevel::Green);
    }

    #[test]
    fn a_weekly_window_ahead_of_pace_says_so_rather_than_claiming_a_cap() {
        // 40% of the week used a fifth of the way in: way ahead of an even burn,
        // nowhere near the cap. Saying "almost capped" here would be wrong.
        let mut state = NotifierState::default();
        let week = window(40.0, (WEEK - WEEK / 5) as i64);
        let alerts = decide(None, Some(week), &on(), &mut state, at(0));
        let escalation = alerts
            .iter()
            .find(|alert| alert.id == "escalation_weekly")
            .unwrap_or_else(|| panic!("no weekly escalation in {alerts:?}"));
        assert_eq!(escalation.title, "Ahead of weekly pace");
        assert!(escalation.body.contains("not near the cap"));
    }

    const WEEK: i64 = 7 * 24 * 3600;

    #[test]
    fn pacing_alerts_fire_on_entering_a_zone_and_respect_their_switches() {
        // A session at 70% with four hours still to go: ahead of pace.
        let mut state = NotifierState::default();
        let alerts = decide(Some(window(70.0, 14_400)), None, &on(), &mut state, at(0));
        let pacing: Vec<&LimitAlert> = alerts.iter().filter(|a| a.id.starts_with("pacing_")).collect();
        assert_eq!(pacing.len(), 1, "got {alerts:?}");
        assert_eq!(pacing[0].id, "pacing_session_hot");
        assert_eq!(state.session_pacing, PacingZone::Hot);

        // With the hot switch off, the zone still updates but nothing is posted.
        let mut quiet_state = NotifierState::default();
        let settings = NotifySettings {
            pacing_hot: false,
            ..on()
        };
        let quiet = decide(
            Some(window(70.0, 14_400)),
            None,
            &settings,
            &mut quiet_state,
            at(0),
        );
        assert!(!quiet.iter().any(|a| a.id.starts_with("pacing_")), "got {quiet:?}");
        assert_eq!(quiet_state.session_pacing, PacingZone::Hot);
    }

    #[test]
    fn tracking_a_surface_can_be_switched_off_independently() {
        let settings = NotifySettings {
            track_session: false,
            track_weekly: true,
            // Isolate the level alerts from the pacing ones.
            pacing_hot: false,
            pacing_warning: false,
            ..on()
        };
        let mut state = NotifierState::default();
        let alerts = decide(
            Some(window(95.0, 600)),
            Some(window(95.0, 600)),
            &settings,
            &mut state,
            at(0),
        );
        assert_eq!(alerts.len(), 1, "only the weekly surface, got {alerts:?}");
        assert_eq!(alerts[0].id, "escalation_weekly");
        assert_eq!(
            state.session_level,
            UsageLevel::Green,
            "an untracked surface's level must not be advanced, or turning it on \
             later would miss the first edge"
        );
    }

    #[test]
    fn with_smart_colour_off_the_level_follows_the_raw_percentage() {
        // 70% with the whole window left: risk-blended this is not yet red, but
        // the raw thresholds put 70% above the 60% warning line.
        let settings = NotifySettings {
            smart_color: false,
            pacing_hot: false,
            pacing_warning: false,
            ..on()
        };
        let mut state = NotifierState::default();
        let alerts = decide(
            Some(window(70.0, 17_999)),
            None,
            &settings,
            &mut state,
            at(0),
        );
        assert_eq!(state.session_level, UsageLevel::Orange, "got {alerts:?}");
        assert_eq!(alerts.len(), 1);
    }

    #[test]
    fn a_window_with_no_reset_time_still_escalates_with_a_plainer_body() {
        let mut state = NotifierState::default();
        let unknown = LimitWindow {
            percent: 99.0,
            resets_at: None,
        };
        let alerts = decide(Some(unknown), None, &on(), &mut state, at(0));
        assert_eq!(alerts.len(), 1, "no reset time means no pacing zone, so only \
                                     the level alert fires: {alerts:?}");
        assert_eq!(alerts[0].id, "escalation_session");
        assert_eq!(alerts[0].body, "Limit almost reached");
    }

    #[test]
    fn state_survives_a_round_trip_so_a_restart_does_not_re_alert() {
        let dir = std::env::temp_dir().join(format!("veronica-alerts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("alerts.json");

        let mut state = NotifierState::default();
        decide(Some(window(90.0, 600)), None, &on(), &mut state, at(0));
        state.record_banner("escalation_session", 42);
        state.save(&path).unwrap();

        let reloaded = NotifierState::load(&path);
        assert_eq!(reloaded, state);
        assert_eq!(reloaded.banner("escalation_session"), Some(42));

        // The reloaded state is at red, so the same window is silent.
        let mut restarted = reloaded;
        let quiet = decide(Some(window(90.0, 600)), None, &on(), &mut restarted, at(5));
        assert!(quiet.is_empty(), "a restart must not re-alert, got {quiet:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_or_corrupt_state_file_starts_clean() {
        let dir = std::env::temp_dir().join(format!("veronica-alerts-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            NotifierState::load(&dir.join("absent.json")),
            NotifierState::default()
        );
        let corrupt = dir.join("corrupt.json");
        std::fs::write(&corrupt, b"not json at all").unwrap();
        assert_eq!(NotifierState::load(&corrupt), NotifierState::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_fresh_state_has_nothing_to_clear_but_a_used_one_does() {
        // The runner clears while alerts are off, including on a fresh launch,
        // so it asks the state rather than remembering whether the switch was
        // flipped during *this* run.
        let mut state = NotifierState::default();
        assert!(!state.has_tracking(), "nothing has happened yet");

        decide(Some(window(90.0, 600)), None, &on(), &mut state, at(0));
        assert!(state.has_tracking());

        state.reset_tracking();
        assert!(!state.has_tracking(), "clearing must be idempotent");
    }

    #[test]
    fn a_recorded_banner_alone_is_not_tracking_to_clear() {
        // Banner ids survive a clear, so counting them as tracking would make
        // the runner rewrite the file on every idle tick forever.
        let mut state = NotifierState::default();
        state.record_banner("escalation_session", 7);
        assert!(!state.has_tracking());
    }

    #[test]
    fn every_kind_of_tracking_counts() {
        for mutate in [
            (|s: &mut NotifierState| s.session_level = UsageLevel::Red)
                as fn(&mut NotifierState),
            |s| s.weekly_level = UsageLevel::Orange,
            |s| s.session_pacing = PacingZone::Hot,
            |s| s.weekly_pacing = PacingZone::Chill,
            |s| s.session_reminder_for = Some(at(0)),
            |s| s.weekly_reminder_for = Some(at(0)),
            |s| s.token_expired_at = Some(at(0)),
        ] {
            let mut state = NotifierState::default();
            mutate(&mut state);
            assert!(state.has_tracking(), "a changed field was not noticed");
        }
    }

    #[test]
    fn clearing_tracking_keeps_the_banner_ids_so_old_banners_are_still_replaceable() {
        let mut state = NotifierState::default();
        state.session_level = UsageLevel::Red;
        state.session_reminder_for = Some(at(0));
        state.record_banner("escalation_session", 7);
        state.reset_tracking();
        assert_eq!(state.session_level, UsageLevel::Green);
        assert_eq!(state.session_reminder_for, None);
        assert_eq!(state.banner("escalation_session"), Some(7));
    }

    #[test]
    fn a_banner_id_of_zero_is_not_remembered() {
        // The spec allows a server to return 0; storing it would ask the next
        // alert to replace notification zero, which is not a notification.
        let mut state = NotifierState::default();
        state.record_banner("escalation_session", 5);
        state.record_banner("escalation_session", 0);
        assert_eq!(state.banner("escalation_session"), None);
    }

    // -- reminders ----------------------------------------------------------

    #[test]
    fn a_reminder_fires_once_inside_its_offset_and_not_before() {
        let settings = NotifySettings {
            reminder_session: true,
            reminder_session_offset_min: 30,
            ..on()
        };
        // Reset in 45 minutes: outside a 30-minute offset, so nothing yet.
        let mut state = NotifierState::default();
        let early = due_reminders(Some(window(50.0, 45 * 60)), None, &settings, &mut state, at(0));
        assert!(early.is_empty(), "got {early:?}");
        assert_eq!(state.session_reminder_for, None);

        // Reset in 20 minutes: inside the offset.
        let due = due_reminders(Some(window(50.0, 20 * 60)), None, &settings, &mut state, at(0));
        assert_eq!(due.len(), 1, "got {due:?}");
        assert_eq!(due[0].id, "reminder_session");
        assert_eq!(due[0].title, "Session resets in 30 min");
        assert_eq!(state.session_reminder_for, Some(at(20 * 60)));

        // Polled again for the same reset: already covered.
        let again = due_reminders(Some(window(50.0, 20 * 60)), None, &settings, &mut state, at(30));
        assert!(again.is_empty(), "a poll must not re-fire, got {again:?}");
    }

    #[test]
    fn the_next_window_gets_its_own_reminder() {
        let settings = NotifySettings {
            reminder_session: true,
            reminder_session_offset_min: 30,
            ..on()
        };
        let mut state = NotifierState::default();
        due_reminders(Some(window(50.0, 20 * 60)), None, &settings, &mut state, at(0));
        // A different reset instant is a different window.
        let next = due_reminders(
            Some(window(50.0, 5 * 3600 + 20 * 60)),
            None,
            &settings,
            &mut state,
            at(5 * 3600),
        );
        assert_eq!(next.len(), 1, "got {next:?}");
    }

    #[test]
    fn a_reminder_does_not_fire_after_the_reset_has_passed() {
        let settings = NotifySettings {
            reminder_weekly: true,
            reminder_weekly_offset_min: 120,
            ..on()
        };
        let mut state = NotifierState::default();
        let past = due_reminders(
            None,
            Some(LimitWindow {
                percent: 80.0,
                resets_at: Some(at(-60)),
            }),
            &settings,
            &mut state,
            at(0),
        );
        assert!(past.is_empty(), "got {past:?}");
    }

    #[test]
    fn reminders_are_off_by_default() {
        let mut state = NotifierState::default();
        let alerts = due_reminders(
            Some(window(50.0, 60)),
            Some(window(50.0, 60)),
            &on(),
            &mut state,
            at(0),
        );
        assert!(alerts.is_empty(), "got {alerts:?}");
    }

    #[test]
    fn the_weekly_reminder_uses_hours_in_its_wording() {
        let settings = NotifySettings {
            reminder_weekly: true,
            reminder_weekly_offset_min: 120,
            ..on()
        };
        let mut state = NotifierState::default();
        let due = due_reminders(None, Some(window(80.0, 3600)), &settings, &mut state, at(0));
        assert_eq!(due[0].title, "Weekly resets in 2 h");
        assert_eq!(due[0].body, "Last lap on the cycle");
    }

    /// The alerts pane reads these names directly, and a nested struct does not
    /// inherit its parent's rename, so the shape is pinned rather than assumed.
    #[test]
    fn the_settings_reach_the_interface_as_camel_case_throughout() {
        let json = serde_json::to_value(NotifySettings::default()).unwrap();
        for key in [
            "master",
            "trackSession",
            "trackWeekly",
            "recovery",
            "pacingWarning",
            "pacingHot",
            "reminderSession",
            "reminderSessionOffsetMin",
            "reminderWeekly",
            "reminderWeeklyOffsetMin",
            "tokenExpired",
            "smartColor",
            "pacingMargin",
            "thresholds",
        ] {
            assert!(json.get(key).is_some(), "missing {key} in {json}");
        }
        // The nested struct is the one that silently keeps snake_case.
        let thresholds = &json["thresholds"];
        assert_eq!(thresholds["warningPercent"], DEFAULT_WARN_PERCENT);
        assert_eq!(thresholds["criticalPercent"], DEFAULT_CRITICAL_PERCENT);
        assert!(
            thresholds.get("warning_percent").is_none(),
            "snake_case leaked into the wire shape: {thresholds}"
        );

        // And it still reads back, so a stored document round trips.
        let back: NotifySettings = serde_json::from_value(json).unwrap();
        assert_eq!(back, NotifySettings::default());
    }

    #[test]
    fn a_negative_or_zero_offset_falls_back_instead_of_going_silent() {
        // Edith computes `reset - offset` unguarded, so a negative offset there
        // lands *after* the reset. Zero is no better: there is no instant both
        // at-or-past the reminder and before the reset, so it would never fire.
        for nonsense in [-30, 0] {
            let mut stored = veronica_core::Settings::default();
            stored.set("notifyReminderSessionOffsetMin", serde_json::json!(nonsense));
            stored.set("notifyReminderWeeklyOffsetMin", serde_json::json!(nonsense));
            let read = NotifySettings::from_settings(&stored);
            assert_eq!(read.reminder_session_offset_min, 30, "offset {nonsense}");
            assert_eq!(read.reminder_weekly_offset_min, 120, "offset {nonsense}");
        }
    }

    // -- token expiry -------------------------------------------------------

    #[test]
    fn token_expiry_is_debounced_to_once_an_hour() {
        let mut state = NotifierState::default();
        assert!(token_expired(&on(), &mut state, at(0)).is_some());
        assert!(
            token_expired(&on(), &mut state, at(1800)).is_none(),
            "half an hour later is still inside the debounce"
        );
        assert!(token_expired(&on(), &mut state, at(3601)).is_some());
    }

    #[test]
    fn token_expiry_respects_both_its_own_switch_and_the_master() {
        let mut state = NotifierState::default();
        let off = NotifySettings {
            token_expired: false,
            ..on()
        };
        assert!(token_expired(&off, &mut state, at(0)).is_none());
        assert!(token_expired(&NotifySettings::default(), &mut state, at(0)).is_none());
        assert_eq!(state.token_expired_at, None, "a suppressed alert must not debounce");
    }

    // -- settings and formatting -------------------------------------------

    #[test]
    fn settings_read_ediths_keys_and_fall_back_to_ediths_defaults() {
        let mut stored = veronica_core::Settings::default();
        assert_eq!(
            NotifySettings::from_settings(&stored),
            NotifySettings::default()
        );

        stored.set("notifyMaster", serde_json::json!(true));
        stored.set("notifyTrackWeekly", serde_json::json!(false));
        stored.set("notifyReminderSessionOffsetMin", serde_json::json!(45));
        stored.set("limitsWarnPercent", serde_json::json!(50));
        stored.set("limitsPacingMargin", serde_json::json!(15.0));
        let read = NotifySettings::from_settings(&stored);
        assert!(read.master);
        assert!(!read.track_weekly);
        assert!(read.track_session, "an unset key keeps its default");
        assert_eq!(read.reminder_session_offset_min, 45);
        assert_eq!(read.thresholds.warning_percent, 50);
        assert_eq!(read.thresholds.critical_percent, DEFAULT_CRITICAL_PERCENT);
        assert_eq!(read.pacing_margin, 15.0);
    }

    #[test]
    fn the_countdown_matches_ediths_thresholds() {
        assert_eq!(countdown(at(0), at(45 * 60)), "45 min");
        assert_eq!(countdown(at(0), at(2 * 3600)), "2 h");
        assert_eq!(countdown(at(0), at(2 * 3600 + 14 * 60)), "2 h 14 min");
        assert_eq!(countdown(at(0), at(27 * 3600)), "1 d 3 h");
        // A reset already past reads as zero, not a negative.
        assert_eq!(countdown(at(0), at(-600)), "0 min");
    }

    #[test]
    fn the_offset_label_prefers_whole_hours() {
        assert_eq!(offset_label(30), "30 min");
        assert_eq!(offset_label(60), "1 h");
        assert_eq!(offset_label(120), "2 h");
        assert_eq!(offset_label(90), "90 min");
    }

    #[test]
    fn the_reminder_preview_is_none_once_its_moment_is_past() {
        assert_eq!(reminder_fire_at(None, 30, at(0)), None);
        assert_eq!(
            reminder_fire_at(Some(at(3600)), 30, at(0)),
            Some(at(3600 - 1800))
        );
        // The offset is bigger than the time left, so the moment has gone.
        assert_eq!(reminder_fire_at(Some(at(600)), 30, at(0)), None);
    }
}
