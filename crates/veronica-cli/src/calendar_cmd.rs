//! `vr calendar` — the agenda, from every configured calendar.

use anyhow::{Context, Result};
use chrono::Local;
use clap::Subcommand;
use veronica_calendar::{agenda, server};

use crate::format::{self, Output};

#[derive(Subcommand)]
pub enum CalendarCommand {
    /// The agenda, grouped by day.
    #[command(alias = "agenda")]
    List {
        /// How many days ahead to include.
        #[arg(long, default_value_t = 7)]
        days: i64,
        /// Include events that have already finished today.
        #[arg(long)]
        all: bool,
    },
    /// The next event that has not started yet.
    Next,
}

/// A time, or "all day" for an event with no time slot.
fn slot(event: &veronica_calendar::Event) -> String {
    if event.all_day {
        return "all day".to_string();
    }
    format!(
        "{}–{}",
        event.start.format("%H:%M"),
        event.end.format("%H:%M")
    )
}

pub async fn run(command: &CalendarCommand, output: Output) -> Result<()> {
    let connection = zbus::Connection::session()
        .await
        .context("cannot reach the session bus; is this a desktop session?")?;

    match command {
        CalendarCommand::List { days, all } => {
            let has_calendars = server::has_calendars(&connection).await.unwrap_or(false);
            let events = server::events_with_links(&connection, *days).await?;
            let now = Local::now();
            let events = if *all {
                events
            } else {
                agenda::upcoming(&events, now)
            };
            let agenda = agenda::group_by_day(&events, now);

            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Report {
                has_calendars: bool,
                days: Vec<veronica_calendar::AgendaDay>,
            }
            let report = Report {
                has_calendars,
                days: agenda,
            };

            output.emit(&report, || {
                if !report.has_calendars {
                    return "no calendars are configured".to_string();
                }
                if report.days.is_empty() {
                    return format!("nothing scheduled in the next {days} days");
                }
                use std::fmt::Write;
                let mut out = String::new();
                for day in &report.days {
                    let _ = writeln!(out, "{}  {}", day.label, day.date);
                    let rows: Vec<Vec<String>> = day
                        .events
                        .iter()
                        .map(|event| {
                            vec![
                                slot(event),
                                event.summary.clone(),
                                event.join_url.clone().unwrap_or_default(),
                            ]
                        })
                        .collect();
                    for line in format::table(&["when", "event", "join"], &rows).lines().skip(1) {
                        let _ = writeln!(out, "  {line}");
                    }
                }
                out.trim_end().to_string()
            })
        }

        CalendarCommand::Next => {
            let events = server::events(&connection, 14).await?;
            let now = Local::now();
            let next = agenda::next_up(&events, now).cloned();
            output.emit(&next, || {
                let Some(event) = &next else {
                    return "nothing coming up".to_string();
                };
                let minutes = (event.start - now).num_minutes().max(0);
                format!(
                    "{} at {} (in {})",
                    event.summary,
                    event.start.format("%H:%M"),
                    format::countdown(minutes * 60)
                )
            })
        }
    }
}
