use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use veronica_protocol::manifest::Permissions;

const TOKENS_PER_EXTENSION: usize = 4096;

#[derive(Debug, Default)]
struct Issued {
    by_token: HashMap<String, PathBuf>,
    by_path: HashMap<PathBuf, String>,
}

#[derive(Debug, Default)]
pub(crate) struct FileTokens {
    issued: Mutex<HashMap<String, Issued>>,
}

fn random_token() -> String {
    let word = || {
        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish()
    };
    format!("{:016x}{:016x}", word(), word())
}

impl FileTokens {
    pub(crate) fn issue(&self, extension: &str, file: PathBuf) -> String {
        let mut issued = self.issued.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = issued.entry(extension.to_string()).or_default();
        if let Some(token) = entry.by_path.get(&file) {
            return token.clone();
        }
        if entry.by_token.len() >= TOKENS_PER_EXTENSION {
            *entry = Issued::default();
        }
        let token = random_token();
        entry.by_token.insert(token.clone(), file.clone());
        entry.by_path.insert(file, token.clone());
        token
    }

    pub(crate) fn lookup(&self, extension: &str, token: &str) -> Option<PathBuf> {
        let issued = self.issued.lock().unwrap_or_else(PoisonError::into_inner);
        issued.get(extension)?.by_token.get(token).cloned()
    }

    pub(crate) fn revoke(&self, extension: &str) {
        self.issued
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(extension);
    }
}

fn expand(home: &Path, raw: &str) -> Option<PathBuf> {
    let path = match raw.strip_prefix('~') {
        Some("") => home.to_path_buf(),
        Some(rest) => home.join(rest.strip_prefix('/')?),
        None => PathBuf::from(raw),
    };
    path.is_absolute().then_some(path)
}

pub(crate) fn granted_file(permissions: &Permissions, home: &Path, raw: &str) -> Option<PathBuf> {
    let file = expand(home, raw)?.canonicalize().ok()?;
    if !file.is_file() {
        return None;
    }
    permissions
        .filesystem
        .iter()
        .filter_map(|grant| expand(home, &grant.path)?.canonicalize().ok())
        .any(|scope| file.starts_with(scope))
        .then_some(file)
}

#[cfg(test)]
mod tests {
    use veronica_protocol::manifest::{FilesystemAccess, FilesystemGrant};

    use super::*;

    fn permissions(paths: &[&str]) -> Permissions {
        Permissions {
            filesystem: paths
                .iter()
                .map(|path| FilesystemGrant {
                    path: (*path).to_string(),
                    access: FilesystemAccess::Read,
                })
                .collect(),
            ..Permissions::default()
        }
    }

    #[test]
    fn only_files_inside_a_granted_scope_resolve() {
        let root = tempfile::tempdir().expect("temporary directory");
        let home = root.path().canonicalize().expect("home");
        std::fs::create_dir_all(home.join("Music/album")).expect("music");
        std::fs::create_dir_all(home.join("Documents")).expect("documents");
        std::fs::write(home.join("Music/album/song.mp3"), "song").expect("song");
        std::fs::write(home.join("Documents/tax.pdf"), "tax").expect("document");
        std::os::unix::fs::symlink(home.join("Documents/tax.pdf"), home.join("Music/leak.pdf"))
            .expect("link");
        let music = permissions(&["~/Music"]);
        let song = home.join("Music/album/song.mp3");

        assert_eq!(
            granted_file(&music, &home, "~/Music/album/song.mp3"),
            Some(song.clone())
        );
        assert_eq!(
            granted_file(&music, &home, song.to_str().expect("path")),
            Some(song)
        );
        for refused in [
            "~/Documents/tax.pdf",
            "~/Music/leak.pdf",
            "~/Music/../Documents/tax.pdf",
            "~/Music/album",
            "~/Music/absent.mp3",
            "Music/album/song.mp3",
            "~other/Music/album/song.mp3",
            "",
        ] {
            assert_eq!(granted_file(&music, &home, refused), None, "{refused}");
        }
        assert_eq!(
            granted_file(&permissions(&[]), &home, "~/Music/album/song.mp3"),
            None
        );
        assert!(granted_file(&permissions(&["~"]), &home, "~/Documents/tax.pdf").is_some());
    }

    #[test]
    fn tokens_are_stable_per_file_and_scoped_to_their_extension() {
        let tokens = FileTokens::default();
        let first = tokens.issue("media", PathBuf::from("/music/a.mp3"));
        assert_eq!(first.len(), 32);
        assert_eq!(tokens.issue("media", PathBuf::from("/music/a.mp3")), first);
        assert_ne!(tokens.issue("media", PathBuf::from("/music/b.mp3")), first);
        assert_eq!(
            tokens.lookup("media", &first),
            Some(PathBuf::from("/music/a.mp3"))
        );
        assert_eq!(tokens.lookup("other", &first), None);
        tokens.revoke("media");
        assert_eq!(tokens.lookup("media", &first), None);
    }
}
