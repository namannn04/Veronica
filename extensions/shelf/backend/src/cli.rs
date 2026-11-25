use std::path::PathBuf;

use anyhow::Result;
use serde_json::json;
use veronica_core::file_shelf::FileShelf;
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::settings::Settings;
use veronica_sdk::Output;

#[derive(clap::Subcommand)]
pub(crate) enum ShelfCommand {
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

pub(crate) fn open() -> Result<FileShelf> {
    let directories = AppDirectories::current()?;
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
    if let Err(error) = shelf.import_legacy(&legacy) {
        eprintln!("some legacy shelf entries could not be imported: {error:#}");
    }
    Ok(shelf)
}

pub(crate) fn run(command: &ShelfCommand, output: Output<'_>) -> Result<()> {
    let shelf = open()?;
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
