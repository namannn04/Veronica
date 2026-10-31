//! Owned file copies. Exporting a shelf path never consumes an entry.
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct FileShelf {
    root: PathBuf,
}
#[derive(Clone, Debug)]
pub struct ShelfEntry {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}
impl FileShelf {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn ensure_root(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    /// Upgrade the old path-only shelf to owned copies, once per source path.
    pub fn import_legacy(&self, sources: &[PathBuf]) -> Result<usize> {
        self.ensure_root()?;
        let manifest = self.root.join(".legacy-imported.json");
        let mut imported: Vec<PathBuf> = match std::fs::read(&manifest) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        let mut count = 0;
        for source in sources {
            if imported.contains(source) || !source.exists() {
                continue;
            }
            self.import(source)?;
            imported.push(source.clone());
            crate::fsutil::write_atomic(&manifest, &serde_json::to_vec(&imported)?)?;
            count += 1;
        }
        Ok(count)
    }

    pub fn entries(&self) -> Result<Vec<ShelfEntry>> {
        self.ensure_root()?;
        let mut entries = Vec::new();
        for item in std::fs::read_dir(&self.root)? {
            let item = item?;
            let id = item.file_name().to_string_lossy().into_owned();
            if !valid_id(&id) || !item.file_type()?.is_dir() {
                continue;
            }
            if let Some(file) = std::fs::read_dir(item.path())?.next() {
                let file = file?;
                entries.push(ShelfEntry {
                    id,
                    name: file.file_name().to_string_lossy().into_owned(),
                    path: file.path(),
                });
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        Ok(entries)
    }

    pub fn import(&self, source: &Path) -> Result<ShelfEntry> {
        self.ensure_root()?;
        let source = source
            .canonicalize()
            .context("The dropped file is no longer available")?;
        let root = self.root.canonicalize()?;
        if source.starts_with(&root) || root.starts_with(&source) {
            bail!("A shelf cannot be copied into itself");
        }
        let name = source.file_name().context("Choose a file or folder")?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let id = format!("{}-{stamp}", std::process::id());
        let staging = root.join(format!(".partial-{id}"));
        std::fs::create_dir(&staging)?;
        let destination = staging.join(name);
        if let Err(error) = copy_tree(&source, &destination, 0) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
        let container = root.join(&id);
        if let Err(error) = std::fs::rename(&staging, &container) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error.into());
        }
        Ok(ShelfEntry {
            id,
            name: name.to_string_lossy().into_owned(),
            path: container.join(name),
        })
    }

    /// Explicit deletion affects only this private copy, never its source.
    pub fn remove(&self, id: &str) -> Result<()> {
        if !valid_id(id) {
            bail!("Invalid shelf entry");
        }
        let container = self.root.join(id);
        if std::fs::symlink_metadata(&container)?
            .file_type()
            .is_symlink()
        {
            bail!("Invalid shelf container");
        }
        std::fs::remove_dir_all(container)?;
        Ok(())
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit() || c == b'-')
}
fn copy_tree(source: &Path, destination: &Path, depth: usize) -> Result<()> {
    if depth > 64 {
        bail!("Folder nesting is too deep");
    }
    let metadata = std::fs::symlink_metadata(source)?;
    if metadata.is_file() {
        std::fs::copy(source, destination)
            .with_context(|| format!("Cannot copy {}", source.display()))?;
    } else if metadata.is_dir() {
        std::fs::create_dir(destination)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(
                &entry.path(),
                &destination.join(entry.file_name()),
                depth + 1,
            )?;
        }
    } else {
        bail!("Symbolic links and special files inside folders cannot be imported");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_are_independent_and_exports_do_not_consume_them() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("photo.png");
        std::fs::write(&source, b"original").unwrap();
        let shelf = FileShelf::new(temp.path().join("shelf"));
        let entry = shelf.import(&source).unwrap();
        std::fs::write(&source, b"changed").unwrap();
        let exported = temp.path().join("upload.png");
        std::fs::copy(&entry.path, &exported).unwrap();
        assert_eq!(std::fs::read(&exported).unwrap(), b"original");
        assert_eq!(shelf.entries().unwrap().len(), 1);
        shelf.remove(&entry.id).unwrap();
        assert_eq!(std::fs::read(source).unwrap(), b"changed");
        assert_eq!(std::fs::read(exported).unwrap(), b"original");
    }
    #[test]
    fn folders_and_duplicate_names_survive_restart() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("file"), b"data").unwrap();
        let root = temp.path().join("shelf");
        let shelf = FileShelf::new(root.clone());
        let a = shelf.import(&folder).unwrap();
        let b = shelf.import(&folder).unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(FileShelf::new(root).entries().unwrap().len(), 2);
        assert_eq!(std::fs::read(a.path.join("file")).unwrap(), b"data");
        assert!(shelf.remove("../folder").is_err());
        assert!(shelf.import(temp.path()).is_err());
    }
    #[test]
    fn legacy_paths_are_copied_once_and_removed_copies_stay_removed() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("old-file");
        std::fs::write(&source, b"saved").unwrap();
        let shelf = FileShelf::new(temp.path().join("shelf"));
        assert_eq!(
            shelf.import_legacy(std::slice::from_ref(&source)).unwrap(),
            1
        );
        let entries = shelf.entries().unwrap();
        shelf.remove(&entries[0].id).unwrap();
        assert_eq!(
            shelf.import_legacy(std::slice::from_ref(&source)).unwrap(),
            0
        );
        assert!(shelf.entries().unwrap().is_empty());
        assert!(source.exists());
    }

    #[cfg(unix)]
    #[test]
    fn failed_imports_leave_no_partial_entry_or_dangling_symlink() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        std::os::unix::fs::symlink(&folder, folder.join("loop")).unwrap();
        let shelf = FileShelf::new(temp.path().join("shelf"));
        assert!(shelf.import(&folder).is_err());
        assert!(shelf.entries().unwrap().is_empty());
    }
}
