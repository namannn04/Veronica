//! `vr backup` — explicit local export/import, with no chosen cloud provider.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::json;
use veronica_core::{AppDirectories, BackupArchive};

use crate::format::Output;

#[derive(clap::Subcommand)]
pub enum BackupCommand {
    /// Export settings, persistent data and state to one checked archive.
    Export {
        /// Destination. Defaults to ./Veronica-backup-YYYY-MM-DD.veronica-backup.
        path: Option<PathBuf>,
    },
    /// Inspect an archive without changing anything.
    Inspect { path: PathBuf },
    /// Restore an archive after validating every file and digest.
    Import {
        path: PathBuf,
        /// Required acknowledgement that existing matching files are replaced.
        #[arg(long)]
        confirm: bool,
    },
}

pub fn run(
    directories: &AppDirectories,
    command: &BackupCommand,
    output: Output,
) -> Result<()> {
    match command {
        BackupCommand::Export { path } => {
            let now = chrono::Utc::now();
            let destination = path.clone().unwrap_or_else(|| {
                PathBuf::from(format!(
                    "Veronica-backup-{}.veronica-backup",
                    now.format("%Y-%m-%d")
                ))
            });
            let archive = BackupArchive::collect(directories, env!("CARGO_PKG_VERSION"), now)?;
            archive.save(&destination)?;
            let bytes = std::fs::metadata(&destination)?.len();
            output.emit(
                &json!({
                    "path": absolute(&destination)?,
                    "files": archive.manifest.files.len(),
                    "bytes": bytes,
                    "createdAt": now.to_rfc3339(),
                }),
                || format!(
                    "exported {} files to {}",
                    archive.manifest.files.len(),
                    destination.display()
                ),
            )
        }
        BackupCommand::Inspect { path } => {
            let archive = BackupArchive::load(path)?;
            let decoded_bytes: u64 = archive.manifest.files.iter().map(|file| file.bytes).sum();
            output.emit(
                &json!({
                    "path": absolute(path)?,
                    "formatVersion": archive.manifest.format_version,
                    "appVersion": archive.manifest.app_version,
                    "createdAt": archive.manifest.created_at.to_rfc3339(),
                    "files": archive.manifest.files.len(),
                    "decodedBytes": decoded_bytes,
                    "paths": archive.manifest.files.iter().map(|file| &file.path).collect::<Vec<_>>(),
                }),
                || format!(
                    "Veronica {} backup from {}\n{} files, {} decoded bytes",
                    archive.manifest.app_version,
                    archive.manifest.created_at.to_rfc3339(),
                    archive.manifest.files.len(),
                    decoded_bytes
                ),
            )
        }
        BackupCommand::Import { path, confirm } => {
            if !confirm {
                anyhow::bail!(
                    "import replaces matching settings and data; rerun with --confirm after inspecting it"
                );
            }
            let archive = BackupArchive::load(path)?;
            let report = archive.restore(directories)?;
            output.emit(&report, || {
                format!(
                    "restored {} files ({} bytes) from Veronica {}",
                    report.files_restored, report.bytes_restored, report.source_version
                )
            })
        }
    }
}

fn absolute(path: &Path) -> Result<String> {
    if path.is_absolute() {
        return Ok(path.display().to_string());
    }
    Ok(std::env::current_dir()
        .context("cannot resolve the current directory")?
        .join(path)
        .display()
        .to_string())
}
