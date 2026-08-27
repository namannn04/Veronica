//! Portable, local backup archives for Veronica.
//!
//! Edith can put its settings in iCloud. Veronica cannot silently choose a
//! Linux cloud provider, so it provides an explicit export/import instead. A
//! `.veronica-backup` file is JSON containing a versioned manifest and base64
//! file bodies. It includes configuration, persistent data and state; cache and
//! runtime files are deliberately excluded because they are reproducible or
//! process-local.
//!
//! Imports are defensive: every path, byte count and SHA-256 digest is checked
//! before the first destination file is touched. Files are then written through
//! siblings and atomically renamed, so a crash cannot leave a truncated file.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::AppDirectories;

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub format_version: u32,
    pub app_version: String,
    pub created_at: DateTime<Utc>,
    pub files: Vec<BackupFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    /// `configuration/settings.json`, `data/clipboard.json`, etc.
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub contents: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupArchive {
    pub manifest: BackupManifest,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub files_restored: usize,
    pub bytes_restored: u64,
    pub source_version: String,
    pub created_at: DateTime<Utc>,
}

impl BackupArchive {
    pub fn collect(dirs: &AppDirectories, app_version: &str, now: DateTime<Utc>) -> Result<Self> {
        let roots = [
            ("configuration", &dirs.configuration),
            ("data", &dirs.data),
            ("state", &dirs.state),
        ];
        let mut files = Vec::new();
        let mut total = 0u64;

        for (label, root) in roots {
            if !root.exists() {
                continue;
            }
            for entry in WalkDir::new(root).follow_links(false) {
                let entry = entry.with_context(|| format!("cannot walk {}", root.display()))?;
                let metadata = entry.metadata()?;
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    continue;
                }
                if metadata.len() > MAX_FILE_BYTES {
                    bail!("{} is too large to back up", entry.path().display());
                }
                total = total
                    .checked_add(metadata.len())
                    .context("backup size overflow")?;
                if total > MAX_ARCHIVE_BYTES {
                    bail!("persistent Veronica data exceeds the 1 GiB backup limit");
                }
                let relative = entry.path().strip_prefix(root)?;
                let path = format!("{label}/{}", portable(relative)?);
                let body = std::fs::read(entry.path())?;
                files.push(BackupFile {
                    path,
                    bytes: body.len() as u64,
                    sha256: digest(&body),
                    contents: STANDARD.encode(body),
                });
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Self {
            manifest: BackupManifest {
                format_version: FORMAT_VERSION,
                app_version: app_version.to_string(),
                created_at: now,
                files,
            },
        })
    }

    pub fn save(&self, destination: &Path) -> Result<()> {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = destination.with_extension("veronica-backup.tmp");
        std::fs::write(&temporary, serde_json::to_vec(self)?)?;
        std::fs::rename(&temporary, destination)
            .with_context(|| format!("cannot replace {}", destination.display()))?;
        Ok(())
    }

    pub fn load(source: &Path) -> Result<Self> {
        let metadata = std::fs::metadata(source)
            .with_context(|| format!("cannot read {}", source.display()))?;
        // Base64 and JSON add overhead; cap the encoded envelope too.
        if metadata.len() > MAX_ARCHIVE_BYTES + MAX_ARCHIVE_BYTES / 2 {
            bail!("backup archive is too large");
        }
        serde_json::from_slice(&std::fs::read(source)?)
            .with_context(|| format!("{} is not a Veronica backup", source.display()))
    }

    pub fn restore(&self, dirs: &AppDirectories) -> Result<ImportReport> {
        if self.manifest.format_version != FORMAT_VERSION {
            bail!(
                "backup format {} is unsupported; this build reads format {}",
                self.manifest.format_version,
                FORMAT_VERSION
            );
        }

        // Validate and decode the complete archive before touching live data.
        let mut seen = BTreeSet::new();
        let mut decoded = Vec::new();
        let mut total = 0u64;
        for file in &self.manifest.files {
            if !seen.insert(file.path.clone()) {
                bail!("backup contains duplicate path {}", file.path);
            }
            let destination = destination_for(dirs, &file.path)?;
            let body = STANDARD
                .decode(&file.contents)
                .with_context(|| format!("{} has invalid base64", file.path))?;
            if body.len() as u64 != file.bytes {
                bail!("{} has the wrong byte count", file.path);
            }
            if digest(&body) != file.sha256 {
                bail!("{} failed its SHA-256 check", file.path);
            }
            if file.bytes > MAX_FILE_BYTES {
                bail!("{} exceeds the per-file restore limit", file.path);
            }
            total = total.checked_add(file.bytes).context("restore size overflow")?;
            if total > MAX_ARCHIVE_BYTES {
                bail!("backup expands beyond the restore limit");
            }
            decoded.push((destination, body));
        }

        for (index, (destination, body)) in decoded.iter().enumerate() {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let temporary = destination.with_extension(format!("restore-{index}.tmp"));
            std::fs::write(&temporary, body)?;
            std::fs::rename(&temporary, destination)
                .with_context(|| format!("cannot restore {}", destination.display()))?;
        }

        Ok(ImportReport {
            files_restored: decoded.len(),
            bytes_restored: total,
            source_version: self.manifest.app_version.clone(),
            created_at: self.manifest.created_at,
        })
    }
}

fn destination_for(dirs: &AppDirectories, archived: &str) -> Result<PathBuf> {
    let path = Path::new(archived);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("unsafe backup path {archived}");
    }
    let mut components = path.components();
    let root = components
        .next()
        .and_then(|part| match part {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .context("backup path has no root")?;
    let base = match root {
        "configuration" => &dirs.configuration,
        "data" => &dirs.data,
        "state" => &dirs.state,
        _ => bail!("unknown backup root {root}"),
    };
    let mut destination = base.clone();
    for component in components {
        if let Component::Normal(value) = component {
            destination.push(value);
        }
    }
    if destination == *base {
        bail!("backup path names a directory rather than a file");
    }
    Ok(destination)
}

fn portable(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(
                value
                    .to_str()
                    .context("backup path is not valid UTF-8")?
                    .to_string(),
            ),
            _ => bail!("cannot archive unsafe path {}", path.display()),
        }
    }
    Ok(parts.join("/"))
}

fn digest(body: &[u8]) -> String {
    format!("{:x}", Sha256::digest(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::EnvOverrides;

    fn dirs(root: &Path) -> AppDirectories {
        AppDirectories::with_env(
            root,
            EnvOverrides {
                config_home: Some(root.join("config")),
                data_home: Some(root.join("data")),
                state_home: Some(root.join("state")),
                cache_home: Some(root.join("cache")),
                runtime_dir: Some(root.join("runtime")),
            },
        )
    }

    fn sandbox(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "veronica-backup-{name}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn export_and_restore_round_trip_every_persistent_root() {
        let source_root = sandbox("roundtrip-source");
        let source = dirs(&source_root);
        source.prepare().unwrap();
        std::fs::write(source.settings_file(), b"{\"appearance\":\"dark\"}").unwrap();
        std::fs::create_dir_all(source.data.join("usage")).unwrap();
        std::fs::write(source.data.join("usage/usage.json"), b"usage").unwrap();
        std::fs::write(source.state.join("alerts.json"), b"alerts").unwrap();
        std::fs::write(source.cache.join("throw-away"), b"cache").unwrap();

        let archive = BackupArchive::collect(&source, "0.1.8", Utc::now()).unwrap();
        assert_eq!(archive.manifest.files.len(), 3, "cache must be excluded");

        let target_root = sandbox("roundtrip-target");
        let target = dirs(&target_root);
        let report = archive.restore(&target).unwrap();
        assert_eq!(report.files_restored, 3);
        assert_eq!(std::fs::read(target.settings_file()).unwrap(), b"{\"appearance\":\"dark\"}");
        assert_eq!(std::fs::read(target.data.join("usage/usage.json")).unwrap(), b"usage");
        assert_eq!(std::fs::read(target.state.join("alerts.json")).unwrap(), b"alerts");
        assert!(!target.cache.join("throw-away").exists());
    }

    #[test]
    fn tampering_is_found_before_any_destination_is_written() {
        let root = sandbox("tamper");
        let source = dirs(&root);
        source.prepare().unwrap();
        std::fs::write(source.settings_file(), b"good").unwrap();
        let mut archive = BackupArchive::collect(&source, "test", Utc::now()).unwrap();
        archive.manifest.files[0].contents = STANDARD.encode(b"evil");

        let target_root = sandbox("tamper-target");
        let target = dirs(&target_root);
        assert!(archive.restore(&target).is_err());
        assert!(!target.settings_file().exists());
    }

    #[test]
    fn traversal_and_unknown_roots_are_rejected() {
        let root = sandbox("paths");
        let dirs = dirs(&root);
        for path in ["../escape", "data/../../escape", "/absolute", "cache/secret"] {
            assert!(destination_for(&dirs, path).is_err(), "accepted {path}");
        }
        assert_eq!(
            destination_for(&dirs, "data/nested/file").unwrap(),
            dirs.data.join("nested/file")
        );
    }

    #[test]
    fn duplicate_paths_are_rejected() {
        let root = sandbox("duplicate");
        let dirs = dirs(&root);
        let body = b"same";
        let file = BackupFile {
            path: "data/file".into(),
            bytes: body.len() as u64,
            sha256: digest(body),
            contents: STANDARD.encode(body),
        };
        let archive = BackupArchive {
            manifest: BackupManifest {
                format_version: FORMAT_VERSION,
                app_version: "test".into(),
                created_at: Utc::now(),
                files: vec![file.clone(), file],
            },
        };
        assert!(archive.restore(&dirs).is_err());
    }

    #[test]
    fn archive_file_round_trips() {
        let root = sandbox("file");
        let dirs = dirs(&root);
        dirs.prepare().unwrap();
        std::fs::write(dirs.settings_file(), b"settings").unwrap();
        let archive = BackupArchive::collect(&dirs, "test", Utc::now()).unwrap();
        let path = root.join("backup.veronica-backup");
        archive.save(&path).unwrap();
        let loaded = BackupArchive::load(&path).unwrap();
        assert_eq!(loaded.manifest.files[0].path, "configuration/settings.json");
    }
}
