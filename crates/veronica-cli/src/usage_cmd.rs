//! `vr usage` — the numbers the dashboard rings and charts show.

use anyhow::{Context, Result};
use clap::Subcommand;
use veronica_core::AppDirectories;
use veronica_usage::aggregate::{self, DayRange, SourceSelection};
use veronica_usage::collector;

use crate::format::{self, money, tokens, Output};

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
