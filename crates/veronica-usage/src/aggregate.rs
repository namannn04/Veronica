//! Dashboard aggregations.
//!
//! `usage.json` is stored per day; every chart Edith draws is a different
//! rollup of that. Each function here takes the document plus the user's source
//! selection so a deselected collector disappears from every figure at once,
//! which is what makes the source filter feel consistent across the dashboard.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::models::{ChatEntry, ModelRow, ProjectEntry, Totals, UsageDocument};

/// Which sources to include. `All` follows the collector's `defaultSources`,
/// so a machine that has never been deselected shows everything it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceSelection {
    All,
    Only(Vec<String>),
}

impl SourceSelection {
    pub fn includes(&self, source: &str, document: &UsageDocument) -> bool {
        match self {
            SourceSelection::All => {
                document.default_sources.is_empty()
                    || document.default_sources.iter().any(|s| s == source)
            }
            SourceSelection::Only(list) => list.iter().any(|s| s == source),
        }
    }
}

/// Inclusive day range, as `YYYY-MM-DD`. String comparison is correct for this
/// format, so no date parsing is needed to filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DayRange {
    pub start: Option<String>,
    pub end: Option<String>,
}

impl DayRange {
    pub fn contains(&self, period: &str) -> bool {
        self.start.as_deref().is_none_or(|s| period >= s)
            && self.end.as_deref().is_none_or(|e| period <= e)
    }

    /// The last `days` calendar days present in the document, which avoids
    /// showing an empty chart when the machine was idle for a while.
    pub fn last_days(document: &UsageDocument, days: usize) -> Self {
        let mut periods: Vec<&str> = document.daily.iter().map(|d| d.period.as_str()).collect();
        periods.sort_unstable();
        periods.dedup();
        let start = periods
            .len()
            .checked_sub(days)
            .and_then(|i| periods.get(i))
            .or(periods.first())
            .map(|s| s.to_string());
        Self {
            start,
            end: periods.last().map(|s| s.to_string()),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayPoint {
    pub period: String,
    pub cost: f64,
    pub tokens: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedAmount {
    pub name: String,
    /// Present for sources, where the id and the label differ.
    pub label: String,
    pub cost: f64,
    pub tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HourPoint {
    pub hour: u8,
    pub cost: f64,
    pub tokens: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatmapCell {
    pub period: String,
    pub cost: f64,
    pub tokens: u64,
    /// 0-4 bucket, matching GitHub's contribution scale.
    pub level: u8,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRollup {
    pub project_name: String,
    pub repository_id: Option<String>,
    pub repository_url: Option<String>,
    pub path: String,
    pub cost: f64,
    pub tokens: u64,
    pub chats: Vec<ChatEntry>,
}

/// Everything the dashboard needs from one pass over the document.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub totals: Totals,
    pub days: Vec<DayPoint>,
    pub by_model: Vec<NamedAmount>,
    pub by_source: Vec<NamedAmount>,
    pub by_hour: Vec<HourPoint>,
    pub heatmap: Vec<HeatmapCell>,
    pub projects: Vec<ProjectRollup>,
    /// Distinct days that carry any spend, for the "N days" KPI.
    pub active_days: usize,
    pub session_count: usize,
}

fn accumulate(target: &mut NamedAmount, row: &ModelRow) {
    target.cost += row.cost;
    target.tokens += row.tokens();
    target.input_tokens += row.input_tokens;
    target.output_tokens += row.output_tokens;
    target.cache_creation_tokens += row.cache_creation_tokens;
    target.cache_read_tokens += row.cache_read_tokens;
}

/// Totals over the selected sources and range. Recomputed rather than read from
/// `document.totals`, because that field covers every source and the whole
/// history and would contradict a filtered view.
pub fn totals(document: &UsageDocument, range: &DayRange, selection: &SourceSelection) -> Totals {
    let mut totals = Totals::default();
    for day in &document.daily {
        if !range.contains(&day.period) {
            continue;
        }
        for (source, rows) in &day.by_source {
            if !selection.includes(source, document) {
                continue;
            }
            for row in rows {
                totals.cost += row.cost;
                totals.input_tokens += row.input_tokens;
                totals.output_tokens += row.output_tokens;
                totals.cache_creation_tokens += row.cache_creation_tokens;
                totals.cache_read_tokens += row.cache_read_tokens;
            }
        }
    }
    totals.tokens = totals.input_tokens
        + totals.output_tokens
        + totals.cache_creation_tokens
        + totals.cache_read_tokens;
    totals
}

pub fn per_day(
    document: &UsageDocument,
    range: &DayRange,
    selection: &SourceSelection,
) -> Vec<DayPoint> {
    let mut points: Vec<DayPoint> = document
        .daily
        .iter()
        .filter(|day| range.contains(&day.period))
        .map(|day| {
            let mut point = DayPoint {
                period: day.period.clone(),
                ..Default::default()
            };
            for (source, rows) in &day.by_source {
                if !selection.includes(source, document) {
                    continue;
                }
                for row in rows {
                    point.cost += row.cost;
                    point.tokens += row.tokens();
                }
            }
            point
        })
        .collect();
    points.sort_by(|a, b| a.period.cmp(&b.period));
    points
}

pub fn by_model(
    document: &UsageDocument,
    range: &DayRange,
    selection: &SourceSelection,
) -> Vec<NamedAmount> {
    let mut map: BTreeMap<String, NamedAmount> = BTreeMap::new();
    for day in &document.daily {
        if !range.contains(&day.period) {
            continue;
        }
        for (source, rows) in &day.by_source {
            if !selection.includes(source, document) {
                continue;
            }
            for row in rows {
                let entry = map.entry(row.model_name.clone()).or_insert_with(|| NamedAmount {
                    name: row.model_name.clone(),
                    label: row.model_name.clone(),
                    ..Default::default()
                });
                accumulate(entry, row);
            }
        }
    }
    sorted_by_cost(map)
}

pub fn by_source(
    document: &UsageDocument,
    range: &DayRange,
    selection: &SourceSelection,
) -> Vec<NamedAmount> {
    let mut map: BTreeMap<String, NamedAmount> = BTreeMap::new();
    for day in &document.daily {
        if !range.contains(&day.period) {
            continue;
        }
        for (source, rows) in &day.by_source {
            if !selection.includes(source, document) {
                continue;
            }
            let entry = map.entry(source.clone()).or_insert_with(|| NamedAmount {
                name: source.clone(),
                label: document.label_for(source),
                ..Default::default()
            });
            for row in rows {
                accumulate(entry, row);
            }
        }
    }
    sorted_by_cost(map)
}

fn sorted_by_cost(map: BTreeMap<String, NamedAmount>) -> Vec<NamedAmount> {
    let mut list: Vec<NamedAmount> = map.into_values().collect();
    // Highest spend first, name as the tiebreak so the order is stable.
    list.sort_by(|a, b| {
        b.cost
            .partial_cmp(&a.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    list
}

/// Spend by hour of day, always 24 buckets so the chart has a fixed axis even
/// when the machine was only used in the evening.
pub fn by_hour(
    document: &UsageDocument,
    range: &DayRange,
    selection: &SourceSelection,
) -> Vec<HourPoint> {
    let mut buckets: Vec<HourPoint> = (0..24)
        .map(|hour| HourPoint {
            hour,
            ..Default::default()
        })
        .collect();

    for day in &document.daily {
        if !range.contains(&day.period) {
            continue;
        }
        for (index, bucket) in day.hours.iter().enumerate().take(24) {
            // Per-source detail is the only way to honour the filter; the
            // bucket's own totals cover every source.
            let mut cost = 0.0;
            let mut tokens = 0u64;
            for (source, amount) in &bucket.by_source {
                if !selection.includes(source, document) {
                    continue;
                }
                cost += amount.cost;
                tokens += amount.tokens;
            }
            buckets[index].cost += cost;
            buckets[index].tokens += tokens;
        }
    }
    buckets
}

/// GitHub-style calendar. Levels are quartiles of the non-zero days, so the
/// scale adapts to the user's own spend instead of a fixed dollar cutoff.
pub fn heatmap(
    document: &UsageDocument,
    range: &DayRange,
    selection: &SourceSelection,
) -> Vec<HeatmapCell> {
    let days = per_day(document, range, selection);
    let mut spent: Vec<f64> = days.iter().map(|d| d.cost).filter(|c| *c > 0.0).collect();
    spent.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Level by percentile rank among active days rather than by fixed
    // quartile values. With only a handful of active days the quartile
    // boundaries collapse onto the maximum, which pushed the busiest day into
    // the middle of the scale; ranking keeps the busiest day at 4 and the
    // quietest at 1 for any number of days. Days sharing a cost share a level,
    // because the rank counts every value at or below it.
    let active = spent.len();
    let level_for = |cost: f64| -> u8 {
        if cost <= 0.0 || active == 0 {
            return 0;
        }
        let rank = spent.partition_point(|value| *value <= cost);
        let fraction = rank as f64 / active as f64;
        (1 + (4.0 * fraction).floor() as u8).min(4)
    };

    days.into_iter()
        .map(|day| {
            let level = level_for(day.cost);
            HeatmapCell {
                period: day.period,
                cost: day.cost,
                tokens: day.tokens,
                level,
            }
        })
        .collect()
}

/// Projects rolled up across the range, keyed by repository when the collector
/// identified one so the same repo checked out twice counts once.
pub fn projects(
    document: &UsageDocument,
    range: &DayRange,
    selection: &SourceSelection,
) -> Vec<ProjectRollup> {
    let mut map: BTreeMap<String, ProjectRollup> = BTreeMap::new();

    for day in &document.daily {
        if !range.contains(&day.period) {
            continue;
        }
        for project in &day.projects {
            let key = project
                .repository_id
                .clone()
                .filter(|id| !id.is_empty())
                .unwrap_or_else(|| project.path.clone());

            let (cost, tokens) = project_amount(project, document, selection);
            if cost == 0.0 && tokens == 0 {
                continue;
            }

            let entry = map.entry(key).or_insert_with(|| ProjectRollup {
                project_name: project.project_name.clone(),
                repository_id: project.repository_id.clone(),
                repository_url: project.repository_url.clone(),
                path: project.path.clone(),
                ..Default::default()
            });
            entry.cost += cost;
            entry.tokens += tokens;
            for chat in &project.chats {
                if selection.includes(&chat.source, document) {
                    entry.chats.push(chat.clone());
                }
            }
            // A worktree's chats belong to the same repository.
            for worktree in &project.worktrees {
                for chat in &worktree.chats {
                    if selection.includes(&chat.source, document) {
                        entry.chats.push(chat.clone());
                    }
                }
            }
        }
    }

    let mut list: Vec<ProjectRollup> = map.into_values().collect();
    for project in &mut list {
        // The same chat appears on every day it was active; keep the latest
        // record for each id so totals are not double counted in the drilldown.
        project.chats.sort_by(|a, b| {
            a.id.cmp(&b.id).then_with(|| b.last_ts.cmp(&a.last_ts))
        });
        project.chats.dedup_by(|a, b| a.id == b.id);
        project
            .chats
            .sort_by(|a, b| b.last_ts.cmp(&a.last_ts));
    }
    list.sort_by(|a, b| {
        b.cost
            .partial_cmp(&a.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.project_name.cmp(&b.project_name))
    });
    list
}

fn project_amount(
    project: &ProjectEntry,
    document: &UsageDocument,
    selection: &SourceSelection,
) -> (f64, u64) {
    // Prefer the per-source breakdown so the filter applies; fall back to the
    // project total when a collector wrote no breakdown.
    if project.by_source.is_empty() {
        return (project.cost, project.tokens);
    }
    let mut cost = 0.0;
    let mut tokens = 0u64;
    for (source, detail) in &project.by_source {
        if selection.includes(source, document) {
            cost += detail.cost;
            tokens += detail.tokens;
        }
    }
    (cost, tokens)
}

/// One pass producing every rollup the dashboard renders.
pub fn dashboard(
    document: &UsageDocument,
    range: &DayRange,
    selection: &SourceSelection,
) -> Dashboard {
    let days = per_day(document, range, selection);
    let active_days = days.iter().filter(|d| d.cost > 0.0 || d.tokens > 0).count();
    let session_count = document
        .sessions
        .iter()
        .filter(|s| selection.includes(&s.source, document))
        .count();

    Dashboard {
        totals: totals(document, range, selection),
        by_model: by_model(document, range, selection),
        by_source: by_source(document, range, selection),
        by_hour: by_hour(document, range, selection),
        heatmap: heatmap(document, range, selection),
        projects: projects(document, range, selection),
        days,
        active_days,
        session_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Amount, DailyEntry, HourBucket, SessionRef, SourceDetail};

    fn row(model: &str, cost: f64, input: u64) -> ModelRow {
        ModelRow {
            model_name: model.into(),
            input_tokens: input,
            output_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            cost,
        }
    }

    fn document() -> UsageDocument {
        let mut doc = UsageDocument {
            schema_version: 8,
            sources: vec!["cli".into(), "codex".into()],
            default_sources: vec!["cli".into(), "codex".into()],
            sessions: vec![
                SessionRef { id: "a".into(), source: "cli".into() },
                SessionRef { id: "b".into(), source: "codex".into() },
            ],
            ..Default::default()
        };
        doc.source_meta.insert(
            "cli".into(),
            crate::models::SourceMeta { label: "Claude Code".into(), tool: "Claude Code".into() },
        );

        let mut day1 = DailyEntry {
            period: "2026-08-18".into(),
            hours: (0..24).map(|_| HourBucket::default()).collect(),
            ..Default::default()
        };
        day1.by_source.insert("cli".into(), vec![row("opus", 10.0, 100)]);
        day1.by_source.insert("codex".into(), vec![row("gpt", 5.0, 50)]);
        day1.hours[9].by_source.insert("cli".into(), Amount { tokens: 100, cost: 10.0 });
        day1.hours[9].by_source.insert("codex".into(), Amount { tokens: 50, cost: 5.0 });

        let mut day2 = DailyEntry {
            period: "2026-08-19".into(),
            hours: (0..24).map(|_| HourBucket::default()).collect(),
            ..Default::default()
        };
        day2.by_source.insert("cli".into(), vec![row("opus", 1.0, 10)]);

        let mut project = ProjectEntry {
            project_name: "veronica".into(),
            repository_id: Some("github.com/x/veronica".into()),
            path: "/home/u/veronica".into(),
            cost: 15.0,
            tokens: 150,
            ..Default::default()
        };
        project.by_source.insert(
            "cli".into(),
            SourceDetail { tokens: 100, cost: 10.0, ..Default::default() },
        );
        project.by_source.insert(
            "codex".into(),
            SourceDetail { tokens: 50, cost: 5.0, ..Default::default() },
        );
        project.chats.push(ChatEntry {
            id: "chat1".into(),
            source: "cli".into(),
            cost: 10.0,
            last_ts: Some(2),
            ..Default::default()
        });
        day1.projects.push(project.clone());
        // The same chat recurs the next day; the drilldown must not list it twice.
        day2.projects.push(project);

        doc.daily = vec![day1, day2];
        doc
    }

    #[test]
    fn totals_are_recomputed_for_the_selection_not_read_from_the_document() {
        let doc = document();
        let all = totals(&doc, &DayRange::default(), &SourceSelection::All);
        assert_eq!(all.cost, 16.0);
        assert_eq!(all.tokens, 160);

        let cli_only = totals(
            &doc,
            &DayRange::default(),
            &SourceSelection::Only(vec!["cli".into()]),
        );
        assert_eq!(cli_only.cost, 11.0);
        assert_eq!(cli_only.tokens, 110);
    }

    #[test]
    fn the_range_filter_excludes_days_outside_it() {
        let doc = document();
        let range = DayRange {
            start: Some("2026-08-19".into()),
            end: None,
        };
        assert_eq!(totals(&doc, &range, &SourceSelection::All).cost, 1.0);
    }

    #[test]
    fn per_day_is_sorted_ascending_so_the_chart_reads_left_to_right() {
        let doc = document();
        let days = per_day(&doc, &DayRange::default(), &SourceSelection::All);
        assert_eq!(
            days.iter().map(|d| d.period.as_str()).collect::<Vec<_>>(),
            vec!["2026-08-18", "2026-08-19"]
        );
    }

    #[test]
    fn model_and_source_tables_sort_by_spend_descending() {
        let doc = document();
        let models = by_model(&doc, &DayRange::default(), &SourceSelection::All);
        assert_eq!(models[0].name, "opus");
        assert_eq!(models[0].cost, 11.0);
        assert_eq!(models[1].name, "gpt");

        let sources = by_source(&doc, &DayRange::default(), &SourceSelection::All);
        assert_eq!(sources[0].name, "cli");
        // The label comes from sourceMeta, the id when there is none.
        assert_eq!(sources[0].label, "Claude Code");
        assert_eq!(sources[1].label, "codex");
    }

    #[test]
    fn hourly_always_has_twenty_four_buckets_and_honours_the_filter() {
        let doc = document();
        let hours = by_hour(&doc, &DayRange::default(), &SourceSelection::All);
        assert_eq!(hours.len(), 24);
        assert_eq!(hours[9].cost, 15.0);
        assert_eq!(hours[0].cost, 0.0);

        let cli = by_hour(
            &doc,
            &DayRange::default(),
            &SourceSelection::Only(vec!["cli".into()]),
        );
        assert_eq!(cli[9].cost, 10.0);
    }

    #[test]
    fn heatmap_levels_are_relative_to_the_users_own_spend() {
        let doc = document();
        let cells = heatmap(&doc, &DayRange::default(), &SourceSelection::All);
        assert_eq!(cells.len(), 2);
        // $1 is the low day and $15 the high one, so they land in different bands
        // and the busiest day reaches the top of the scale even with only two
        // active days.
        let low = cells.iter().find(|c| c.period == "2026-08-19").unwrap();
        let high = cells.iter().find(|c| c.period == "2026-08-18").unwrap();
        assert_eq!(high.level, 4);
        assert!(low.level >= 1 && low.level < high.level, "low was {}", low.level);
    }

    #[test]
    fn heatmap_gives_the_busiest_day_the_top_level_at_any_scale() {
        // Regression: fixed quartile boundaries collapsed onto the maximum when
        // few days were active, so the busiest day rendered mid-scale.
        for count in 1..=40usize {
            let doc = UsageDocument {
                schema_version: 8,
                default_sources: vec!["cli".into()],
                daily: (0..count)
                    .map(|i| {
                        let mut day = DailyEntry {
                            period: format!("2026-01-{:02}", i + 1),
                            ..Default::default()
                        };
                        day.by_source
                            .insert("cli".into(), vec![row("m", (i + 1) as f64, 1)]);
                        day
                    })
                    .collect(),
                ..Default::default()
            };
            let cells = heatmap(&doc, &DayRange::default(), &SourceSelection::All);
            assert_eq!(cells.len(), count);
            assert_eq!(
                cells.last().unwrap().level,
                4,
                "busiest of {count} days should be level 4"
            );
            assert!(
                cells.iter().all(|c| c.level >= 1),
                "every active day should be at least level 1 with {count} days"
            );
        }
    }

    #[test]
    fn heatmap_gives_days_with_equal_spend_the_same_level() {
        let doc = UsageDocument {
            schema_version: 8,
            default_sources: vec!["cli".into()],
            daily: (0..6)
                .map(|i| {
                    let mut day = DailyEntry {
                        period: format!("2026-02-{:02}", i + 1),
                        ..Default::default()
                    };
                    day.by_source.insert("cli".into(), vec![row("m", 7.0, 1)]);
                    day
                })
                .collect(),
            ..Default::default()
        };
        let cells = heatmap(&doc, &DayRange::default(), &SourceSelection::All);
        let levels: Vec<u8> = cells.iter().map(|c| c.level).collect();
        assert_eq!(levels, vec![4; 6], "identical spend must share a level");
    }

    #[test]
    fn a_zero_day_is_level_zero() {
        let mut doc = document();
        doc.daily.push(DailyEntry {
            period: "2026-08-20".into(),
            ..Default::default()
        });
        let cells = heatmap(&doc, &DayRange::default(), &SourceSelection::All);
        let idle = cells.iter().find(|c| c.period == "2026-08-20").unwrap();
        assert_eq!(idle.level, 0);
    }

    #[test]
    fn projects_roll_up_across_days_and_deduplicate_their_chats() {
        let doc = document();
        let rollups = projects(&doc, &DayRange::default(), &SourceSelection::All);
        assert_eq!(rollups.len(), 1);
        // Present on both days, so the spend adds up.
        assert_eq!(rollups[0].cost, 30.0);
        // But the chat is the same one, listed once.
        assert_eq!(rollups[0].chats.len(), 1);
        assert_eq!(rollups[0].chats[0].id, "chat1");
    }

    #[test]
    fn deselecting_a_source_removes_its_chats_and_spend_from_projects() {
        let doc = document();
        let rollups = projects(
            &doc,
            &DayRange::default(),
            &SourceSelection::Only(vec!["codex".into()]),
        );
        assert_eq!(rollups[0].cost, 10.0, "only the codex half of both days");
        assert!(rollups[0].chats.is_empty(), "the only chat came from cli");
    }

    #[test]
    fn last_days_window_uses_days_present_in_the_document() {
        let doc = document();
        let range = DayRange::last_days(&doc, 1);
        assert_eq!(range.start.as_deref(), Some("2026-08-19"));
        assert_eq!(range.end.as_deref(), Some("2026-08-19"));
    }

    #[test]
    fn last_days_clamps_when_asked_for_more_days_than_exist() {
        let doc = document();
        let range = DayRange::last_days(&doc, 90);
        assert_eq!(range.start.as_deref(), Some("2026-08-18"));
    }

    #[test]
    fn dashboard_counts_active_days_and_sessions_for_the_selection() {
        let doc = document();
        let board = dashboard(&doc, &DayRange::default(), &SourceSelection::All);
        assert_eq!(board.active_days, 2);
        assert_eq!(board.session_count, 2);

        let cli = dashboard(
            &doc,
            &DayRange::default(),
            &SourceSelection::Only(vec!["cli".into()]),
        );
        assert_eq!(cli.session_count, 1);
    }

    #[test]
    fn an_empty_document_produces_an_empty_dashboard_without_panicking() {
        let doc = UsageDocument { schema_version: 8, ..Default::default() };
        let board = dashboard(&doc, &DayRange::default(), &SourceSelection::All);
        assert_eq!(board.totals.cost, 0.0);
        assert_eq!(board.by_hour.len(), 24);
        assert!(board.heatmap.is_empty());
        assert_eq!(board.active_days, 0);
    }
}
