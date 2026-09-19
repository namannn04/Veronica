//! `vr emoji` — the emoji picker from the terminal.
//!
//! Edith's `ed emoji` searches the same catalogue and copies the result. The
//! ranking, the skin tones and the recents ledger are shared verbatim, so a
//! search here returns what the picker would show.
//!
//! `copy` records the pick in the same ledger the picker ranks by, so reaching
//! for an emoji from a script and reaching for it from the panel both count.

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use veronica_core::emoji::{Catalog, SkinTone, UsageLedger};
use veronica_core::{AppDirectories, Settings};

use crate::format::{self, Output};

#[derive(clap::Subcommand)]
pub enum EmojiCommand {
    /// Search the catalogue. With no query, the whole catalogue in order.
    #[command(alias = "search")]
    List {
        /// What to look for: a name, a word in one, or a search term.
        #[arg(default_value = "")]
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Restrict to one group id, e.g. `flags`.
        #[arg(long)]
        group: Option<String>,
        /// Which skin tone to print. Defaults to the configured tone.
        #[arg(long)]
        tone: Option<String>,
    },
    /// Print the groups the catalogue is divided into.
    Groups,
    /// Copy one emoji to the clipboard and record the pick.
    ///
    /// The argument is either the character itself or a search: the best match
    /// is what gets copied, which is what makes `vr emoji copy shrug` useful.
    Copy {
        query: String,
        #[arg(long)]
        tone: Option<String>,
        /// Record the pick without touching the clipboard.
        #[arg(long)]
        no_copy: bool,
    },
    /// The emoji you reach for most, as the picker ranks them.
    Recents {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Forget one emoji's usage, or all of it with `--all`.
    Forget {
        character: Option<String>,
        #[arg(long)]
        all: bool,
    },
}

fn parse_tone(raw: &str) -> Result<SkinTone> {
    let parsed = SkinTone::parse(raw);
    // `parse` falls back to the default, which is right for a stored setting
    // but wrong for an explicit flag: a typo should be reported.
    if parsed == SkinTone::Standard && !raw.eq_ignore_ascii_case("standard") {
        let known: Vec<&str> = SkinTone::ALL.iter().map(|tone| tone.key()).collect();
        anyhow::bail!("unknown skin tone '{raw}'; try one of {}", known.join(", "));
    }
    Ok(parsed)
}

pub async fn run(
    directories: &AppDirectories,
    settings: &Settings,
    command: &EmojiCommand,
    output: Output,
) -> Result<()> {
    let catalog = Catalog::bundled();
    let path = directories.emoji_usage_file();
    let configured = SkinTone::parse(settings.string("emojiSkinTone").unwrap_or("standard"));

    match command {
        EmojiCommand::List {
            query,
            limit,
            group,
            tone,
        } => {
            let tone = match tone {
                Some(raw) => parse_tone(raw)?,
                None => configured,
            };

            let group_index = match group {
                Some(id) => Some(
                    catalog
                        .groups
                        .iter()
                        .position(|entry| entry.id.eq_ignore_ascii_case(id))
                        .with_context(|| {
                            let known: Vec<&str> =
                                catalog.groups.iter().map(|g| g.id.as_str()).collect();
                            format!("unknown group '{id}'; try one of {}", known.join(", "))
                        })?,
                ),
                None => None,
            };

            // Search first, then narrow: filtering by group before ranking
            // would change which results win, and the ranking is the point.
            let matches: Vec<_> = catalog
                .search(query, usize::MAX)
                .into_iter()
                .filter(|emoji| group_index.is_none_or(|index| emoji.group_index == index))
                .take(*limit)
                .collect();

            let rows: Vec<serde_json::Value> = matches
                .iter()
                .map(|emoji| {
                    json!({
                        "character": emoji.character(tone),
                        "name": emoji.name,
                        "group": catalog.groups[emoji.group_index].id,
                        "terms": emoji.terms,
                        "supportsSkinTones": emoji.supports_skin_tones(),
                    })
                })
                .collect();

            output.emit(&rows, || {
                if matches.is_empty() {
                    return format!("nothing matches {query:?}");
                }
                let table: Vec<Vec<String>> = matches
                    .iter()
                    .map(|emoji| {
                        vec![
                            emoji.character(tone).to_string(),
                            emoji.name.clone(),
                            catalog.groups[emoji.group_index].name.clone(),
                        ]
                    })
                    .collect();
                format::table(&["", "name", "group"], &table)
            })
        }

        EmojiCommand::Groups => output.emit(&catalog.groups, || {
            let rows: Vec<Vec<String>> = catalog
                .groups
                .iter()
                .enumerate()
                .map(|(index, group)| {
                    vec![
                        group.id.clone(),
                        group.name.clone(),
                        catalog.group(index).len().to_string(),
                    ]
                })
                .collect();
            format::table(&["id", "name", "count"], &rows)
        }),

        EmojiCommand::Copy {
            query,
            tone,
            no_copy,
        } => {
            let tone = match tone {
                Some(raw) => parse_tone(raw)?,
                None => configured,
            };

            // An exact character wins over a search, so copying an emoji you
            // already have in hand never resolves to something else.
            let emoji = catalog
                .find(query)
                .or_else(|| catalog.search(query, 1).into_iter().next())
                .with_context(|| format!("nothing matches {query:?}"))?;
            let value = emoji.character(tone).to_string();

            let mut ledger = UsageLedger::load(&path)?;
            ledger.record(&emoji.character, Utc::now().timestamp_millis());
            ledger.save(&path)?;

            let copied = if *no_copy {
                None
            } else {
                match veronica_system::selection::write(&value).await {
                    Ok(writer) => Some(writer.title()),
                    Err(error) => {
                        // The pick is already recorded; a clipboard failure must
                        // not make the command look like it did nothing.
                        tracing::warn!(target: "veronica", "cannot copy the emoji: {error:#}");
                        None
                    }
                }
            };

            output.emit(
                &json!({
                    "character": value,
                    "name": emoji.name,
                    "tone": tone.key(),
                    "copiedVia": copied,
                }),
                || match copied {
                    Some(via) => format!("{value}  {}  (copied via {via})", emoji.name),
                    None => format!("{value}  {}", emoji.name),
                },
            )
        }

        EmojiCommand::Recents { limit } => {
            let ledger = UsageLedger::load(&path)?;
            let now = Utc::now().timestamp_millis();
            let ranked = ledger.ranked(now, *limit);
            let rows: Vec<serde_json::Value> = ranked
                .iter()
                .map(|character| {
                    let usage = ledger
                        .entries
                        .iter()
                        .find(|usage| &usage.character == character);
                    json!({
                        "character": character,
                        "name": catalog.find(character).map(|emoji| emoji.name.clone()),
                        "count": usage.map(|usage| usage.count).unwrap_or(0),
                    })
                })
                .collect();
            output.emit(&rows, || {
                if ranked.is_empty() {
                    return "no emoji picked yet".to_string();
                }
                let table: Vec<Vec<String>> = ranked
                    .iter()
                    .map(|character| {
                        vec![
                            character.clone(),
                            catalog
                                .find(character)
                                .map(|emoji| emoji.name.clone())
                                .unwrap_or_default(),
                            ledger
                                .entries
                                .iter()
                                .find(|usage| &usage.character == character)
                                .map(|usage| usage.count.to_string())
                                .unwrap_or_else(|| "0".to_string()),
                        ]
                    })
                    .collect();
                format::table(&["", "name", "picks"], &table)
            })
        }

        EmojiCommand::Forget { character, all } => {
            let mut ledger = UsageLedger::load(&path)?;
            match (character, all) {
                (Some(_), true) => anyhow::bail!("pass either an emoji or --all, not both"),
                (None, false) => anyhow::bail!("which emoji? pass one, or --all"),
                (Some(character), false) => {
                    ledger.forget(character);
                    ledger.save(&path)?;
                    output.emit(&json!({ "forgot": character }), || {
                        format!("forgot {character}")
                    })
                }
                (None, true) => {
                    let count = ledger.entries.len();
                    ledger.clear();
                    ledger.save(&path)?;
                    output.emit(&json!({ "forgot": count }), || {
                        format!("forgot {count} emoji")
                    })
                }
            }
        }
    }
}
