//! The emoji picker's catalogue, search and usage ledger.
//!
//! A direct port of Edith's `EmojiCatalog`, `EmojiSearch` and
//! `EmojiUsageLedger`, over the same `emoji-catalog.json` — same emojibase
//! release, same names, same search terms, same group order, same skin-tone
//! variants — so a search for "shrug" ranks the same characters in the same
//! order on both platforms.
//!
//! What differs is only what happens after a pick. Edith synthesises the
//! keystroke into whatever app was frontmost; on Wayland only the compositor
//! may do that, so Veronica always copies to the clipboard and inserts in place
//! through its shell extension where one is running. Everything in this module
//! is the part that is identical: which emoji exist, which one a query means,
//! and which ones you reach for most.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

use serde::{Deserialize, Serialize};

/// The catalogue, embedded rather than read at runtime: the picker must open
/// instantly on a hotkey, and a missing file would be a broken feature rather
/// than a degraded one.
pub const CATALOG_JSON: &str = include_str!("../../../resources/emoji-catalog.json");

/// How many recents the ledger keeps, and how fast an old pick loses weight.
/// Edith's numbers: a character used twenty times a month ago should not
/// outrank one used five times today.
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

    /// The raised hand in this tone, which is what a tone chooser shows.
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

    /// Which tone variant this is, 1-based. `Standard` has no variant.
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

    /// Unknown values fall back to the default, as elsewhere.
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
    /// Edith stores an SF Symbol here; Veronica's interface maps it to its own
    /// glyph, so the field is carried through rather than reinterpreted.
    #[serde(rename = "symbol")]
    pub symbol: String,
}

/// One emoji, in the catalogue's compact on-disk shape.
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

    /// This emoji in one tone, falling back to the standard character when the
    /// emoji has no such variant — most do not.
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
    /// Precomputed once, because the picker searches on every keystroke.
    index: Vec<IndexEntry>,
}

#[derive(Debug, Clone)]
struct IndexEntry {
    name: String,
    words: Vec<String>,
    terms: Vec<String>,
}

/// The schema this build understands. A newer catalogue is refused rather than
/// read as if the fields still meant the same thing.
pub const SCHEMA: u32 = 1;

impl Catalog {
    /// The embedded catalogue.
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
            // A group index past the end would be an emoji no tab can show.
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

    /// Find one emoji by its standard character.
    pub fn find(&self, character: &str) -> Option<&Emoji> {
        self.emoji.iter().find(|emoji| emoji.character == character)
    }

    /// Everything in one group, in catalogue order.
    pub fn group(&self, index: usize) -> Vec<&Emoji> {
        self.emoji
            .iter()
            .filter(|emoji| emoji.group_index == index)
            .collect()
    }

    /// Search, ranked. An empty query is the whole catalogue in its own order,
    /// which is what an unfiltered picker shows.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&Emoji> {
        let normalized = normalize(query);
        if normalized.is_empty() {
            return self.emoji.iter().take(limit).collect();
        }

        // Seven buckets, filled in catalogue order, then concatenated: within a
        // rank the catalogue's own order decides, so results are stable.
        let mut buckets: [Vec<&Emoji>; 7] = Default::default();
        for (position, entry) in self.index.iter().enumerate() {
            if let Some(rank) = score(entry, &normalized) {
                buckets[rank].push(&self.emoji[position]);
            }
        }
        buckets.into_iter().flatten().take(limit).collect()
    }
}

/// Edith's normalisation: `:smiling_face:` and `Smiling Face` are one query.
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

/// How well one entry matches, lower is better. `None` is no match.
///
/// The ladder is Edith's exactly: an exact name, then a name prefix, then a
/// word prefix, then an exact term, a term prefix, a name substring, and last a
/// term substring. It is what makes "grin" put 😀 above 😬.
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
    /// Milliseconds since the Unix epoch, so the ledger needs no clock type.
    pub last_used_at_ms: i64,
}

/// Which emoji you reach for, weighted so an old habit decays.
///
/// Edith's `EmojiUsageLedger`: a count with a 21-day half-life, capped at 200
/// characters. A plain count would pin last year's favourites to the top of the
/// picker forever.
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

    /// The most-used characters, best first.
    pub fn ranked(&self, now_ms: i64, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }
        let mut scored: Vec<(&Usage, f64)> = self
            .entries
            .iter()
            .map(|usage| (usage, Self::score(usage, now_ms)))
            .collect();
        // Ties break on recency and then on the character itself, so the order
        // is total and the picker never reshuffles between two equal entries.
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
        // The character just used always survives, even if its single use ranks
        // below everything else — dropping what was just picked would be absurd.
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

    /// A missing file is an empty ledger, not an error: that is a first pick.
    /// A corrupt one is also empty rather than fatal, because losing a list of
    /// recents must never stop the picker from opening.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    /// Written atomically, so a crash mid-write cannot truncate the file.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_vec_pretty(self)?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, &body)?;
        std::fs::rename(&temp, path)
            .with_context(|| format!("cannot replace {}", path.display()))?;
        Ok(())
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
        // It would otherwise be listed under no tab and reachable only by search.
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
        // "cya" is a term on the waving hand, and appears in no name.
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

        // Most emoji have no variants; asking for a tone must not blank them.
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
        // Five uses today beat twenty uses two half-lives ago: 5 > 20 * 0.25.
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
        // Fill it with well-used, recent characters.
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
