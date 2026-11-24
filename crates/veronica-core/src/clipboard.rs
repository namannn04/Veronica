use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const DEFAULT_LIMIT: usize = 200;

pub const MAX_TEXT_BYTES: usize = 64 * 1024;

pub const PREVIEW_CHARS: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipEntry {
    pub id: u64,
    pub text: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub count: u32,
}

impl ClipEntry {
    pub fn preview(&self) -> String {
        let collapsed = self.text.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.chars().count() <= PREVIEW_CHARS {
            return collapsed;
        }
        let truncated: String = collapsed.chars().take(PREVIEW_CHARS).collect();
        format!("{truncated}…")
    }

    pub fn line_count(&self) -> usize {
        self.text.lines().count().max(1)
    }

    pub fn byte_len(&self) -> usize {
        self.text.len()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardHistory {
    #[serde(default)]
    pub entries: Vec<ClipEntry>,
    #[serde(default)]
    next_id: u64,
}

impl ClipboardHistory {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let mut history: Self = serde_json::from_slice(&bytes)
                    .with_context(|| format!("{} is not a clipboard history", path.display()))?;
                let highest = history.entries.iter().map(|e| e.id).max().unwrap_or(0);
                history.next_id = history.next_id.max(highest + 1);
                Ok(history)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        crate::fsutil::write_atomic(path, &serde_json::to_vec(self)?)
    }

    pub fn is_recordable(text: &str) -> bool {
        !text.trim().is_empty() && text.len() <= MAX_TEXT_BYTES
    }

    pub fn record(&mut self, text: &str, now: DateTime<Utc>, limit: usize) -> Option<u64> {
        if !Self::is_recordable(text) {
            return None;
        }

        if let Some(position) = self.entries.iter().position(|entry| entry.text == text) {
            let mut entry = self.entries.remove(position);
            entry.last_seen = now;
            entry.count = entry.count.saturating_add(1);
            let id = entry.id;
            self.entries.insert(0, entry);
            return Some(id);
        }

        let id = self.next_id.max(1);
        self.next_id = id + 1;
        self.entries.insert(
            0,
            ClipEntry {
                id,
                text: text.to_string(),
                first_seen: now,
                last_seen: now,
                count: 1,
            },
        );
        self.entries.truncate(limit.max(1));
        Some(id)
    }

    pub fn get(&self, id: u64) -> Option<&ClipEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        self.entries.len() != before
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn search(&self, query: &str) -> Vec<&ClipEntry> {
        let needle = query.trim().to_lowercase();
        self.entries
            .iter()
            .filter(|entry| needle.is_empty() || entry.text.to_lowercase().contains(&needle))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_787_000_000 + seconds, 0).unwrap()
    }

    #[test]
    fn records_newest_first() {
        let mut history = ClipboardHistory::default();
        history.record("first", at(0), DEFAULT_LIMIT);
        history.record("second", at(1), DEFAULT_LIMIT);
        assert_eq!(history.entries[0].text, "second");
        assert_eq!(history.entries[1].text, "first");
    }

    #[test]
    fn ids_are_unique_and_stable() {
        let mut history = ClipboardHistory::default();
        let first = history.record("a", at(0), DEFAULT_LIMIT).unwrap();
        let second = history.record("b", at(1), DEFAULT_LIMIT).unwrap();
        assert_ne!(first, second);
        let again = history.record("a", at(2), DEFAULT_LIMIT).unwrap();
        assert_eq!(again, first);
    }

    #[test]
    fn recopying_moves_to_front_and_counts_rather_than_duplicating() {
        let mut history = ClipboardHistory::default();
        history.record("keep", at(0), DEFAULT_LIMIT);
        history.record("other", at(1), DEFAULT_LIMIT);
        history.record("keep", at(2), DEFAULT_LIMIT);

        assert_eq!(history.entries.len(), 2, "no duplicate row");
        assert_eq!(history.entries[0].text, "keep");
        assert_eq!(history.entries[0].count, 2);
        assert_eq!(
            history.entries[0].first_seen,
            at(0),
            "first seen is preserved"
        );
        assert_eq!(history.entries[0].last_seen, at(2));
    }

    #[test]
    fn blank_and_oversized_text_is_not_recorded() {
        let mut history = ClipboardHistory::default();
        assert!(history.record("", at(0), DEFAULT_LIMIT).is_none());
        assert!(history.record("   \n\t ", at(0), DEFAULT_LIMIT).is_none());
        let huge = "x".repeat(MAX_TEXT_BYTES + 1);
        assert!(history.record(&huge, at(0), DEFAULT_LIMIT).is_none());
        assert!(history.entries.is_empty());
        let at_limit = "y".repeat(MAX_TEXT_BYTES);
        assert!(history.record(&at_limit, at(0), DEFAULT_LIMIT).is_some());
    }

    #[test]
    fn the_history_is_capped_and_drops_the_oldest() {
        let mut history = ClipboardHistory::default();
        for index in 0..10 {
            history.record(&format!("entry {index}"), at(index), 4);
        }
        assert_eq!(history.entries.len(), 4);
        assert_eq!(history.entries[0].text, "entry 9");
        assert_eq!(history.entries[3].text, "entry 6");
    }

    #[test]
    fn a_zero_limit_still_keeps_one_entry_rather_than_discarding_everything() {
        let mut history = ClipboardHistory::default();
        history.record("only", at(0), 0);
        assert_eq!(history.entries.len(), 1);
    }

    #[test]
    fn preview_collapses_whitespace_and_truncates() {
        let mut history = ClipboardHistory::default();
        history.record("line one\n\n   line   two\t\tthree", at(0), DEFAULT_LIMIT);
        assert_eq!(history.entries[0].preview(), "line one line two three");

        let long = "word ".repeat(200);
        history.record(&long, at(1), DEFAULT_LIMIT);
        let preview = history.entries[0].preview();
        assert!(preview.ends_with('…'));
        assert_eq!(preview.chars().count(), PREVIEW_CHARS + 1);
    }

    #[test]
    fn preview_counts_characters_not_bytes() {
        let mut history = ClipboardHistory::default();
        let text = "मैं ना मानू ".repeat(40);
        history.record(&text, at(0), DEFAULT_LIMIT);
        let preview = history.entries[0].preview();
        assert_eq!(preview.chars().count(), PREVIEW_CHARS + 1);
    }

    #[test]
    fn search_is_case_insensitive_and_an_empty_query_matches_all() {
        let mut history = ClipboardHistory::default();
        history.record("Hello World", at(0), DEFAULT_LIMIT);
        history.record("goodbye", at(1), DEFAULT_LIMIT);

        assert_eq!(history.search("hello").len(), 1);
        assert_eq!(history.search("WORLD").len(), 1);
        assert_eq!(history.search("").len(), 2);
        assert_eq!(history.search("   ").len(), 2);
        assert!(history.search("absent").is_empty());
    }

    #[test]
    fn remove_and_clear() {
        let mut history = ClipboardHistory::default();
        let id = history.record("gone", at(0), DEFAULT_LIMIT).unwrap();
        history.record("stays", at(1), DEFAULT_LIMIT);
        assert!(history.remove(id));
        assert!(
            !history.remove(id),
            "removing twice reports nothing removed"
        );
        assert_eq!(history.entries.len(), 1);
        history.clear();
        assert!(history.entries.is_empty());
    }

    #[test]
    fn line_count_reflects_the_shape_of_the_content() {
        let mut history = ClipboardHistory::default();
        history.record("one", at(0), DEFAULT_LIMIT);
        assert_eq!(history.entries[0].line_count(), 1);
        history.record("a\nb\nc", at(1), DEFAULT_LIMIT);
        assert_eq!(history.entries[0].line_count(), 3);
    }

    #[test]
    fn a_missing_file_is_an_empty_history() {
        let history = ClipboardHistory::load(Path::new("/nonexistent/clip.json")).unwrap();
        assert!(history.entries.is_empty());
    }

    #[test]
    fn survives_a_round_trip_and_never_reuses_an_id() {
        let dir = std::env::temp_dir().join(format!("veronica-clip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clipboard.json");

        let mut history = ClipboardHistory::default();
        history.record("one", at(0), DEFAULT_LIMIT);
        let second = history.record("two", at(1), DEFAULT_LIMIT).unwrap();
        history.save(&path).unwrap();

        let mut reloaded = ClipboardHistory::load(&path).unwrap();
        assert_eq!(reloaded.entries.len(), 2);
        let third = reloaded.record("three", at(2), DEFAULT_LIMIT).unwrap();
        assert!(third > second, "ids must not be reused after a reload");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_without_the_id_counter_recovers_it_from_the_entries() {
        let dir = std::env::temp_dir().join(format!("veronica-clip2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clipboard.json");
        std::fs::write(
            &path,
            br#"{"entries":[{"id":7,"text":"old","firstSeen":"2026-08-20T00:00:00Z","lastSeen":"2026-08-20T00:00:00Z","count":1}]}"#,
        )
        .unwrap();

        let mut history = ClipboardHistory::load(&path).unwrap();
        let id = history.record("new", at(0), DEFAULT_LIMIT).unwrap();
        assert!(id > 7, "must not collide with the existing id, got {id}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
