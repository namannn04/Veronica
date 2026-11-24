use tauri::{AppHandle, Manager, State};

use super::{fail, CommandResult};
use crate::state::AppState;

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn emoji_groups(state: State<'_, AppState>) -> Vec<veronica_core::emoji::Group> {
    state.emoji.groups.clone()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn emoji_search(
    state: State<'_, AppState>,
    query: String,
    group: Option<usize>,
    limit: usize,
) -> Vec<EmojiRow> {
    let tone = state.emoji_tone();
    state
        .emoji
        .search(&query, usize::MAX)
        .into_iter()
        .filter(|emoji| group.is_none_or(|index| emoji.group_index == index))
        .take(limit)
        .map(|emoji| EmojiRow::new(emoji, tone))
        .collect()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive State, AppHandle and deserialized arguments by value"
)]
pub(crate) fn emoji_recents(
    state: State<'_, AppState>,
    limit: usize,
) -> CommandResult<Vec<EmojiRow>> {
    let ledger = veronica_core::emoji::UsageLedger::load(&state.directories.emoji_usage_file())
        .map_err(fail)?;
    let tone = state.emoji_tone();
    Ok(ledger
        .ranked(chrono::Utc::now().timestamp_millis(), limit)
        .into_iter()
        .filter_map(|character| {
            state
                .emoji
                .find(&character)
                .map(|emoji| EmojiRow::new(emoji, tone))
        })
        .collect())
}

#[tauri::command]
pub(crate) async fn emoji_copy(
    app: AppHandle,
    window: tauri::WebviewWindow,
    character: String,
    insert: bool,
) -> CommandResult<EmojiCopyResult> {
    let (path, base) = {
        let state = app.state::<AppState>();
        let base = state
            .emoji
            .emoji()
            .iter()
            .find(|emoji| emoji.character(state.emoji_tone()) == character)
            .map_or_else(|| character.clone(), |emoji| emoji.character.clone());
        (state.directories.emoji_usage_file(), base)
    };

    let mut ledger = veronica_core::emoji::UsageLedger::load(&path).map_err(fail)?;
    ledger.record(&base, chrono::Utc::now().timestamp_millis());
    ledger.save(&path).map_err(fail)?;

    let copied_via = veronica_system::selection::write(&character)
        .await
        .map(|writer| writer.title().to_string())
        .map_err(fail)?;

    let inserted = if insert {
        let result = if uses_emoji_target(window.label(), insert) {
            veronica_system::selection::paste_from_picker().await
        } else {
            veronica_system::selection::paste_in_place().await
        };
        match result {
            Ok(()) => true,
            Err(error) => {
                tracing::info!(target: "veronica", "cannot insert the emoji in place: {error:#}");
                false
            }
        }
    } else {
        false
    };

    Ok(EmojiCopyResult {
        character,
        copied_via,
        inserted,
    })
}

fn uses_emoji_target(window_label: &str, insert: bool) -> bool {
    insert && window_label == crate::emoji_picker::LABEL
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmojiRow {
    pub character: String,
    pub name: String,
    pub group_index: usize,
    pub supports_skin_tones: bool,
}

impl EmojiRow {
    fn new(emoji: &veronica_core::Emoji, tone: veronica_core::SkinTone) -> Self {
        Self {
            character: emoji.character(tone).to_string(),
            name: emoji.name.clone(),
            group_index: emoji.group_index,
            supports_skin_tones: emoji.supports_skin_tones(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmojiCopyResult {
    pub character: String,
    pub copied_via: String,
    pub inserted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_picker_uses_the_shell_target_without_hiding() {
        assert!(uses_emoji_target(crate::emoji_picker::LABEL, true));
        assert!(!uses_emoji_target("main", true));
        assert!(!uses_emoji_target(crate::emoji_picker::LABEL, false));
    }
}
