//! Rate-limit gauges.
//!
//! One place that turns raw provider windows into everything a ring needs:
//! percentage, countdown, risk, level and pacing zone. The CLI, the application
//! and the shell extension all read this, so a ring can never disagree with the
//! same figure shown elsewhere.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::limits::{
    level_for_risk, pacing_delta, smart_risk, zone_for_risk, LimitWindow, PacingZone, UsageLevel,
    SESSION_WINDOW_SECS, WEEKLY_WINDOW_SECS,
};

/// How far ahead of a linear burn counts as merely on-pace, in percentage
/// points. Matches the default Edith uses for its rings.
pub const DEFAULT_PACING_MARGIN: f64 = 10.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gauge {
    /// "Claude" or "Codex".
    pub provider: String,
    /// "Session", "Week", or a model-scoped label.
    pub window: String,
    pub percent: f64,
    /// Seconds until the window resets, when the provider says.
    pub resets_in_secs: Option<i64>,
    /// 0-1, blending absolute use, projected overrun and pace.
    pub risk: f64,
    pub level: UsageLevel,
    pub zone: PacingZone,
    /// Percentage points ahead of a linear burn; negative means behind.
    pub pace_delta: Option<f64>,
}

/// What a limits read produced, including why anything is missing.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GaugeReport {
    pub gauges: Vec<Gauge>,
    /// Human-readable reasons a provider contributed nothing.
    pub notes: Vec<String>,
}

impl GaugeReport {
    /// The gauge most worth showing in a confined space such as a panel.
    ///
    /// Highest risk wins rather than highest percentage: a window at 60% with
    /// minutes left matters more than one at 80% that resets in a week.
    pub fn most_pressing(&self) -> Option<&Gauge> {
        self.gauges.iter().max_by(|a, b| {
            a.risk
                .partial_cmp(&b.risk)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

fn gauge(
    provider: &str,
    window_name: &str,
    window: LimitWindow,
    duration_secs: f64,
    margin: f64,
    now: DateTime<Utc>,
) -> Gauge {
    let risk = smart_risk(window.percent, window.resets_at, duration_secs, margin, now);
    Gauge {
        provider: provider.to_string(),
        window: window_name.to_string(),
        percent: window.percent,
        resets_in_secs: window
            .resets_at
            .map(|resets| (resets - now).num_seconds().max(0)),
        risk,
        level: level_for_risk(risk),
        zone: zone_for_risk(risk, None),
        pace_delta: window
            .resets_at
            .map(|resets| pacing_delta(window.percent, resets, duration_secs, now)),
    }
}

/// Read both providers and build their gauges.
///
/// Never fails as a whole: a provider that cannot be read contributes a note
/// instead, so one signed-out account does not hide the other's figures.
pub async fn collect(now: DateTime<Utc>, margin: f64) -> GaugeReport {
    let mut report = GaugeReport::default();

    match crate::claude::limits_for_user(now).await {
        Ok(Some(limits)) => {
            if let Some(window) = limits.session {
                report
                    .gauges
                    .push(gauge("Claude", "Session", window, SESSION_WINDOW_SECS, margin, now));
            }
            if let Some(window) = limits.week {
                report
                    .gauges
                    .push(gauge("Claude", "Week", window, WEEKLY_WINDOW_SECS, margin, now));
            }
            for scoped in limits.scoped {
                report.gauges.push(gauge(
                    "Claude",
                    &scoped.label,
                    scoped.window,
                    WEEKLY_WINDOW_SECS,
                    margin,
                    now,
                ));
            }
        }
        Ok(None) => report
            .notes
            .push("Claude: not signed in on this computer".to_string()),
        Err(error) => report.notes.push(format!("Claude: {error:#}")),
    }

    match crate::codex::fetch_limits().await {
        Ok(limits) => {
            if let Some(window) = limits.session {
                report
                    .gauges
                    .push(gauge("Codex", "Session", window, SESSION_WINDOW_SECS, margin, now));
            }
            if let Some(window) = limits.week {
                report
                    .gauges
                    .push(gauge("Codex", "Week", window, WEEKLY_WINDOW_SECS, margin, now));
            }
        }
        Err(error) => report.notes.push(format!("Codex: {error:#}")),
    }

    report
}

/// Convenience wrapper with the default margin.
pub async fn collect_now() -> Result<GaugeReport> {
    Ok(collect(Utc::now(), DEFAULT_PACING_MARGIN).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_787_000_000 + secs, 0).unwrap()
    }

    #[test]
    fn a_window_ahead_of_pace_reads_as_hot_and_high_risk() {
        // 87% used with 2h13m left of a five-hour window: the live reading that
        // prompted this feature.
        let window = LimitWindow {
            percent: 87.0,
            resets_at: Some(at(7988)),
        };
        let g = gauge("Claude", "Session", window, SESSION_WINDOW_SECS, 10.0, at(0));
        assert_eq!(g.percent, 87.0);
        assert_eq!(g.resets_in_secs, Some(7988));
        assert_eq!(g.level, UsageLevel::Red);
        assert_eq!(g.zone, PacingZone::Hot);
        assert!(g.risk > 0.9, "risk was {}", g.risk);
        assert!(g.pace_delta.unwrap() > 0.0, "should be ahead of a linear burn");
    }

    #[test]
    fn a_window_behind_pace_reads_as_chill() {
        // 45% used with minutes left: nearly reset, so nothing to worry about.
        let window = LimitWindow {
            percent: 45.0,
            resets_at: Some(at(788)),
        };
        let g = gauge("Claude", "Week", window, WEEKLY_WINDOW_SECS, 10.0, at(0));
        assert_eq!(g.level, UsageLevel::Green);
        assert_eq!(g.zone, PacingZone::Chill);
        assert!(g.pace_delta.unwrap() < 0.0);
    }

    #[test]
    fn a_window_with_no_reset_time_still_produces_a_gauge() {
        let window = LimitWindow { percent: 70.0, resets_at: None };
        let g = gauge("Claude", "Session", window, SESSION_WINDOW_SECS, 10.0, at(0));
        assert_eq!(g.resets_in_secs, None);
        assert_eq!(g.pace_delta, None);
        // Falls back to absolute use, which at 70% is partway up the ramp.
        assert!(g.risk > 0.0 && g.risk < 1.0);
    }

    #[test]
    fn a_reset_already_past_reports_zero_rather_than_negative() {
        let window = LimitWindow {
            percent: 50.0,
            resets_at: Some(at(-600)),
        };
        let g = gauge("Claude", "Session", window, SESSION_WINDOW_SECS, 10.0, at(0));
        assert_eq!(g.resets_in_secs, Some(0));
    }

    #[test]
    fn the_most_pressing_gauge_is_by_risk_not_percentage() {
        // The 60% window is nearly out of time; the 80% one has a week left.
        let report = GaugeReport {
            gauges: vec![
                gauge(
                    "Claude",
                    "Week",
                    LimitWindow { percent: 80.0, resets_at: Some(at(6 * 86_400)) },
                    WEEKLY_WINDOW_SECS,
                    10.0,
                    at(0),
                ),
                gauge(
                    "Claude",
                    "Session",
                    LimitWindow { percent: 60.0, resets_at: Some(at(300)) },
                    SESSION_WINDOW_SECS,
                    10.0,
                    at(0),
                ),
            ],
            notes: Vec::new(),
        };
        let pressing = report.most_pressing().expect("one of them");
        assert_eq!(
            pressing.window, "Week",
            "risk should pick the window projected to run out, got {pressing:?}"
        );
    }

    #[test]
    fn an_empty_report_has_nothing_pressing() {
        assert!(GaugeReport::default().most_pressing().is_none());
    }
}
