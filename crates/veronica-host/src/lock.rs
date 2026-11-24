use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

use anyhow::{Context, Result};
use rustix::fs::{flock, FlockOperation};

#[derive(Debug)]
pub struct ExtensionsLock {
    _file: File,
}

fn open(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .with_context(|| format!("cannot open {}", path.display()))
}

impl ExtensionsLock {
    pub fn acquire(path: &Path) -> Result<Self> {
        let file = open(path)?;
        flock(&file, FlockOperation::LockExclusive)
            .with_context(|| format!("cannot lock {}", path.display()))?;
        Ok(Self { _file: file })
    }

    pub fn try_acquire(path: &Path) -> Result<Option<Self>> {
        let file = open(path)?;
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(error) if io::Error::from(error).kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(io::Error::from(error))
                .with_context(|| format!("cannot lock {}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Barrier;

    use super::ExtensionsLock;

    #[test]
    fn a_held_lock_excludes_other_holders_until_dropped() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("runtime/extensions.lock");
        let held = ExtensionsLock::acquire(&path).unwrap();
        assert!(ExtensionsLock::try_acquire(&path).unwrap().is_none());
        drop(held);
        assert!(ExtensionsLock::try_acquire(&path).unwrap().is_some());
    }

    #[test]
    fn contending_threads_never_overlap() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("extensions.lock");
        let inside = AtomicUsize::new(0);
        let barrier = Barrier::new(8);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    barrier.wait();
                    for _ in 0..25 {
                        let _guard = ExtensionsLock::acquire(&path).unwrap();
                        assert_eq!(inside.fetch_add(1, Ordering::SeqCst), 0);
                        std::thread::yield_now();
                        inside.fetch_sub(1, Ordering::SeqCst);
                    }
                });
            }
        });
    }
}
