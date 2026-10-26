//! Writing shared files safely.
//!
//! The app, `vr` and the shell extension (through `vr`) all write the same
//! small JSON files. Each writer therefore needs a temporary file of its own:
//! with one fixed `name.json.tmp`, two concurrent saves write into the same
//! file, and one rename can publish the other's half-written body, after which
//! every reader fails to parse it.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A sibling temporary path unique to this process and this call.
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

/// Replace `path` with `body` atomically: readers see the old file or the new
/// one, never a mixture, and a crash cannot leave it truncated.
pub fn write_atomic(path: &Path, body: &[u8]) -> Result<()> {
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
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name() != "settings.json")
            .collect();
        assert!(leftovers.is_empty(), "temporary files left behind");
    }

    #[test]
    fn temporary_names_differ_between_calls() {
        let path = Path::new("/x/settings.json");
        assert_ne!(temporary_for(path), temporary_for(path));
        assert_eq!(temporary_for(path).parent(), path.parent());
    }
}
