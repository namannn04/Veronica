use std::io::Read;

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use veronica_core::clipboard::{ClipboardHistory, DEFAULT_LIMIT};
use veronica_core::AppDirectories;

use crate::format::{self, Output};

#[derive(clap::Subcommand)]
pub enum ClipboardCommand {
    #[command(
        about = "Record a copy, reading the text from stdin",
        long_about = "Record a copy, reading the text from stdin.\n\nUsed by the shell extension. Silent and idempotent by design: re-copying something moves it to the front rather than adding a duplicate."
    )]
    Record {
        #[arg(long, default_value_t = DEFAULT_LIMIT, help = "Keep at most this many entries")]
        limit: usize,
        #[arg(
            long,
            help = "Keep the text byte for byte. Without it one trailing newline is dropped, since `echo text | vr clipboard record` adds one; the shell extension passes the copy exactly and uses this"
        )]
        exact: bool,
    },
    #[command(alias = "ls", about = "List the history, newest first")]
    List {
        #[arg(long, default_value = "", help = "Only entries containing this text")]
        query: String,
        #[arg(long, default_value_t = 20, help = "Show at most this many")]
        limit: usize,
    },
    #[command(about = "Print one entry's full text, for piping onward")]
    Get { id: u64 },
    #[command(alias = "rm", about = "Forget one entry")]
    Remove { id: u64 },
    #[command(about = "Forget everything")]
    Clear,
}

pub async fn run(
    directories: &AppDirectories,
    command: &ClipboardCommand,
    output: Output,
) -> Result<()> {
    let path = directories.clipboard_db();
    let mut history = ClipboardHistory::load(&path)?;

    match command {
        ClipboardCommand::Record { limit, exact } => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .context("cannot read the clipboard text from stdin")?;

            let text = if *exact {
                text.as_str()
            } else {
                text.strip_suffix('\n').unwrap_or(&text)
            };

            match history.record(text, Utc::now(), *limit) {
                Some(id) => {
                    history.save(&path)?;
                    output.emit(&json!({ "recorded": id }), String::new)
                }
                None => output.emit(&json!({ "recorded": null }), String::new),
            }
        }

        ClipboardCommand::List { query, limit } => {
            let matches: Vec<_> = history
                .search(query)
                .into_iter()
                .take(*limit)
                .cloned()
                .collect();

            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Row {
                id: u64,
                preview: String,
                lines: usize,
                bytes: usize,
                count: u32,
                last_seen: String,
            }
            let rows: Vec<Row> = matches
                .iter()
                .map(|entry| Row {
                    id: entry.id,
                    preview: entry.preview(),
                    lines: entry.line_count(),
                    bytes: entry.byte_len(),
                    count: entry.count,
                    last_seen: entry.last_seen.to_rfc3339(),
                })
                .collect();

            output.emit(&rows, || {
                if rows.is_empty() {
                    return "nothing in the clipboard history".to_string();
                }
                let table: Vec<Vec<String>> = rows
                    .iter()
                    .map(|row| {
                        vec![
                            row.id.to_string(),
                            row.preview.clone(),
                            format!("{}L", row.lines),
                            format!("x{}", row.count),
                        ]
                    })
                    .collect();
                format::table(&["id", "preview", "lines", "copied"], &table)
            })
        }

        ClipboardCommand::Get { id } => {
            let entry = history
                .get(*id)
                .with_context(|| format!("no clipboard entry {id}"))?;
            match output {
                Output::Json => {
                    println!("{}", serde_json::to_string_pretty(entry)?);
                }
                Output::Text => print!("{}", entry.text),
            }
            Ok(())
        }

        ClipboardCommand::Remove { id } => {
            if !history.remove(*id) {
                anyhow::bail!("no clipboard entry {id}");
            }
            history.save(&path)?;
            output.emit(&json!({ "removed": id }), || format!("removed {id}"))
        }

        ClipboardCommand::Clear => {
            let count = history.entries.len();
            history.clear();
            history.save(&path)?;
            output.emit(&json!({ "cleared": count }), || {
                format!("cleared {count} entries")
            })
        }
    }
}
