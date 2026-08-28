use std::path::Path;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "opus", "wav", "m4a", "aac", "webm",
];
const MAX_TRACKS: usize = 20_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalTrack {
    pub id: String,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_path: Option<String>,
}

pub fn scan_library(root: &Path) -> Result<Vec<LocalTrack>> {
    if !root.is_absolute() {
        bail!("music library path must be absolute");
    }
    if !root.is_dir() {
        bail!("music library folder does not exist: {}", root.display());
    }
    let canonical = root.canonicalize()?;
    let mut tracks = Vec::new();
    for entry in WalkDir::new(&canonical)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if tracks.len() >= MAX_TRACKS {
            break;
        }
        let path = entry.path();
        if !entry.file_type().is_file() || !is_audio(path) {
            continue;
        }
        let Ok(path) = path.canonicalize() else {
            continue;
        };
        if !path.starts_with(&canonical) {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Unknown track");
        let (artist, title) = stem
            .split_once(" - ")
            .map(|(artist, title)| (artist.trim(), title.trim()))
            .unwrap_or(("Unknown artist", stem));
        let album = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
            .unwrap_or("Music");
        let art_path = path
            .parent()
            .and_then(find_art)
            .map(|path| path.display().to_string());
        tracks.push(LocalTrack {
            id: stable_id(&path),
            path: path.display().to_string(),
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            art_path,
        });
    }
    tracks.sort_by(|left, right| {
        left.artist
            .to_lowercase()
            .cmp(&right.artist.to_lowercase())
            .then_with(|| left.album.to_lowercase().cmp(&right.album.to_lowercase()))
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
    });
    Ok(tracks)
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| AUDIO_EXTENSIONS.contains(&value.to_lowercase().as_str()))
        .unwrap_or(false)
}
fn find_art(directory: &Path) -> Option<std::path::PathBuf> {
    ["cover.jpg", "cover.png", "folder.jpg", "folder.png"]
        .into_iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
}
fn stable_id(path: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hash);
    format!("{:016x}", hash.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    fn root() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "veronica-music-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("Album")).unwrap();
        root
    }
    #[test]
    fn indexes_supported_audio_and_ignores_other_files() {
        let root = root();
        std::fs::write(root.join("Album/Artist - Song.mp3"), []).unwrap();
        std::fs::write(root.join("Album/readme.txt"), []).unwrap();
        let tracks = scan_library(&root).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].artist, "Artist");
        assert_eq!(tracks[0].title, "Song");
        assert_eq!(tracks[0].album, "Album");
    }
    #[test]
    fn adjacent_cover_is_detected() {
        let root = root();
        std::fs::write(root.join("Album/Song.flac"), []).unwrap();
        std::fs::write(root.join("Album/cover.jpg"), []).unwrap();
        assert!(scan_library(&root).unwrap()[0].art_path.is_some());
    }
    #[test]
    fn relative_and_missing_roots_are_rejected() {
        assert!(scan_library(Path::new("Music")).is_err());
        assert!(scan_library(Path::new("/definitely/not/veronica/music")).is_err());
    }
}
