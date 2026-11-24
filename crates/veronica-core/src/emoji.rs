use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

use serde::{Deserialize, Serialize};

pub const CATALOG_JSON: &str = include_str!("../../../resources/emoji-catalog.json");

pub const LEDGER_CAPACITY: usize = 200;
pub const HALF_LIFE_DAYS: f64 = 21.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkinTone {
    #[default]
    Standard,
    Light,
    MediumLight,
    Medium,
    MediumDark,
    Dark,
}

impl SkinTone {
    pub const ALL: [SkinTone; 6] = [
        SkinTone::Standard,
        SkinTone::Light,
        SkinTone::MediumLight,
        SkinTone::Medium,
        SkinTone::MediumDark,
        SkinTone::Dark,
    ];

    pub fn title(self) -> &'static str {
        match self {
            SkinTone::Standard => "Default",
            SkinTone::Light => "Light",
            SkinTone::MediumLight => "Medium light",
            SkinTone::Medium => "Medium",
            SkinTone::MediumDark => "Medium dark",
            SkinTone::Dark => "Dark",
        }
    }

    pub fn sample(self) -> &'static str {
        match self {
            SkinTone::Standard => "✋",
            SkinTone::Light => "✋🏻",
            SkinTone::MediumLight => "✋🏼",
            SkinTone::Medium => "✋🏽",
            SkinTone::MediumDark => "✋🏾",
            SkinTone::Dark => "✋🏿",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            SkinTone::Standard => "standard",
            SkinTone::Light => "light",
            SkinTone::MediumLight => "mediumLight",
            SkinTone::Medium => "medium",
            SkinTone::MediumDark => "mediumDark",
            SkinTone::Dark => "dark",
        }
    }

    fn variant_index(self) -> Option<usize> {
        match self {
            SkinTone::Standard => None,
            SkinTone::Light => Some(1),
            SkinTone::MediumLight => Some(2),
            SkinTone::Medium => Some(3),
            SkinTone::MediumDark => Some(4),
            SkinTone::Dark => Some(5),
        }
    }

    pub fn parse(raw: &str) -> Self {
        SkinTone::ALL
            .into_iter()
            .find(|tone| tone.key().eq_ignore_ascii_case(raw))
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(rename = "symbol")]
    pub symbol: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct RawEmoji {
    #[serde(rename = "e")]
    character: String,
    #[serde(rename = "n")]
    name: String,
    #[serde(rename = "g")]
    group_index: usize,
    #[serde(rename = "v")]
    unicode_version: f64,
    #[serde(rename = "t", default)]
    terms: Vec<String>,
    #[serde(rename = "s", default)]
    tone_variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Emoji {
    pub character: String,
    pub name: String,
    pub group_index: usize,
    pub unicode_version: f64,
    pub terms: Vec<String>,
    pub tone_variants: Vec<String>,
}

impl Emoji {
    pub fn supports_skin_tones(&self) -> bool {
        !self.tone_variants.is_empty()
    }

    pub fn character(&self, tone: SkinTone) -> &str {
        match tone.variant_index() {
            Some(index) => self
                .tone_variants
                .get(index - 1)
                .map(String::as_str)
                .unwrap_or(&self.character),
            None => &self.character,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawCatalog {
    schema: u32,
    source: String,
    groups: Vec<Group>,
    emoji: Vec<RawEmoji>,
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub source: String,
    pub groups: Vec<Group>,
    emoji: Vec<Emoji>,
    index: Vec<IndexEntry>,
}

#[derive(Debug, Clone)]
struct IndexEntry {
    name: String,
    words: Vec<String>,
    terms: Vec<String>,
}

pub const SCHEMA: u32 = 1;

impl Catalog {
    pub fn bundled() -> Self {
        Self::parse(CATALOG_JSON).expect("the bundled emoji catalogue is valid")
    }

    pub fn parse(json: &str) -> anyhow::Result<Self> {
        let raw: RawCatalog = serde_json::from_str(json)?;
        if raw.schema != SCHEMA {
            anyhow::bail!(
                "emoji catalogue schema {} is unsupported; this build reads schema {SCHEMA}",
                raw.schema
            );
        }
        let group_count = raw.groups.len();
        let emoji: Vec<Emoji> = raw
            .emoji
            .into_iter()
            .filter(|entry| entry.group_index < group_count)
            .map(|entry| Emoji {
                character: entry.character,
                name: entry.name,
                group_index: entry.group_index,
                unicode_version: entry.unicode_version,
                terms: entry.terms,
                tone_variants: entry.tone_variants,
            })
            .collect();

        let index = emoji
            .iter()
            .map(|emoji| {
                let name = normalize(&emoji.name);
                IndexEntry {
                    words: words(&name),
                    name,
                    terms: emoji.terms.iter().map(|term| normalize(term)).collect(),
                }
            })
            .collect();

        Ok(Self {
            source: raw.source,
            groups: raw.groups,
            emoji,
            index,
        })
    }

    pub fn emoji(&self) -> &[Emoji] {
        &self.emoji
    }

    pub fn len(&self) -> usize {
        self.emoji.len()
    }

    pub fn is_empty(&self) -> bool {
        self.emoji.is_empty()
    }

    pub fn find(&self, character: &str) -> Option<&Emoji> {
        self.emoji.iter().find(|emoji| emoji.character == character)
    }

    pub fn group(&self, index: usize) -> Vec<&Emoji> {
        self.emoji
            .iter()
            .filter(|emoji| emoji.group_index == index)
            .collect()
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<&Emoji> {
        let normalized = normalize(query);
        if normalized.is_empty() {
            return self.emoji.iter().take(limit).collect();
        }

        let mut buckets: [Vec<&Emoji>; 7] = Default::default();
        for (position, entry) in self.index.iter().enumerate() {
            if let Some(rank) = score(entry, &normalized) {
                buckets[rank].push(&self.emoji[position]);
            }
        }
        buckets.into_iter().flatten().take(limit).collect()
    }
}

pub fn normalize(query: &str) -> String {
    query
        .trim()
        .to_lowercase()
        .replace('_', " ")
        .replace(':', "")
}

fn words(name: &str) -> Vec<String> {
    name.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

fn score(entry: &IndexEntry, query: &str) -> Option<usize> {
    if entry.name == query {
        return Some(0);
    }
    if entry.name.starts_with(query) {
        return Some(1);
    }
    if entry.words.iter().any(|word| word.starts_with(query)) {
        return Some(2);
    }
    if entry.terms.iter().any(|term| term == query) {
        return Some(3);
    }
    if entry.terms.iter().any(|term| term.starts_with(query)) {
        return Some(4);
    }
    if entry.name.contains(query) {
        return Some(5);
    }
    if entry.terms.iter().any(|term| term.contains(query)) {
        return Some(6);
    }
    None
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub character: String,
    pub count: u32,
    pub last_used_at_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLedger {
    pub entries: Vec<Usage>,
}

impl UsageLedger {
    pub fn score(usage: &Usage, now_ms: i64) -> f64 {
        let age_days = (now_ms - usage.last_used_at_ms).max(0) as f64 / 86_400_000.0;
        usage.count as f64 * 0.5_f64.powf(age_days / HALF_LIFE_DAYS)
    }

    pub fn ranked(&self, now_ms: i64, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }
        let mut scored: Vec<(&Usage, f64)> = self
            .entries
            .iter()
            .map(|usage| (usage, Self::score(usage, now_ms)))
            .collect();
        scored.sort_by(|(left, left_score), (right, right_score)| {
            right_score
                .partial_cmp(left_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(right.last_used_at_ms.cmp(&left.last_used_at_ms))
                .then(left.character.cmp(&right.character))
        });
        scored
            .into_iter()
            .take(limit)
            .map(|(usage, _)| usage.character.clone())
            .collect()
    }

    pub fn record(&mut self, character: &str, now_ms: i64) {
        match self
            .entries
            .iter_mut()
            .find(|usage| usage.character == character)
        {
            Some(usage) => {
                usage.count += 1;
                usage.last_used_at_ms = now_ms;
            }
            None => self.entries.push(Usage {
                character: character.to_string(),
                count: 1,
                last_used_at_ms: now_ms,
            }),
        }

        if self.entries.len() <= LEDGER_CAPACITY {
            return;
        }
        let mut survivors: HashMap<&str, ()> = HashMap::new();
        let ranked = self.ranked(now_ms, LEDGER_CAPACITY - 1);
        for character in &ranked {
            survivors.insert(character.as_str(), ());
        }
        survivors.insert(character, ());
        let keep: Vec<String> = survivors.keys().map(|value| value.to_string()).collect();
        self.entries
            .retain(|usage| keep.iter().any(|character| character == &usage.character));
    }

    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        crate::fsutil::write_atomic(path, &serde_json::to_vec_pretty(self)?)
    }

    pub fn forget(&mut self, character: &str) {
        self.entries.retain(|usage| usage.character != character);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        Catalog::bundled()
    }

    const DAY_MS: i64 = 86_400_000;

    #[test]
    fn the_bundled_catalogue_parses_and_carries_ediths_groups() {
        let catalog = catalog();
        assert!(catalog.len() > 1_500, "got {}", catalog.len());
        let ids: Vec<&str> = catalog.groups.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "smileys-emotion",
                "people-body",
                "animals-nature",
                "food-drink",
                "travel-places",
                "activities",
                "objects",
                "symbols",
                "flags",
            ]
        );
        assert!(catalog.source.starts_with("emojibase-data@"));
    }

    #[test]
    fn a_newer_schema_is_refused_rather_than_misread() {
        let json = r#"{"schema":2,"source":"x","groups":[],"emoji":[]}"#;
        let error = Catalog::parse(json).unwrap_err().to_string();
        assert!(error.contains("schema 2"), "got {error}");
    }

    #[test]
    fn an_emoji_in_a_group_that_does_not_exist_is_dropped() {
        let json = r#"{"schema":1,"source":"x",
          "groups":[{"id":"a","name":"A","symbol":"s"}],
          "emoji":[{"e":"😀","n":"grinning face","g":0,"v":1},
                   {"e":"🙃","n":"upside down","g":7,"v":1}]}"#;
        let catalog = Catalog::parse(json).unwrap();
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog.emoji()[0].character, "😀");
    }

    #[test]
    fn every_emoji_belongs_to_a_group_that_exists() {
        let catalog = catalog();
        for emoji in catalog.emoji() {
            assert!(
                emoji.group_index < catalog.groups.len(),
                "{} has group {}",
                emoji.character,
                emoji.group_index
            );
        }
    }

    #[test]
    fn an_exact_name_outranks_a_prefix_and_a_term() {
        let catalog = catalog();
        let results = catalog.search("grinning face", 5);
        assert_eq!(results[0].name, "grinning face");
    }

    #[test]
    fn a_search_finds_by_term_not_only_by_name() {
        let catalog = catalog();
        let results = catalog.search("cya", 20);
        assert!(
            results.iter().any(|emoji| emoji.character == "👋"),
            "waving hand should be found by its term"
        );
    }

    #[test]
    fn a_colon_wrapped_or_underscored_query_is_the_same_query() {
        let catalog = catalog();
        let plain = catalog.search("grinning face", 3);
        let decorated = catalog.search(":grinning_face:", 3);
        assert_eq!(
            plain.iter().map(|e| &e.character).collect::<Vec<_>>(),
            decorated.iter().map(|e| &e.character).collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_empty_query_is_the_whole_catalogue_in_its_own_order() {
        let catalog = catalog();
        let results = catalog.search("   ", 10);
        assert_eq!(results.len(), 10);
        assert_eq!(results[0].character, catalog.emoji()[0].character);
    }

    #[test]
    fn a_query_that_matches_nothing_returns_nothing_rather_than_everything() {
        assert!(catalog().search("zzzzzznotanemoji", 20).is_empty());
    }

    #[test]
    fn the_limit_is_honoured_on_both_paths() {
        let catalog = catalog();
        assert_eq!(catalog.search("face", 4).len(), 4);
        assert_eq!(catalog.search("", 4).len(), 4);
    }

    #[test]
    fn a_skin_tone_picks_the_right_variant_and_falls_back_when_there_is_none() {
        let catalog = catalog();
        let wave = catalog.find("👋").unwrap();
        assert!(wave.supports_skin_tones());
        assert_eq!(wave.character(SkinTone::Standard), "👋");
        assert_eq!(wave.character(SkinTone::Light), "👋🏻");
        assert_eq!(wave.character(SkinTone::Dark), "👋🏿");

        let grin = catalog.find("😀").unwrap();
        assert!(!grin.supports_skin_tones());
        assert_eq!(grin.character(SkinTone::Dark), "😀");
    }

    #[test]
    fn an_unknown_tone_falls_back_to_the_default() {
        assert_eq!(SkinTone::parse("neon"), SkinTone::Standard);
        assert_eq!(SkinTone::parse(""), SkinTone::Standard);
        assert_eq!(SkinTone::parse("mediumDark"), SkinTone::MediumDark);
        assert_eq!(SkinTone::parse("MEDIUMDARK"), SkinTone::MediumDark);
    }

    #[test]
    fn every_tone_has_a_distinct_sample_so_the_chooser_is_readable() {
        let samples: Vec<&str> = SkinTone::ALL.iter().map(|tone| tone.sample()).collect();
        let mut sorted = samples.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), samples.len());
    }

    #[test]
    fn a_group_lists_only_its_own_members_in_catalogue_order() {
        let catalog = catalog();
        let flags = catalog.groups.iter().position(|g| g.id == "flags").unwrap();
        let members = catalog.group(flags);
        assert!(!members.is_empty());
        assert!(members.iter().all(|emoji| emoji.group_index == flags));
    }

    #[test]
    fn recording_a_pick_counts_it() {
        let mut ledger = UsageLedger::default();
        ledger.record("😀", 0);
        ledger.record("😀", 1_000);
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(ledger.entries[0].count, 2);
        assert_eq!(ledger.entries[0].last_used_at_ms, 1_000);
    }

    #[test]
    fn a_recent_pick_outranks_a_stale_habit() {
        let now = 100 * DAY_MS;
        let ledger = UsageLedger {
            entries: vec![
                Usage {
                    character: "🙂".into(),
                    count: 5,
                    last_used_at_ms: now,
                },
                Usage {
                    character: "😀".into(),
                    count: 20,
                    last_used_at_ms: now - 42 * DAY_MS,
                },
            ],
        };
        assert_eq!(ledger.ranked(now, 2), vec!["🙂", "😀"]);
    }

    #[test]
    fn the_half_life_is_exactly_twenty_one_days() {
        let usage = Usage {
            character: "😀".into(),
            count: 8,
            last_used_at_ms: 0,
        };
        let decayed = UsageLedger::score(&usage, 21 * DAY_MS);
        assert!((decayed - 4.0).abs() < 1e-9, "got {decayed}");
    }

    #[test]
    fn a_clock_that_went_backwards_does_not_inflate_a_score() {
        let usage = Usage {
            character: "😀".into(),
            count: 3,
            last_used_at_ms: 10 * DAY_MS,
        };
        assert_eq!(
            UsageLedger::score(&usage, 0),
            3.0,
            "a negative age is no age"
        );
    }

    #[test]
    fn ties_break_on_recency_then_on_the_character_so_the_order_is_stable() {
        let now = 10 * DAY_MS;
        let ledger = UsageLedger {
            entries: vec![
                Usage {
                    character: "🅱".into(),
                    count: 1,
                    last_used_at_ms: now,
                },
                Usage {
                    character: "🅰".into(),
                    count: 1,
                    last_used_at_ms: now,
                },
                Usage {
                    character: "🅾".into(),
                    count: 1,
                    last_used_at_ms: now - DAY_MS,
                },
            ],
        };
        assert_eq!(ledger.ranked(now, 3), vec!["🅰", "🅱", "🅾"]);
    }

    #[test]
    fn the_ledger_is_capped_and_the_character_just_used_always_survives() {
        let mut ledger = UsageLedger::default();
        for index in 0..LEDGER_CAPACITY {
            ledger.entries.push(Usage {
                character: format!("e{index}"),
                count: 50,
                last_used_at_ms: 100 * DAY_MS,
            });
        }
        ledger.record("brand-new", 100 * DAY_MS);
        assert_eq!(ledger.entries.len(), LEDGER_CAPACITY);
        assert!(
            ledger
                .entries
                .iter()
                .any(|usage| usage.character == "brand-new"),
            "a single use must not be evicted the instant it is recorded"
        );
    }

    #[test]
    fn forgetting_and_clearing_do_what_they_say() {
        let mut ledger = UsageLedger::default();
        ledger.record("😀", 0);
        ledger.record("🙂", 0);
        ledger.forget("😀");
        assert_eq!(ledger.ranked(0, 5), vec!["🙂"]);
        ledger.clear();
        assert!(ledger.ranked(0, 5).is_empty());
    }

    #[test]
    fn asking_for_no_recents_returns_none() {
        let mut ledger = UsageLedger::default();
        ledger.record("😀", 0);
        assert!(ledger.ranked(0, 0).is_empty());
    }

    #[test]
    fn a_missing_or_corrupt_ledger_is_empty_rather_than_fatal() {
        let dir = std::env::temp_dir().join(format!("veronica-emoji-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(UsageLedger::load(&dir.join("absent.json"))
            .unwrap()
            .entries
            .is_empty());
        let corrupt = dir.join("corrupt.json");
        std::fs::write(&corrupt, b"{not json").unwrap();
        assert!(UsageLedger::load(&corrupt).unwrap().entries.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_saved_ledger_round_trips() {
        let dir = std::env::temp_dir().join(format!("veronica-emoji-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("emoji-usage.json");
        let mut ledger = UsageLedger::default();
        ledger.record("😀", 42);
        ledger.save(&path).unwrap();
        assert_eq!(UsageLedger::load(&path).unwrap(), ledger);
        std::fs::remove_dir_all(&dir).ok();
    }
}
