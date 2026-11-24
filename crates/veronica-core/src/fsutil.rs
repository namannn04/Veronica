use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn temporary_for(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

pub fn write_atomic(path: &Path, body: &[u8]) -> Result<()> {
    replace(path, body, None)
}

pub fn write_atomic_with_mode(path: &Path, body: &[u8], mode: u32) -> Result<()> {
    replace(path, body, Some(mode))
}

fn replace(path: &Path, body: &[u8], mode: Option<u32>) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let temporary = temporary_for(path);
    let written = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .with_context(|| format!("cannot create {}", temporary.display()))?;
        file.write_all(body)?;
        if let Some(mode) = mode {
            file.set_permissions(fs::Permissions::from_mode(mode))?;
        }
        file.sync_all()?;
        fs::rename(&temporary, path).with_context(|| format!("cannot replace {}", path.display()))
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_writers_never_publish_a_torn_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("settings.json");
        let bodies: Vec<String> = (0..8)
            .map(|i| format!("{{\"writer\":{i},\"pad\":\"{}\"}}", "x".repeat(64 * 1024)))
            .collect();
        std::thread::scope(|scope| {
            for body in &bodies {
                let path = path.clone();
                scope.spawn(move || {
                    for _ in 0..20 {
                        write_atomic(&path, body.as_bytes()).unwrap();
                        let read = fs::read(&path).unwrap();
                        serde_json::from_slice::<serde_json::Value>(&read)
                            .expect("a reader saw a torn file");
                    }
                });
            }
        });
        let leftovers: Vec<_> = fs::read_dir(root.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_name() != "settings.json")
            .collect();
        assert!(leftovers.is_empty(), "temporary files left behind");
    }

    #[test]
    fn mode_is_applied_before_the_file_is_published() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("secret.json");
        write_atomic_with_mode(&path, b"{}", 0o600).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn temporary_names_differ_between_calls() {
        let path = Path::new("/x/settings.json");
        assert_ne!(temporary_for(path), temporary_for(path));
        assert_eq!(temporary_for(path).parent(), path.parent());
    }
}
