use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use veronica_core::fsutil::write_atomic_with_mode;

pub(crate) const ATTEMPTS: usize = 5;

const SYMLINK_LIMIT: usize = 40;

const NEW_FILE_MODE: u32 = 0o600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Edit {
    Keep,
    Write { body: String, replaced: bool },
    Refuse(String),
}

#[derive(Debug)]
pub(crate) struct Applied {
    pub(crate) edit: Edit,
    pub(crate) backup: Option<PathBuf>,
}

pub(crate) fn apply(
    path: &Path,
    dry_run: bool,
    edit: &dyn Fn(Option<&str>) -> Edit,
) -> Result<Applied> {
    apply_with(path, dry_run, edit, &mut |_| {})
}

pub(crate) fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
    }
}

fn apply_with(
    path: &Path,
    dry_run: bool,
    edit: &dyn Fn(Option<&str>) -> Edit,
    before_commit: &mut dyn FnMut(&Path),
) -> Result<Applied> {
    let target = resolve_target(path)?;
    for _ in 0..ATTEMPTS {
        let original = read_optional(&target)?;
        let planned = edit(original.as_deref());
        let Edit::Write { body, replaced } = planned else {
            return Ok(Applied {
                edit: planned,
                backup: None,
            });
        };
        if dry_run {
            return Ok(Applied {
                edit: Edit::Write { body, replaced },
                backup: None,
            });
        }
        before_commit(&target);
        if read_optional(&target)? != original {
            continue;
        }
        let mode = match fs::metadata(&target) {
            Ok(metadata) => metadata.permissions().mode() & 0o7777,
            Err(error) if error.kind() == ErrorKind::NotFound => NEW_FILE_MODE,
            Err(error) => {
                return Err(error).with_context(|| format!("cannot inspect {}", target.display()));
            }
        };
        let backup = original
            .as_deref()
            .map(|text| write_backup(&target, text, mode))
            .transpose()?;
        write_atomic_with_mode(&target, body.as_bytes(), mode)?;
        return Ok(Applied {
            edit: Edit::Write { body, replaced },
            backup,
        });
    }
    bail!(
        "{} kept changing while it was being updated; try again",
        target.display()
    )
}

fn resolve_target(path: &Path) -> Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..SYMLINK_LIMIT {
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let link = fs::read_link(&current)
                    .with_context(|| format!("cannot follow {}", current.display()))?;
                current = match current.parent() {
                    Some(parent) if link.is_relative() => parent.join(link),
                    _ => link,
                };
            }
            _ => return Ok(current),
        }
    }
    bail!("too many symbolic links behind {}", path.display())
}

fn write_backup(target: &Path, text: &str, mode: u32) -> Result<PathBuf> {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    for counter in 0..100 {
        let suffix = if counter == 0 {
            String::new()
        } else {
            format!("-{counter}")
        };
        let backup = target.with_file_name(format!("{name}.veronica-backup-{stamp}{suffix}"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&backup)
        {
            Ok(mut file) => {
                file.write_all(text.as_bytes())?;
                file.sync_all()?;
                return Ok(backup);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(error).with_context(|| format!("cannot create {}", backup.display()));
            }
        }
    }
    bail!("cannot choose a backup name beside {}", target.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn append_marker(text: Option<&str>) -> Edit {
        Edit::Write {
            body: format!("{}+", text.unwrap_or_default()),
            replaced: false,
        }
    }

    #[test]
    fn a_concurrent_change_is_merged_on_the_next_attempt() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config");
        fs::write(&path, "a").unwrap();
        let mut calls = 0;
        let applied = apply_with(&path, false, &append_marker, &mut |target| {
            calls += 1;
            if calls < 3 {
                fs::write(target, format!("a{calls}")).unwrap();
            }
        })
        .unwrap();
        assert_eq!(calls, 3);
        assert_eq!(fs::read_to_string(&path).unwrap(), "a2+");
        let backup = applied.backup.unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), "a2");
    }

    #[test]
    fn a_file_that_never_settles_is_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config");
        fs::write(&path, "0").unwrap();
        let mut calls = 0;
        let error = apply_with(&path, false, &append_marker, &mut |target| {
            calls += 1;
            fs::write(target, calls.to_string()).unwrap();
        })
        .unwrap_err();
        assert_eq!(calls, ATTEMPTS);
        assert!(error.to_string().contains("kept changing"));
        assert_eq!(fs::read_to_string(&path).unwrap(), ATTEMPTS.to_string());
        let entries = fs::read_dir(root.path()).unwrap().count();
        assert_eq!(entries, 1);
    }

    #[test]
    fn backups_never_overwrite_each_other() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("config");
        let first = write_backup(&target, "one", 0o600).unwrap();
        let second = write_backup(&target, "two", 0o600).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read_to_string(first).unwrap(), "one");
        assert_eq!(fs::read_to_string(second).unwrap(), "two");
    }
}
