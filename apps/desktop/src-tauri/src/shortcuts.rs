use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use veronica_core::Settings;
use veronica_host::registry::Origin;
use veronica_host::shortcuts::{self, Planned, State, CLEAN_KEYS, SHOW};

use crate::state::AppState;
use crate::{commands, emoji_picker, main_window};

pub(crate) const UPDATED: &str = "shortcuts-updated";
const EMOJI: &str = "emoji";
const EMOJI_PICKER: &str = "open";

#[derive(Default)]
pub(crate) struct Shortcuts {
    plan: Mutex<Vec<Planned>>,
    registered: Mutex<HashMap<u32, Planned>>,
}

impl Shortcuts {
    pub(crate) fn planned(&self) -> Vec<Planned> {
        lock(&self.plan).clone()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn wanted(planned: &mut [Planned]) -> HashMap<u32, (Shortcut, usize)> {
    let mut wanted = HashMap::new();
    for (index, entry) in planned.iter_mut().enumerate() {
        if entry.state != State::Active {
            continue;
        }
        let Some(binding) = entry.binding.as_deref() else {
            continue;
        };
        match binding.parse::<Shortcut>() {
            Ok(shortcut) => {
                wanted.insert(shortcut.id(), (shortcut, index));
            }
            Err(_) => entry.state = State::Invalid,
        }
    }
    wanted
}

pub(crate) fn sync(app: &AppHandle, settings: &Settings) {
    let layout = app.state::<AppState>().extensions.layout().clone();
    let mut planned = shortcuts::current(&layout, settings).unwrap_or_else(|error| {
        tracing::warn!(target: "veronica", "cannot read extension shortcuts: {error:#}");
        shortcuts::plan(shortcuts::host_shortcuts(), settings)
    });
    let wanted = wanted(&mut planned);
    let manager = app.global_shortcut();
    let state = app.state::<Shortcuts>();
    let mut registered = lock(&state.registered);
    registered.retain(|id, entry| {
        if wanted.contains_key(id) {
            return true;
        }
        if let Some(binding) = entry.binding.as_deref() {
            if let Err(error) = manager.unregister(binding) {
                tracing::warn!(target: "veronica", "cannot unregister {binding}: {error}");
            }
        }
        false
    });
    for (id, (shortcut, index)) in wanted {
        if !registered.contains_key(&id) {
            if let Err(error) = manager.register(shortcut) {
                tracing::warn!(
                    target: "veronica",
                    "cannot register {}: {error}",
                    planned[index].binding.as_deref().unwrap_or_default()
                );
                planned[index].state = State::Unavailable;
                continue;
            }
        }
        registered.insert(id, planned[index].clone());
    }
    drop(registered);
    *lock(&state.plan) = planned;
    let _ = app.emit(UPDATED, ());
}

fn show_route(app: &AppHandle, route: &str) {
    if let Err(error) = main_window::show(app, Some(route)) {
        tracing::warn!(target: "veronica", "cannot open {route}: {error:#}");
    }
}

fn shell_method(method: &'static str) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = commands::call_shell_method(method).await {
            tracing::warn!(target: "veronica", "the {method} shortcut failed: {error}");
        }
    });
}

fn built_in(app: &AppHandle, extension: &str) {
    match extension {
        "clipboard" => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                if commands::call_shell_method("ShowClipboard").await.is_err() {
                    show_route(&handle, "ext:clipboard/main");
                }
            });
        }
        "micMute" => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                match veronica_system::audio::toggle_microphone().await {
                    Ok(state) => {
                        let _ = handle.emit("microphone-updated", state);
                    }
                    Err(error) => {
                        tracing::warn!(target: "veronica", "mic shortcut failed: {error:#}");
                    }
                }
            });
        }
        _ => {}
    }
}

fn host(app: &AppHandle, id: &str) {
    match id {
        SHOW => {
            if let Err(error) = main_window::show(app, None) {
                tracing::warn!(target: "veronica", "cannot open Veronica: {error:#}");
            }
        }
        CLEAN_KEYS => shell_method("CleanKeys"),
        _ => {}
    }
}

fn extension(app: &AppHandle, planned: Planned) {
    if planned.declared.owner == EMOJI && planned.declared.id == EMOJI_PICKER {
        if let Err(error) = emoji_picker::show(app) {
            tracing::warn!(target: "veronica", "cannot open the emoji picker: {error:#}");
        }
        return;
    }
    let host = std::sync::Arc::clone(&app.state::<AppState>().extensions);
    tauri::async_runtime::spawn(async move {
        let declared = planned.declared;
        if let Err(error) = host
            .router()
            .shortcut(&declared.owner, &declared.id, declared.run)
            .await
        {
            tracing::warn!(
                target: "veronica",
                "the {} shortcut of {} failed: {error}",
                declared.id,
                declared.owner
            );
        }
    });
}

pub(crate) fn fire(app: &AppHandle, shortcut: &Shortcut) {
    let Some(planned) = lock(&app.state::<Shortcuts>().registered)
        .get(&shortcut.id())
        .cloned()
    else {
        return;
    };
    match planned.declared.origin {
        None => host(app, &planned.declared.id),
        Some(Origin::Builtin) => built_in(app, &planned.declared.owner),
        Some(_) => extension(app, planned),
    }
}

#[cfg(test)]
mod tests {
    use veronica_host::shortcuts::{host_shortcuts, normalize, plan, Declared, State};

    use super::wanted;

    #[test]
    fn every_normalized_spelling_parses_as_a_global_shortcut() {
        let spellings = [
            "ctrl+alt+v",
            "Super+Shift+F12",
            "Ctrl+Alt+Space",
            "Alt+Enter",
            "Ctrl+Escape",
            "Ctrl+PageDown",
            "Ctrl+Alt+Left",
            "Super+Comma",
            "Ctrl+Alt+7",
            "Ctrl+Delete",
        ];
        let declared: Vec<Declared> = spellings
            .iter()
            .enumerate()
            .map(|(index, spelling)| Declared {
                owner: "probe".to_string(),
                origin: Some(veronica_host::registry::Origin::Sideload),
                id: format!("key{index}"),
                title: (*spelling).to_string(),
                default: Some(normalize(spelling).expect(spelling)),
                run: vec!["probe".to_string()],
            })
            .collect();
        let mut planned = plan(declared, &veronica_core::Settings::default());
        let wanted = wanted(&mut planned);
        assert_eq!(wanted.len(), spellings.len());
        assert!(planned.iter().all(|entry| entry.state == State::Active));
    }

    #[test]
    fn host_shortcuts_register_under_distinct_ids() {
        let mut planned = plan(host_shortcuts(), &veronica_core::Settings::default());
        assert_eq!(wanted(&mut planned).len(), 2);
    }
}
