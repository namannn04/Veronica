//! The same owned-copy shelf used by the native drop surface and notch tab.
use crate::format::Output;
use anyhow::Result;
use serde_json::json;
use std::path::PathBuf;
use veronica_core::{file_shelf::FileShelf, AppDirectories, Settings};

#[derive(clap::Subcommand)]
pub enum ShelfCommand {
    List,
    Add {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    Remove {
        id: String,
    },
    Clear,
}

pub fn run(directories: &AppDirectories, command: &ShelfCommand, output: Output) -> Result<()> {
    let shelf = FileShelf::new(directories.data.join("file-shelf"));
    let settings = Settings::load(&directories.settings_file())?;
    let legacy: Vec<PathBuf> = settings
        .get("notchShelfFiles")
        .and_then(|value| value.as_array())
        .map(|paths| {
            paths
                .iter()
                .filter_map(|value| value.as_str().map(PathBuf::from))
                .collect()
        })
        .unwrap_or_default();
    // A bad old path must not make already-owned entries inaccessible.
    if let Err(error) = shelf.import_legacy(&legacy) {
        tracing::warn!("some legacy shelf entries could not be imported: {error:#}");
    }
    match command {
        ShelfCommand::List => {}
        ShelfCommand::Add { paths } => {
            for path in paths {
                shelf.import(path)?;
            }
        }
        ShelfCommand::Remove { id } => shelf.remove(id)?,
        ShelfCommand::Clear => {
            for entry in shelf.entries()? {
                shelf.remove(&entry.id)?;
            }
        }
    }
    let entries = shelf.entries()?;
    output.emit(&json!({"entries": entries}), || {
        entries
            .iter()
            .map(|entry| format!("{}  {}", entry.id, entry.name))
            .collect::<Vec<_>>()
            .join("\n")
    })
}
