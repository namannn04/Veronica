//! Local-first Companion notes and voice-memo index.
//!
//! Edith's Companion is a separate service. Veronica keeps the useful user
//! experience without requiring a daemon or container: text metadata is one
//! atomic JSON document and recordings live beside it under XDG data. The
//! entire directory is already included by Veronica backups.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const FORMAT_VERSION: u32 = 1;
const MAX_TITLE_CHARS: usize = 120;
const MAX_BODY_CHARS: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompanionKind {
    Note,
    Voice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionItem {
    pub id: u64,
    pub kind: CompanionKind,
    pub title: String,
    pub body: String,
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompanionDocument {
    #[serde(default = "format_version")]
    format_version: u32,
    #[serde(default = "first_id")]
    next_id: u64,
    #[serde(default)]
    items: Vec<CompanionItem>,
}

fn format_version() -> u32 {
    FORMAT_VERSION
}
fn first_id() -> u64 {
    1
}

impl Default for CompanionDocument {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            next_id: 1,
            items: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompanionRepository {
    root: PathBuf,
}

impl CompanionRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn recordings_dir(&self) -> PathBuf {
        self.root.join("recordings")
    }
    fn index_file(&self) -> PathBuf {
        self.root.join("index.json")
    }

    fn load(&self) -> Result<CompanionDocument> {
        match fs::read(self.index_file()) {
            Ok(bytes) => {
                let document: CompanionDocument =
                    serde_json::from_slice(&bytes).context("invalid Companion index")?;
                if document.format_version != FORMAT_VERSION {
                    bail!(
                        "Companion index format {} is unsupported",
                        document.format_version
                    );
                }
                Ok(document)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Default::default()),
            Err(error) => Err(error.into()),
        }
    }

    fn save(&self, document: &CompanionDocument) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let destination = self.index_file();
        let temporary = destination.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(document)?)?;
        fs::rename(&temporary, &destination)
            .with_context(|| format!("cannot replace {}", destination.display()))
    }

    pub fn list(&self, query: &str) -> Result<Vec<CompanionItem>> {
        let needle = query.trim().to_lowercase();
        let mut items: Vec<_> = self
            .load()?
            .items
            .into_iter()
            .filter(|item| {
                needle.is_empty()
                    || item.title.to_lowercase().contains(&needle)
                    || item.body.to_lowercase().contains(&needle)
            })
            .collect();
        items.sort_by(|left, right| {
            right
                .pinned
                .cmp(&left.pinned)
                .then_with(|| right.updated_at.cmp(&left.updated_at))
        });
        Ok(items)
    }

    pub fn create_note(
        &self,
        title: &str,
        body: &str,
        now: DateTime<Utc>,
    ) -> Result<CompanionItem> {
        let mut document = self.load()?;
        let item = CompanionItem {
            id: document.next_id,
            kind: CompanionKind::Note,
            title: clean_title(title, "Untitled note"),
            body: clean_body(body)?,
            pinned: false,
            created_at: now,
            updated_at: now,
            audio_path: None,
            duration_seconds: None,
        };
        document.next_id = document.next_id.saturating_add(1).max(1);
        document.items.push(item.clone());
        self.save(&document)?;
        Ok(item)
    }

    pub fn add_voice(
        &self,
        path: &Path,
        duration_seconds: u64,
        now: DateTime<Utc>,
    ) -> Result<CompanionItem> {
        if !path.is_file() || !path.starts_with(self.recordings_dir()) {
            bail!("voice recording is outside Companion storage or missing");
        }
        let mut document = self.load()?;
        let item = CompanionItem {
            id: document.next_id,
            kind: CompanionKind::Voice,
            title: format!("Voice memo · {}", now.format("%b %-d, %H:%M")),
            body: String::new(),
            pinned: false,
            created_at: now,
            updated_at: now,
            audio_path: Some(path.display().to_string()),
            duration_seconds: Some(duration_seconds.max(1)),
        };
        document.next_id = document.next_id.saturating_add(1).max(1);
        document.items.push(item.clone());
        self.save(&document)?;
        Ok(item)
    }

    pub fn update(
        &self,
        id: u64,
        title: &str,
        body: &str,
        pinned: bool,
        now: DateTime<Utc>,
    ) -> Result<CompanionItem> {
        let mut document = self.load()?;
        let item = document
            .items
            .iter_mut()
            .find(|item| item.id == id)
            .with_context(|| format!("no Companion item {id}"))?;
        item.title = clean_title(
            title,
            if item.kind == CompanionKind::Note {
                "Untitled note"
            } else {
                "Voice memo"
            },
        );
        item.body = clean_body(body)?;
        item.pinned = pinned;
        item.updated_at = now;
        let result = item.clone();
        self.save(&document)?;
        Ok(result)
    }

    pub fn remove(&self, id: u64) -> Result<()> {
        let mut document = self.load()?;
        let position = document
            .items
            .iter()
            .position(|item| item.id == id)
            .with_context(|| format!("no Companion item {id}"))?;
        let removed = document.items.remove(position);
        self.save(&document)?;
        if let Some(audio) = removed.audio_path {
            let audio = PathBuf::from(audio);
            if audio.starts_with(self.recordings_dir()) {
                match fs::remove_file(audio) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(())
    }

    pub fn audio_path(&self, id: u64) -> Result<PathBuf> {
        self.load()?
            .items
            .into_iter()
            .find(|item| item.id == id)
            .and_then(|item| item.audio_path.map(PathBuf::from))
            .filter(|path| path.is_file() && path.starts_with(self.recordings_dir()))
            .with_context(|| format!("Companion item {id} has no recording"))
    }
}

fn clean_title(value: &str, fallback: &str) -> String {
    let value: String = value.trim().chars().take(MAX_TITLE_CHARS).collect();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

fn clean_body(value: &str) -> Result<String> {
    if value.chars().count() > MAX_BODY_CHARS {
        bail!("note is too long");
    }
    Ok(value.replace("\r\n", "\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }
    fn repository() -> (tempfile::TempDir, CompanionRepository) {
        let temp = tempfile::tempdir().unwrap();
        let repository = CompanionRepository::new(temp.path());
        (temp, repository)
    }

    #[test]
    fn notes_round_trip_update_search_pin_and_delete() {
        let (_temp, repository) = repository();
        let first = repository
            .create_note("Ideas", "Build a launcher", at(1))
            .unwrap();
        repository.create_note("Shopping", "Coffee", at(2)).unwrap();
        let updated = repository
            .update(
                first.id,
                "Product ideas",
                "Build a fast launcher",
                true,
                at(3),
            )
            .unwrap();
        assert!(updated.pinned);
        assert_eq!(repository.list("launcher").unwrap(), vec![updated.clone()]);
        assert_eq!(repository.list("").unwrap()[0].id, first.id);
        repository.remove(first.id).unwrap();
        assert_eq!(repository.list("").unwrap().len(), 1);
    }

    #[test]
    fn voice_files_are_indexed_and_removed_with_the_item() {
        let (_temp, repository) = repository();
        fs::create_dir_all(repository.recordings_dir()).unwrap();
        let audio = repository.recordings_dir().join("memo.wav");
        fs::write(&audio, b"RIFF voice").unwrap();
        let memo = repository.add_voice(&audio, 7, at(1)).unwrap();
        assert_eq!(repository.audio_path(memo.id).unwrap(), audio);
        repository.remove(memo.id).unwrap();
        assert!(!audio.exists());
    }

    #[test]
    fn malformed_and_oversized_content_is_rejected_safely() {
        let (_temp, repository) = repository();
        assert!(repository
            .create_note("x", &"a".repeat(MAX_BODY_CHARS + 1), at(1))
            .is_err());
        assert!(repository
            .add_voice(Path::new("/tmp/outside.wav"), 1, at(1))
            .is_err());
    }
}
