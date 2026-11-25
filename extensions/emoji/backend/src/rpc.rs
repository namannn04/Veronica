use serde::{Deserialize, Serialize};
use serde_json::Value;
use veronica_core::emoji::{Catalog, Emoji, SkinTone, UsageLedger};
use veronica_sdk::protocol::rpc::ErrorObject;
use veronica_sdk::rpc::{failure, params, reply};

use crate::preferences::Preferences;

pub(crate) const METHODS: [&str; 4] = ["groups", "search", "recents", "copy"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmojiRow {
    pub character: String,
    pub name: String,
    pub group_index: usize,
    pub supports_skin_tones: bool,
}

impl EmojiRow {
    pub(crate) fn new(emoji: &Emoji, tone: SkinTone) -> Self {
        Self {
            character: emoji.character(tone).to_string(),
            name: emoji.name.clone(),
            group_index: emoji.group_index,
            supports_skin_tones: emoji.supports_skin_tones(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CopyResult {
    character: String,
    copied_via: String,
    inserted: bool,
}

#[derive(Deserialize)]
struct SearchParams {
    #[serde(default)]
    query: String,
    #[serde(default)]
    group: Option<usize>,
    limit: usize,
}

#[derive(Deserialize)]
struct LimitParams {
    limit: usize,
}

#[derive(Deserialize)]
struct CopyParams {
    character: String,
}

pub(crate) fn search(
    catalog: &Catalog,
    tone: SkinTone,
    query: &str,
    group: Option<usize>,
    limit: usize,
) -> Vec<EmojiRow> {
    catalog
        .search(query, usize::MAX)
        .into_iter()
        .filter(|emoji| group.is_none_or(|index| emoji.group_index == index))
        .take(limit)
        .map(|emoji| EmojiRow::new(emoji, tone))
        .collect()
}

pub(crate) fn base_character(catalog: &Catalog, tone: SkinTone, character: &str) -> String {
    catalog
        .emoji()
        .iter()
        .find(|emoji| emoji.character(tone) == character)
        .map_or_else(|| character.to_string(), |emoji| emoji.character.clone())
}

pub(crate) async fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    let fail = |error: anyhow::Error| failure(&error);
    let catalog = Catalog::bundled();
    match method {
        "groups" => reply(&catalog.groups),
        "search" => {
            let SearchParams {
                query,
                group,
                limit,
            } = params(arguments)?;
            let preferences = Preferences::load().map_err(fail)?;
            reply(&search(&catalog, preferences.tone, &query, group, limit))
        }
        "recents" => {
            let LimitParams { limit } = params(arguments)?;
            let preferences = Preferences::load().map_err(fail)?;
            let ledger = UsageLedger::load(&preferences.path).map_err(fail)?;
            let rows: Vec<EmojiRow> = ledger
                .ranked(chrono::Utc::now().timestamp_millis(), limit)
                .into_iter()
                .filter_map(|character| {
                    catalog
                        .find(&character)
                        .map(|emoji| EmojiRow::new(emoji, preferences.tone))
                })
                .collect();
            reply(&rows)
        }
        "copy" => {
            let CopyParams { character } = params(arguments)?;
            let preferences = Preferences::load().map_err(fail)?;
            let base = base_character(&catalog, preferences.tone, &character);
            let mut ledger = UsageLedger::load(&preferences.path).map_err(fail)?;
            ledger.record(&base, chrono::Utc::now().timestamp_millis());
            ledger.save(&preferences.path).map_err(fail)?;
            let copied_via = veronica_system::selection::write(&character)
                .await
                .map_err(fail)?
                .title()
                .to_string();
            reply(&CopyResult {
                character,
                copied_via,
                inserted: false,
            })
        }
        other => Err(ErrorObject::method_not_found(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_filters_by_group_and_applies_the_tone() {
        let catalog = Catalog::bundled();
        let all = search(&catalog, SkinTone::Standard, "", None, 5);
        assert_eq!(all.len(), 5);
        let group = all[0].group_index;
        assert!(search(&catalog, SkinTone::Standard, "", Some(group), 50)
            .iter()
            .all(|row| row.group_index == group));
        let toned = catalog
            .emoji()
            .iter()
            .find(|emoji| emoji.supports_skin_tones())
            .unwrap();
        let dark = toned.character(SkinTone::Dark).to_string();
        assert_eq!(
            base_character(&catalog, SkinTone::Dark, &dark),
            toned.character
        );
        assert_eq!(base_character(&catalog, SkinTone::Dark, "zz"), "zz");
    }

    #[test]
    fn rows_are_camel_case() {
        let catalog = Catalog::bundled();
        let row = EmojiRow::new(&catalog.emoji()[0], SkinTone::Standard);
        let json = serde_json::to_value(row).unwrap();
        assert!(json["groupIndex"].is_u64());
        assert!(json["supportsSkinTones"].is_boolean());
    }
}
