use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use veronica_core::companion::{CompanionKind, CompanionRepository};
use veronica_core::AppDirectories;

use crate::format::{self, Output};

#[derive(clap::Subcommand)]
pub enum CompanionCommand {
    #[command(alias = "ls", about = "Everything, newest first, optionally filtered")]
    List {
        #[arg(default_value = "", help = "Match the title or the body")]
        query: String,
        #[arg(long, help = "Only notes, or only voice memos")]
        kind: Option<String>,
    },
    #[command(about = "Print one item in full, for piping onward")]
    Show { id: u64 },
    #[command(about = "Write a note. The body is read from stdin when it is not given")]
    Add {
        title: String,
        #[arg(help = "The note's text. Omit it to read stdin, so a file can be piped in")]
        body: Option<String>,
        #[arg(long)]
        pinned: bool,
    },
    #[command(about = "Pin or unpin an item, so it sorts to the top")]
    Pin {
        id: u64,
        #[arg(long, help = "Unpin instead")]
        off: bool,
    },
    #[command(
        alias = "rm",
        about = "Delete an item, and its recording if it has one"
    )]
    Remove { id: u64 },
}

fn parse_kind(raw: &str) -> Result<CompanionKind> {
    match raw.to_lowercase().as_str() {
        "note" | "notes" => Ok(CompanionKind::Note),
        "voice" | "memo" | "memos" => Ok(CompanionKind::Voice),
        _ => anyhow::bail!("unknown kind '{raw}'; try note or voice"),
    }
}

fn kind_label(kind: CompanionKind) -> &'static str {
    match kind {
        CompanionKind::Note => "note",
        CompanionKind::Voice => "voice",
    }
}

pub fn run(directories: &AppDirectories, command: &CompanionCommand, output: Output) -> Result<()> {
    let repository = CompanionRepository::new(directories.companion_dir());

    match command {
        CompanionCommand::List { query, kind } => {
            let wanted = kind.as_deref().map(parse_kind).transpose()?;
            let items: Vec<_> = repository
                .list(query)?
                .into_iter()
                .filter(|item| wanted.is_none_or(|kind| item.kind == kind))
                .collect();
            output.emit(&items, || {
                if items.is_empty() {
                    return if query.is_empty() {
                        "nothing saved yet".to_string()
                    } else {
                        format!("nothing matches {query:?}")
                    };
                }
                let rows: Vec<Vec<String>> = items
                    .iter()
                    .map(|item| {
                        vec![
                            item.id.to_string(),
                            if item.pinned { "◆" } else { "" }.to_string(),
                            kind_label(item.kind).to_string(),
                            item.title.clone(),
                            item.updated_at.format("%Y-%m-%d %H:%M").to_string(),
                        ]
                    })
                    .collect();
                format::table(&["id", "", "kind", "title", "updated"], &rows)
            })
        }

        CompanionCommand::Show { id } => {
            let item = repository
                .list("")?
                .into_iter()
                .find(|item| item.id == *id)
                .with_context(|| format!("no Companion item {id}"))?;
            output.emit(&item, || {
                use std::fmt::Write;
                let mut out = String::new();
                let _ = writeln!(out, "{}", item.title);
                let _ = writeln!(
                    out,
                    "{} · updated {}{}",
                    kind_label(item.kind),
                    item.updated_at.format("%Y-%m-%d %H:%M"),
                    if item.pinned { " · pinned" } else { "" }
                );
                if let Some(path) = &item.audio_path {
                    let _ = writeln!(out, "recording  {path}");
                }
                if !item.body.is_empty() {
                    let _ = write!(out, "\n{}", item.body);
                }
                out
            })
        }

        CompanionCommand::Add {
            title,
            body,
            pinned,
        } => {
            let text = match body {
                Some(body) => body.clone(),
                None => {
                    let mut buffer = String::new();
                    std::io::Read::read_to_string(&mut std::io::stdin(), &mut buffer)
                        .context("cannot read the note body from stdin")?;
                    buffer
                }
            };
            let now = Utc::now();
            let item = repository.create_note(title, text.trim_end(), now)?;
            let item = if *pinned {
                repository.update(item.id, &item.title, &item.body, true, now)?
            } else {
                item
            };
            output.emit(&item, || format!("saved note {} — {}", item.id, item.title))
        }

        CompanionCommand::Pin { id, off } => {
            let item = repository
                .list("")?
                .into_iter()
                .find(|item| item.id == *id)
                .with_context(|| format!("no Companion item {id}"))?;
            let updated = repository.update(item.id, &item.title, &item.body, !*off, Utc::now())?;
            output.emit(&updated, || {
                format!(
                    "{} {}",
                    if updated.pinned { "pinned" } else { "unpinned" },
                    updated.title
                )
            })
        }

        CompanionCommand::Remove { id } => {
            repository.remove(*id)?;
            output.emit(&json!({ "removed": id }), || format!("removed {id}"))
        }
    }
}
