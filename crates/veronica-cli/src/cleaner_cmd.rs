//! `vr cleaner` — the disk cleaner.
//!
//! Edith's `ed cleaner`, with Ubuntu's paths. Two things carry over exactly and
//! are worth repeating on every screen that shows this: `clean` has no notion
//! of a selection, so it moves everything the same scan found rather than the
//! subset an interface would have ticked; and moving to the Trash does not free
//! the space until the Trash is emptied.

use anyhow::{Context, Result};
use serde_json::json;
use veronica_core::cleaner::{self, Family};

use crate::format::{self, Output};

#[derive(clap::Subcommand)]
pub enum CleanerCommand {
    /// Measure what could be reclaimed. Reads only.
    Scan {
        /// Also sweep this folder for project directories such as
        /// `node_modules`. Without it, only the fixed caches are measured.
        #[arg(long)]
        root: Option<String>,
        /// Restrict to these category ids. Repeatable.
        #[arg(long = "category")]
        categories: Vec<String>,
    },
    /// List the categories the cleaner knows, and what each one costs.
    #[command(alias = "ls")]
    Categories,
    /// Re-scan, then move everything found to the Trash.
    ///
    /// Does nothing without `--yes`.
    Clean {
        #[arg(long)]
        root: Option<String>,
        #[arg(long = "category")]
        categories: Vec<String>,
        /// Required. Without it the command reports what it would move.
        #[arg(long)]
        yes: bool,
    },
    /// The mounted volumes and how full they are.
    Drives,
}

fn home() -> Result<std::path::PathBuf> {
    veronica_core::paths::home_dir().context("cannot resolve the home directory")
}

/// Validate the ids, and split them into the two families, because a project
/// category cannot turn up without a `--root` and saying so beats an empty scan.
fn check_categories(categories: &[String], root: Option<&String>) -> Result<Option<Vec<String>>> {
    if categories.is_empty() {
        return Ok(None);
    }
    for id in categories {
        let entry = cleaner::category(id).with_context(|| {
            let known: Vec<&str> = cleaner::CATEGORIES.iter().map(|e| e.id).collect();
            format!("unknown category '{id}'; try one of {}", known.join(", "))
        })?;
        if entry.family == Family::Project && root.is_none() {
            anyhow::bail!("'{id}' only matches directories inside a folder you sweep; pass --root");
        }
    }
    Ok(Some(categories.to_vec()))
}

/// Both families in one scan, so a command with `--root` reports one total.
fn scan(
    root: Option<&String>,
    selected: Option<&Vec<String>>,
) -> Result<veronica_core::CleanerScan> {
    let home = home()?;
    let ids = selected.map(Vec::as_slice);
    let mut caches = cleaner::scan_caches(&home, ids);
    if let Some(root) = root {
        let projects = cleaner::scan_projects(std::path::Path::new(root), ids)?;
        caches.items.extend(projects.items);
        for (id, bytes) in projects.totals {
            *caches.totals.entry(id).or_default() += bytes;
        }
        caches.total_bytes = caches.total_bytes.saturating_add(projects.total_bytes);
        caches.items.sort_by(|left, right| {
            right
                .bytes
                .cmp(&left.bytes)
                .then(left.path.cmp(&right.path))
        });
    }
    Ok(caches)
}

pub fn run(command: &CleanerCommand, output: Output) -> Result<()> {
    match command {
        CleanerCommand::Categories => output.emit(&cleaner::CATEGORIES, || {
            let rows: Vec<Vec<String>> = cleaner::CATEGORIES
                .iter()
                .map(|entry| {
                    vec![
                        entry.id.to_string(),
                        match entry.family {
                            Family::Cache => "cache",
                            Family::Project => "project",
                        }
                        .to_string(),
                        if entry.on_by_default { "on" } else { "off" }.to_string(),
                        entry.title.to_string(),
                    ]
                })
                .collect();
            format!(
                "{}\n\nA project category only turns up inside a folder you sweep with --root.",
                format::table(&["id", "family", "default", "what"], &rows)
            )
        }),

        CleanerCommand::Scan { root, categories } => {
            let selected = check_categories(categories, root.as_ref())?;
            let found = scan(root.as_ref(), selected.as_ref())?;
            output.emit(&found, || summary(&found, false))
        }

        CleanerCommand::Clean {
            root,
            categories,
            yes,
        } => {
            let selected = check_categories(categories, root.as_ref())?;
            let found = scan(root.as_ref(), selected.as_ref())?;
            if !yes {
                return output.emit(
                    &json!({ "wouldTrash": found.items, "totalBytes": found.total_bytes }),
                    || summary(&found, true),
                );
            }
            let report = cleaner::clean(&home()?, &found);
            output.emit(&report, || {
                use std::fmt::Write;
                let mut out = format!(
                    "moved {} item{} to the Trash, {}",
                    report.trashed.len(),
                    if report.trashed.len() == 1 { "" } else { "s" },
                    bytes(report.bytes_reclaimed)
                );
                for (path, error) in &report.failed {
                    let _ = write!(out, "\ncould not move {path}: {error}");
                }
                let _ = write!(
                    out,
                    "\n\nThe Trash keeps occupying the disk until you empty it."
                );
                out
            })
        }

        CleanerCommand::Drives => {
            let snapshot = veronica_system::MetricsSampler::new().sample();
            output.emit(&snapshot.disks, || {
                let rows: Vec<Vec<String>> = snapshot
                    .disks
                    .iter()
                    .map(|disk| {
                        vec![
                            disk.mount_point.clone(),
                            disk.file_system.clone(),
                            bytes(disk.total_bytes),
                            bytes(disk.available_bytes),
                            format!("{:.0}%", disk.used_percent()),
                            if disk.removable { "removable" } else { "" }.to_string(),
                        ]
                    })
                    .collect();
                format::table(&["mount", "fs", "size", "free", "used", ""], &rows)
            })
        }
    }
}

fn summary(scan: &veronica_core::CleanerScan, dry_run: bool) -> String {
    use std::fmt::Write;
    if scan.items.is_empty() {
        return "nothing to reclaim".to_string();
    }
    let rows: Vec<Vec<String>> = scan
        .items
        .iter()
        .map(|item| vec![bytes(item.bytes), item.category.clone(), item.path.clone()])
        .collect();
    let mut out = format::table(&["size", "category", "path"], &rows);
    let _ = write!(out, "\n\n{} in total", bytes(scan.total_bytes));
    if dry_run {
        let _ = write!(
            out,
            "\n\nNothing was moved. Rerun with --yes, which moves every item above \
             to the Trash — the Trash then keeps occupying the disk until you empty it."
        );
    }
    out
}

fn bytes(value: u64) -> String {
    veronica_system::metrics::human_bytes(value)
}
