//! `vr clipboard` — the clipboard history.
//!
//! `record` is what the GNOME Shell extension calls: only the compositor can
//! watch the selection on Wayland, so the extension does the watching and pipes
//! each new entry here on stdin.

use std::io::Read;

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use veronica_core::clipboard::{ClipboardHistory, DEFAULT_LIMIT};
use veronica_core::AppDirectories;

use crate::format::{self, Output};

#[derive(clap::Subcommand)]
pub enum ClipboardCommand {
    /// Record a copy, reading the text from stdin.
    ///
    /// Used by the shell extension. Silent and idempotent by design: re-copying
    /// something moves it to the front rather than adding a duplicate.
    Record {
        /// Keep at most this many entries.
        #[arg(long, default_value_t = DEFAULT_LIMIT)]
        limit: usize,
    },
    /// List the history, newest first.
    #[command(alias = "ls")]
    List {
        /// Only entries containing this text.
        #[arg(long, default_value = "")]
        query: String,
        /// Show at most this many.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Print one entry's full text, for piping onward.
    Get { id: u64 },
    /// Forget one entry.
    #[command(alias = "rm")]
    Remove { id: u64 },
    /// Forget everything.
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
        ClipboardCommand::Record { limit } => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .context("cannot read the clipboard text from stdin")?;

            // A trailing newline is an artefact of piping, not part of the copy.
            let text = text.strip_suffix('\n').unwrap_or(&text);

            match history.record(text, Utc::now(), *limit) {
                Some(id) => {
                    history.save(&path)?;
                    output.emit(&json!({ "recorded": id }), || String::new())
                }
                // Blank or oversized: not an error, just nothing to keep.
                None => output.emit(&json!({ "recorded": null }), || String::new()),
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
            // Printed raw so it can be piped; no trailing formatting.
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
