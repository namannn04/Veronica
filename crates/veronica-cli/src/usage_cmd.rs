use std::fmt::Write as _;

use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::json;
use veronica_core::AppDirectories;
use veronica_usage::aggregate::{self, DayRange, SourceSelection};
use veronica_usage::collector;

use crate::format::{self, countdown, money, tokens, Output};

#[derive(Subcommand)]
pub(crate) enum UsageCommand {
    #[command(about = "Totals, day count and model spread for a window")]
    Summary {
        #[arg(
            long,
            help = "How many recent days to include; omit for the full history"
        )]
        days: Option<usize>,
        #[arg(
            long = "source",
            help = "Restrict to these collector ids, e.g. --source cli --source codex"
        )]
        sources: Vec<String>,
    },
    #[command(about = "Spend per collector")]
    Sources {
        #[arg(long)]
        days: Option<usize>,
    },
    #[command(about = "Spend per model")]
    Models {
        #[arg(long)]
        days: Option<usize>,
        #[arg(long = "source")]
        sources: Vec<String>,
    },
    #[command(about = "Spend per project, with its chats")]
    Projects {
        #[arg(long)]
        days: Option<usize>,
        #[arg(long = "source")]
        sources: Vec<String>,
        #[arg(long, help = "Show the chats inside each project")]
        chats: bool,
    },
    #[command(about = "The daily spend calendar")]
    Calendar {
        #[arg(long)]
        days: Option<usize>,
    },
    #[command(about = "Rate limits for Claude and Codex, as live gauges")]
    Limits {
        #[arg(long, default_value_t = 60, help = "Warn above this utilisation")]
        warn: i64,
        #[arg(long, default_value_t = 85, help = "Treat above this as critical")]
        critical: i64,
    },
    #[command(
        about = "What the alert notifier would do right now",
        long_about = "What the alert notifier would do right now.\n\nA dry run: it reads the live windows and the stored notifier state, then reports which alerts a poll at this instant would post. Nothing is posted and no state is written, so it is safe to run repeatedly."
    )]
    Alerts {
        #[arg(
            long,
            help = "Also print every switch, so a misfiring alert can be traced to one"
        )]
        settings: bool,
    },
    #[command(
        about = "Render usage as branded PNG cards, for sharing",
        long_about = "Render usage as branded PNG cards, for sharing.\n\nA card never shows a repository name, a folder path, a chat title or a dollar cost: it exists to be posted somewhere public, and those are the four fields that would leak a client, an employer or an income."
    )]
    Export {
        #[arg(
            long = "card",
            help = "Which card. Repeat for several; `all` is every one. Defaults to all"
        )]
        cards: Vec<String>,
        #[arg(
            long,
            help = "A directory, or the destination file when there is exactly one card. Defaults to the current directory"
        )]
        output: Option<String>,
        #[arg(
            long,
            help = "How many recent days to include; omit for the full history"
        )]
        days: Option<usize>,
        #[arg(long = "source")]
        sources: Vec<String>,
        #[arg(
            long,
            help = "Write the SVG rather than a PNG. Useful with no fonts installed, and for editing a card before posting it"
        )]
        svg: bool,
    },
    #[command(about = "Run the collector and rewrite usage.json")]
    Refresh {
        #[arg(long, help = "Print each collector phase as it completes")]
        progress: bool,
    },
}

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

fn range(days: Option<usize>) -> DayRange {
    match days {
        Some(days) => DayRange::last_days(days, chrono::Local::now().date_naive()),
        None => DayRange::default(),
    }
}

pub(crate) async fn run(
    directories: &AppDirectories,
    command: &UsageCommand,
    output: Output,
) -> Result<()> {
    match command {
        UsageCommand::Summary { days, sources } => {
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

            let document = load(directories)?;
            let range = range(*days);
            let board = aggregate::dashboard(&document, &range, &selection(sources));

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
            let range = range(*days);
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
            let range = range(*days);
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
            let range = range(*days);
            let rows = aggregate::projects(&document, &range, &selection(sources));
            let show_chats = *chats;
            output.emit(&rows, || {
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
            let range = range(*days);
            let cells = aggregate::heatmap(&document, &range, &SourceSelection::All);
            output.emit(&cells, || {
                const BLOCKS: [&str; 5] = ["·", "░", "▒", "▓", "█"];
                let mut out = String::new();
                for cell in &cells {
                    let block = BLOCKS[cell.level.min(4) as usize];
                    let _ = writeln!(out, "{}  {}  {:>9}", cell.period, block, money(cell.cost));
                }
                out.trim_end().to_string()
            })
        }

        UsageCommand::Alerts {
            settings: show_settings,
        } => {
            use veronica_usage::alerts::{self, NotifierState, NotifySettings};

            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Preview {
                percent: f64,
                resets_in: Option<String>,
            }

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

            let stored = veronica_core::Settings::load(&directories.settings_file())?;
            let notify = NotifySettings::from_settings(&stored);
            let now = chrono::Utc::now();

            let mut state = NotifierState::load(&directories.alerts_state_file());
            let (session, week, note) = match veronica_usage::claude::limits_for_user(now).await {
                Ok(Some(limits)) => (limits.session, limits.week, None),
                Ok(None) => (
                    None,
                    None,
                    Some("Claude is not signed in on this computer".to_string()),
                ),
                Err(error) => (None, None, Some(format!("{error:#}"))),
            };

            let mut would_post = alerts::decide(session, week, &notify, &mut state, now);
            would_post.extend(alerts::due_reminders(
                session, week, &notify, &mut state, now,
            ));

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
                let mut out = String::new();
                let _ = writeln!(
                    out,
                    "alerts are {}",
                    if notify.master {
                        "on"
                    } else {
                        "off (vr config set notifyMaster true)"
                    }
                );
                if let Some(note) = &note {
                    let _ = writeln!(out, "note      {note}");
                }
                for (label, window) in [("session", session), ("weekly ", week)] {
                    if let Some(window) = window {
                        let left = window
                            .resets_at
                            .filter(|at| *at > now)
                            .map_or_else(|| "unknown".into(), |at| alerts::countdown(now, at));
                        let _ =
                            writeln!(out, "{label}   {:>5.1}%  resets in {left}", window.percent);
                    }
                }
                if would_post.is_empty() {
                    let _ = writeln!(out, "\nnothing would be posted right now");
                } else {
                    let _ = writeln!(out, "\nwould post now:");
                    for alert in &would_post {
                        let _ = writeln!(out, "  [{}] {}: {}", alert.id, alert.title, alert.body);
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
                                gauge.resets_in_secs.map_or_else(|| "-".into(), countdown),
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

        UsageCommand::Export {
            cards,
            output: destination,
            days,
            sources,
            svg,
        } => {
            use veronica_usage::cards::{self, Card};

            let chosen: Vec<Card> = if cards.is_empty() {
                Card::ALL.to_vec()
            } else {
                let mut chosen = Vec::new();
                for raw in cards {
                    let matched = Card::parse(raw).with_context(|| {
                        let known: Vec<&str> = Card::ALL.iter().map(|c| c.key()).collect();
                        format!("unknown card '{raw}'; try one of {}, all", known.join(", "))
                    })?;
                    for card in matched {
                        if !chosen.contains(&card) {
                            chosen.push(card);
                        }
                    }
                }
                chosen
            };

            let document = load(directories)?;
            let range = range(*days);
            let board = aggregate::dashboard(&document, &range, &selection(sources));
            if board.totals.tokens == 0 {
                anyhow::bail!("no usage in this window, so there is nothing to put on a card");
            }

            let label = match (range.start.as_deref(), range.end.as_deref()) {
                (Some(start), Some(end)) if start == end => start.to_string(),
                (Some(start), Some(end)) => format!("{start} to {end}"),
                _ => "all time".to_string(),
            };
            let extension = if *svg { "svg" } else { "png" };
            let stamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string();

            let target = destination.as_deref().map(std::path::Path::new);
            let single_file = match target {
                Some(path) if chosen.len() == 1 && !path.is_dir() => path.extension().is_some(),
                _ => false,
            };
            if let Some(path) = target {
                if !single_file && !path.is_dir() {
                    std::fs::create_dir_all(path)
                        .with_context(|| format!("cannot create {}", path.display()))?;
                }
            }

            let mut written = Vec::new();
            for card in &chosen {
                let svg_text = cards::render_svg(*card, &board, &label);
                let bytes = if *svg {
                    svg_text.into_bytes()
                } else {
                    cards::render_png(&svg_text)?
                };

                let path = if single_file {
                    target.expect("single_file implies a target").to_path_buf()
                } else {
                    let directory = target.map_or_else(
                        || std::path::PathBuf::from("."),
                        std::path::Path::to_path_buf,
                    );
                    directory.join(cards::file_name(*card, &stamp, extension))
                };

                std::fs::write(&path, &bytes)
                    .with_context(|| format!("cannot write {}", path.display()))?;
                written.push(
                    std::fs::canonicalize(&path)
                        .unwrap_or(path)
                        .display()
                        .to_string(),
                );
            }

            output.emit(&json!({ "range": label, "files": written }), || {
                written.join("\n")
            })
        }

        UsageCommand::Refresh { progress } => {
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

            let script = directories.collector_script();
            collector::install_script(&script)?;
            let out_dir = directories.usage_dir();
            let show = *progress && output == Output::Text;

            let outcome = collector::refresh(&script, &out_dir, &directories.cache, |event| {
                if show {
                    match event {
                        veronica_usage::CollectorEvent::Phase {
                            name,
                            detail,
                            seconds,
                        } => eprintln!("  {name:<14} {detail}  ({seconds:.2}s)"),
                        veronica_usage::CollectorEvent::Note { message } => {
                            eprintln!("  … {message}");
                        }
                        veronica_usage::CollectorEvent::Error { message } => {
                            eprintln!("  ! {message}");
                        }
                        _ => {}
                    }
                }
            })
            .await?;

            let refreshed = Refreshed {
                completed: outcome.completed,
                generated_at: outcome.document.generated_at.clone(),
                sources: outcome.document.sources.clone(),
                totals: outcome.document.totals,
                days: outcome.document.daily.len(),
                errors: outcome.errors().into_iter().map(String::from).collect(),
            };

            output.emit(&refreshed, || {
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

fn bar(percent: f64) -> String {
    let filled = (1..=10_u8)
        .filter(|&step| percent / 10.0 >= f64::from(step) - 0.5)
        .count();
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
        assert_eq!(bar(140.0), "[##########]");
    }
}
