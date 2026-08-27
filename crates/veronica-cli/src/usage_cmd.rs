//! `vr usage` — the numbers the dashboard rings and charts show.

use anyhow::{Context, Result};
use clap::Subcommand;
use veronica_core::AppDirectories;
use veronica_usage::aggregate::{self, DayRange, SourceSelection};
use veronica_usage::collector;

use crate::format::{self, countdown, money, tokens, Output};

#[derive(Subcommand)]
pub enum UsageCommand {
    /// Totals, day count and model spread for a window.
    Summary {
        /// How many recent days to include; omit for the full history.
        #[arg(long)]
        days: Option<usize>,
        /// Restrict to these collector ids, e.g. --source cli --source codex.
        #[arg(long = "source")]
        sources: Vec<String>,
    },
    /// Spend per collector.
    Sources {
        #[arg(long)]
        days: Option<usize>,
    },
    /// Spend per model.
    Models {
        #[arg(long)]
        days: Option<usize>,
        #[arg(long = "source")]
        sources: Vec<String>,
    },
    /// Spend per project, with its chats.
    Projects {
        #[arg(long)]
        days: Option<usize>,
        #[arg(long = "source")]
        sources: Vec<String>,
        /// Show the chats inside each project.
        #[arg(long)]
        chats: bool,
    },
    /// The daily spend calendar.
    Calendar {
        #[arg(long)]
        days: Option<usize>,
    },
    /// Rate limits for Claude and Codex, as live gauges.
    Limits {
        /// Warn above this utilisation.
        #[arg(long, default_value_t = 60)]
        warn: i64,
        /// Treat above this as critical.
        #[arg(long, default_value_t = 85)]
        critical: i64,
    },
    /// What the alert notifier would do right now.
    ///
    /// A dry run: it reads the live windows and the stored notifier state, then
    /// reports which alerts a poll at this instant would post. Nothing is
    /// posted and no state is written, so it is safe to run repeatedly.
    Alerts {
        /// Also print every switch, so a misfiring alert can be traced to one.
        #[arg(long)]
        settings: bool,
    },
    /// Run the collector and rewrite usage.json.
    Refresh {
        /// Print each collector phase as it completes.
        #[arg(long)]
        progress: bool,
    },
}

/// Load the document the collector last wrote, with a clear error when the
/// user has never refreshed.
fn load(directories: &AppDirectories) -> Result<veronica_usage::UsageDocument> {
    let path = directories.usage_file();
    collector::read_document(&path)?.with_context(|| {
        format!(
            "no usage data at {}. Run `vr usage refresh` first.",
            path.display()
        )
    })
}

fn selection(sources: &[String]) -> SourceSelection {
    if sources.is_empty() {
        SourceSelection::All
    } else {
        SourceSelection::Only(sources.to_vec())
    }
}

fn range(document: &veronica_usage::UsageDocument, days: Option<usize>) -> DayRange {
    match days {
        Some(days) => DayRange::last_days(document, days),
        None => DayRange::default(),
    }
}

pub async fn run(
    directories: &AppDirectories,
    command: &UsageCommand,
    output: Output,
) -> Result<()> {
    match command {
        UsageCommand::Summary { days, sources } => {
            let document = load(directories)?;
            let range = range(&document, *days);
            let board = aggregate::dashboard(&document, &range, &selection(sources));

            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Summary<'a> {
                generated_at: &'a str,
                start: Option<&'a str>,
                end: Option<&'a str>,
                active_days: usize,
                sessions: usize,
                totals: veronica_usage::models::Totals,
                sources: Vec<&'a str>,
                models: usize,
            }

            let summary = Summary {
                generated_at: &document.generated_at,
                start: range.start.as_deref(),
                end: range.end.as_deref(),
                active_days: board.active_days,
                sessions: board.session_count,
                totals: board.totals,
                sources: board.by_source.iter().map(|s| s.name.as_str()).collect(),
                models: board.by_model.len(),
            };

            output.emit(&summary, || {
                use std::fmt::Write;
                let mut out = String::new();
                let window = match (range.start.as_deref(), range.end.as_deref()) {
                    (Some(start), Some(end)) if start != end => format!("{start} to {end}"),
                    (Some(start), _) => start.to_string(),
                    _ => "all time".to_string(),
                };
                let _ = writeln!(out, "Spend      {}", money(summary.totals.cost));
                let _ = writeln!(out, "Tokens     {}", tokens(summary.totals.tokens));
                let _ = writeln!(
                    out,
                    "Window     {window} · {} active days",
                    summary.active_days
                );
                let _ = writeln!(out, "Sessions   {}", summary.sessions);
                let _ = writeln!(out, "Models     {}", summary.models);
                let _ = write!(out, "Sources    {}", summary.sources.join(", "));
                out
            })
        }

        UsageCommand::Sources { days } => {
            let document = load(directories)?;
            let range = range(&document, *days);
            let rows = aggregate::by_source(&document, &range, &SourceSelection::All);
            output.emit(&rows, || {
                let table: Vec<Vec<String>> = rows
                    .iter()
                    .map(|row| {
                        vec![
                            row.name.clone(),
                            row.label.clone(),
                            money(row.cost),
                            tokens(row.tokens),
                        ]
                    })
                    .collect();
                format::table(&["id", "label", "cost", "tokens"], &table)
            })
        }

        UsageCommand::Models { days, sources } => {
            let document = load(directories)?;
            let range = range(&document, *days);
            let rows = aggregate::by_model(&document, &range, &selection(sources));
            output.emit(&rows, || {
                let table: Vec<Vec<String>> = rows
                    .iter()
                    .map(|row| {
                        vec![
                            row.name.clone(),
                            money(row.cost),
                            tokens(row.tokens),
                            tokens(row.input_tokens),
                            tokens(row.output_tokens),
                            tokens(row.cache_read_tokens),
                        ]
                    })
                    .collect();
                format::table(
                    &["model", "cost", "tokens", "input", "output", "cache read"],
                    &table,
                )
            })
        }

        UsageCommand::Projects {
            days,
            sources,
            chats,
        } => {
            let document = load(directories)?;
            let range = range(&document, *days);
            let rows = aggregate::projects(&document, &range, &selection(sources));
            let show_chats = *chats;
            output.emit(&rows, || {
                use std::fmt::Write;
                if show_chats {
                    let mut out = String::new();
                    for project in &rows {
                        let _ = writeln!(
                            out,
                            "{}  {}  {}",
                            project.project_name,
                            money(project.cost),
                            tokens(project.tokens)
                        );
                        for chat in &project.chats {
                            let title = if chat.title.is_empty() {
                                chat.id.as_str()
                            } else {
                                chat.title.as_str()
                            };
                            let _ = writeln!(
                                out,
                                "    {}  {}  {}",
                                money(chat.cost),
                                chat.source,
                                title
                            );
                        }
                    }
                    return out.trim_end().to_string();
                }
                let table: Vec<Vec<String>> = rows
                    .iter()
                    .map(|project| {
                        vec![
                            project.project_name.clone(),
                            money(project.cost),
                            tokens(project.tokens),
                            project.chats.len().to_string(),
                            project
                                .repository_id
                                .clone()
                                .unwrap_or_else(|| project.path.clone()),
                        ]
                    })
                    .collect();
                format::table(
                    &["project", "cost", "tokens", "chats", "repository"],
                    &table,
                )
            })
        }

        UsageCommand::Calendar { days } => {
            let document = load(directories)?;
            let range = range(&document, *days);
            let cells = aggregate::heatmap(&document, &range, &SourceSelection::All);
            output.emit(&cells, || {
                // Five shades, quietest to busiest, so the calendar reads in a
                // terminal the same way it does in the app.
                const BLOCKS: [&str; 5] = ["·", "░", "▒", "▓", "█"];
                let mut out = String::new();
                for cell in &cells {
                    let block = BLOCKS[cell.level.min(4) as usize];
                    out.push_str(&format!(
                        "{}  {}  {:>9}\n",
                        cell.period,
                        block,
                        money(cell.cost)
                    ));
                }
                out.trim_end().to_string()
            })
        }

        UsageCommand::Alerts { settings: show_settings } => {
            use veronica_usage::alerts::{self, NotifierState, NotifySettings};

            let stored = veronica_core::Settings::load(&directories.settings_file())?;
            let notify = NotifySettings::from_settings(&stored);
            let now = chrono::Utc::now();

            // A copy of the persisted state, so the dry run cannot consume the
            // edge a real poll would fire on.
            let mut state = NotifierState::load(&directories.alerts_state_file());
            let (session, week, note) =
                match veronica_usage::claude::limits_for_user(now).await {
                    Ok(Some(limits)) => (limits.session, limits.week, None),
                    Ok(None) => (
                        None,
                        None,
                        Some("Claude is not signed in on this computer".to_string()),
                    ),
                    Err(error) => (None, None, Some(format!("{error:#}"))),
                };

            let mut would_post =
                alerts::decide(session, week, &notify, &mut state, now);
            would_post.extend(alerts::due_reminders(
                session, week, &notify, &mut state, now,
            ));

            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Report<'a> {
                enabled: bool,
                note: Option<String>,
                session: Option<Preview>,
                week: Option<Preview>,
                would_post: &'a [alerts::LimitAlert],
                session_reminder_at: Option<String>,
                week_reminder_at: Option<String>,
                #[serde(skip_serializing_if = "Option::is_none")]
                settings: Option<&'a NotifySettings>,
            }

            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Preview {
                percent: f64,
                resets_in: Option<String>,
            }

            let preview = |window: Option<veronica_usage::limits::LimitWindow>| {
                window.map(|window| Preview {
                    percent: window.percent,
                    resets_in: window
                        .resets_at
                        .filter(|at| *at > now)
                        .map(|at| alerts::countdown(now, at)),
                })
            };

            let report = Report {
                enabled: notify.master,
                note: note.clone(),
                session: preview(session),
                week: preview(week),
                would_post: &would_post,
                session_reminder_at: alerts::reminder_fire_at(
                    session.and_then(|w| w.resets_at),
                    notify.reminder_session_offset_min,
                    now,
                )
                .filter(|_| notify.reminder_session)
                .map(|at| at.to_rfc3339()),
                week_reminder_at: alerts::reminder_fire_at(
                    week.and_then(|w| w.resets_at),
                    notify.reminder_weekly_offset_min,
                    now,
                )
                .filter(|_| notify.reminder_weekly)
                .map(|at| at.to_rfc3339()),
                settings: show_settings.then_some(&notify),
            };

            output.emit(&report, || {
                use std::fmt::Write;
                let mut out = String::new();
                let _ = writeln!(
                    out,
                    "alerts are {}",
                    if notify.master { "on" } else { "off (vr config set notifyMaster true)" }
                );
                if let Some(note) = &note {
                    let _ = writeln!(out, "note      {note}");
                }
                for (label, window) in [("session", session), ("weekly ", week)] {
                    if let Some(window) = window {
                        let left = window
                            .resets_at
                            .filter(|at| *at > now)
                            .map(|at| alerts::countdown(now, at))
                            .unwrap_or_else(|| "unknown".into());
                        let _ = writeln!(
                            out,
                            "{label}   {:>5.1}%  resets in {left}",
                            window.percent
                        );
                    }
                }
                if would_post.is_empty() {
                    let _ = writeln!(out, "\nnothing would be posted right now");
                } else {
                    let _ = writeln!(out, "\nwould post now:");
                    for alert in &would_post {
                        let _ = writeln!(out, "  [{}] {} — {}", alert.id, alert.title, alert.body);
                    }
                }
                out.trim_end().to_string()
            })
        }

        UsageCommand::Limits { warn, critical } => {
            use veronica_usage::limits::UsageThresholds;

            let thresholds = UsageThresholds {
                warning_percent: *warn,
                critical_percent: *critical,
            };
            let report = veronica_usage::gauges::collect(
                chrono::Utc::now(),
                veronica_usage::gauges::DEFAULT_PACING_MARGIN,
            )
            .await;

            output.emit(&report, || {
                use std::fmt::Write;
                let mut out = String::new();
                if report.gauges.is_empty() {
                    let _ = write!(out, "no rate limits available");
                } else {
                    let rows: Vec<Vec<String>> = report
                        .gauges
                        .iter()
                        .map(|gauge| {
                            let level = veronica_usage::limits::UsageLevel::from_percent(
                                gauge.percent,
                                thresholds,
                            );
                            vec![
                                gauge.provider.clone(),
                                gauge.window.clone(),
                                bar(gauge.percent),
                                format!("{:.0}%", gauge.percent),
                                gauge
                                    .resets_in_secs
                                    .map(countdown)
                                    .unwrap_or_else(|| "—".into()),
                                format!("{:?}", gauge.zone).to_lowercase(),
                                format!("{level:?}").to_lowercase(),
                            ]
                        })
                        .collect();
                    let _ = write!(
                        out,
                        "{}",
                        format::table(
                            &["provider", "window", "", "used", "resets", "pace", "level"],
                            &rows
                        )
                    );
                }
                for note in &report.notes {
                    let _ = write!(out, "\n{note}");
                }
                out
            })
        }

        UsageCommand::Refresh { progress } => {
            let script = directories.collector_script();
            collector::install_script(&script)?;
            let out_dir = directories.usage_dir();
            let show = *progress && output == Output::Text;

            let outcome = collector::refresh(&script, &out_dir, &directories.cache, |event| {
                if show {
                    // Progress goes to stderr so stdout stays one document.
                    match event {
                        veronica_usage::CollectorEvent::Phase {
                            name,
                            detail,
                            seconds,
                        } => eprintln!("  {name:<14} {detail}  ({seconds:.2}s)"),
                        veronica_usage::CollectorEvent::Note { message } => {
                            eprintln!("  … {message}")
                        }
                        veronica_usage::CollectorEvent::Error { message } => {
                            eprintln!("  ! {message}")
                        }
                        _ => {}
                    }
                }
            })
            .await?;

            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Refreshed {
                completed: bool,
                generated_at: String,
                sources: Vec<String>,
                totals: veronica_usage::models::Totals,
                days: usize,
                errors: Vec<String>,
            }

            let refreshed = Refreshed {
                completed: outcome.completed,
                generated_at: outcome.document.generated_at.clone(),
                sources: outcome.document.sources.clone(),
                totals: outcome.document.totals,
                days: outcome.document.daily.len(),
                errors: outcome.errors().into_iter().map(String::from).collect(),
            };

            output.emit(&refreshed, || {
                use std::fmt::Write;
                let mut out = String::new();
                for (name, detail) in outcome.summaries() {
                    let _ = writeln!(out, "{name:<10} {detail}");
                }
                let _ = write!(
                    out,
                    "collected  {} across {} days from {}",
                    money(refreshed.totals.cost),
                    refreshed.days,
                    refreshed.sources.join(", ")
                );
                out
            })
        }
    }
}

/// A ten-cell bar for a percentage, so a terminal reading is scannable.
fn bar(percent: f64) -> String {
    let filled = ((percent / 10.0).round() as usize).min(10);
    format!("[{}{}]", "#".repeat(filled), "·".repeat(10 - filled))
}

#[cfg(test)]
mod tests {
    use super::bar;

    #[test]
    fn the_bar_fills_proportionally_and_clamps() {
        assert_eq!(bar(0.0), "[··········]");
        assert_eq!(bar(50.0), "[#####·····]");
        assert_eq!(bar(100.0), "[##########]");
        // Over the limit still renders, rather than overflowing the cell count.
        assert_eq!(bar(140.0), "[##########]");
    }
}
