use std::collections::BTreeMap;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use veronica_core::settings::Settings;
use veronica_protocol::Manifest;

use crate::layout::Layout;
use crate::registry::{self, Origin};

pub const OVERRIDES_KEY: &str = "shortcutOverrides";
pub const HOST_OWNER: &str = "veronica";
pub const SHOW: &str = "show";
pub const CLEAN_KEYS: &str = "cleanKeys";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Declared {
    pub owner: String,
    pub origin: Option<Origin>,
    pub id: String,
    pub title: String,
    pub default: Option<String>,
    pub run: Vec<String>,
}

impl Declared {
    fn new(owner: &str, origin: Option<Origin>, id: &str, title: &str, default: &str) -> Self {
        Self {
            owner: owner.to_string(),
            origin,
            id: id.to_string(),
            title: title.to_string(),
            default: Some(default.to_string()),
            run: Vec::new(),
        }
    }

    #[must_use]
    pub fn key(&self) -> String {
        format!("{}/{}", self.owner, self.id)
    }

    #[must_use]
    pub fn is_host(&self) -> bool {
        self.origin.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Active,
    Unbound,
    Invalid,
    Conflict,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Planned {
    #[serde(flatten)]
    pub declared: Declared,
    pub binding: Option<String>,
    pub overridden: bool,
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflicts_with: Option<String>,
}

#[must_use]
pub fn host_shortcuts() -> Vec<Declared> {
    vec![
        Declared::new(HOST_OWNER, None, SHOW, "Show Veronica", "Ctrl+Alt+V"),
        Declared::new(HOST_OWNER, None, CLEAN_KEYS, "Clean keys", "Ctrl+Alt+K"),
    ]
}

#[must_use]
pub fn builtin_shortcuts(extension: &str) -> Vec<Declared> {
    let (id, title, default) = match extension {
        "clipboard" => ("open", "Clipboard", "Ctrl+Alt+B"),
        "micMute" => ("toggle", "Microphone mute", "Ctrl+Alt+M"),
        "colorPicker" => ("pick", "Pick color", "Ctrl+Alt+P"),
        "emoji" => ("open", "Emoji", "Ctrl+Alt+E"),
        _ => return Vec::new(),
    };
    vec![Declared::new(
        extension,
        Some(Origin::Builtin),
        id,
        title,
        default,
    )]
}

#[must_use]
pub fn from_manifest(manifest: &Manifest, origin: Origin) -> Vec<Declared> {
    manifest
        .shortcuts
        .iter()
        .map(|shortcut| Declared {
            owner: manifest.id.clone(),
            origin: Some(origin),
            id: shortcut.id.clone(),
            title: shortcut.title.clone(),
            default: shortcut.default.clone(),
            run: shortcut.run.clone(),
        })
        .collect()
}

pub fn declared(layout: &Layout, settings: &Settings) -> Result<Vec<Declared>> {
    let registry = registry::view(&layout.installed_file(), settings)?;
    let mut declared = host_shortcuts();
    for (id, entry) in &registry.extensions {
        if !entry.enabled {
            continue;
        }
        if entry.origin == Origin::Builtin {
            declared.extend(builtin_shortcuts(id));
            continue;
        }
        let path = layout.current_link(id).join("manifest.json");
        let manifest = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| Manifest::parse(&raw).ok());
        if let Some(manifest) = manifest {
            declared.extend(from_manifest(&manifest, entry.origin));
        }
    }
    Ok(declared)
}

const MODIFIERS: [(&str, &[&str]); 4] = [
    ("Ctrl", &["ctrl", "control"]),
    ("Alt", &["alt", "option"]),
    ("Shift", &["shift"]),
    ("Super", &["super", "meta", "cmd", "command", "logo"]),
];

const NAMED_KEYS: [(&str, &[&str]); 17] = [
    ("Space", &["space"]),
    ("Enter", &["enter", "return"]),
    ("Tab", &["tab"]),
    ("Escape", &["escape", "esc"]),
    ("Backspace", &["backspace"]),
    ("Delete", &["delete", "del"]),
    ("Insert", &["insert"]),
    ("Home", &["home"]),
    ("End", &["end"]),
    ("PageUp", &["pageup"]),
    ("PageDown", &["pagedown"]),
    ("ArrowUp", &["up", "arrowup"]),
    ("ArrowDown", &["down", "arrowdown"]),
    ("ArrowLeft", &["left", "arrowleft"]),
    ("ArrowRight", &["right", "arrowright"]),
    ("Comma", &["comma", ","]),
    ("Period", &["period", "."]),
];

fn key_name(token: &str) -> Option<String> {
    let lower = token.to_ascii_lowercase();
    if lower.len() == 1 && lower.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Some(lower.to_ascii_uppercase());
    }
    if let Some(number) = lower.strip_prefix('f') {
        if number
            .parse::<u8>()
            .is_ok_and(|number| (1..=24).contains(&number))
        {
            return Some(format!("F{number}"));
        }
    }
    NAMED_KEYS
        .iter()
        .find(|(_, aliases)| aliases.contains(&lower.as_str()))
        .map(|(name, _)| (*name).to_string())
}

#[must_use]
pub fn normalize(accelerator: &str) -> Option<String> {
    let mut held = [false; MODIFIERS.len()];
    let mut key = None;
    for token in accelerator.split('+').map(str::trim) {
        let lower = token.to_ascii_lowercase();
        if let Some(index) = MODIFIERS
            .iter()
            .position(|(_, aliases)| aliases.contains(&lower.as_str()))
        {
            if held[index] {
                return None;
            }
            held[index] = true;
        } else if key.is_none() {
            key = Some(key_name(token)?);
        } else {
            return None;
        }
    }
    let shift = MODIFIERS
        .iter()
        .position(|(name, _)| *name == "Shift")
        .unwrap_or_default();
    let anchored = held
        .iter()
        .enumerate()
        .any(|(index, held)| *held && index != shift);
    if !anchored {
        return None;
    }
    let mut parts: Vec<String> = MODIFIERS
        .iter()
        .zip(held)
        .filter(|(_, held)| *held)
        .map(|((name, _), _)| (*name).to_string())
        .collect();
    parts.push(key?);
    Some(parts.join("+"))
}

enum Choice {
    Default(Option<String>),
    Override(Option<String>),
    Invalid,
}

fn choice(declared: &Declared, overrides: Option<&serde_json::Map<String, Value>>) -> Choice {
    match overrides.and_then(|overrides| overrides.get(&declared.key())) {
        Some(Value::Null) => Choice::Override(None),
        Some(Value::String(text)) => {
            normalize(text).map_or(Choice::Invalid, |binding| Choice::Override(Some(binding)))
        }
        Some(_) => Choice::Invalid,
        None => declared
            .default
            .as_deref()
            .map_or(Choice::Default(None), |text| {
                normalize(text).map_or(Choice::Invalid, |binding| Choice::Default(Some(binding)))
            }),
    }
}

fn rank(declared: &Declared, choice: &Choice) -> u8 {
    match (declared.is_host(), choice) {
        (true, _) => 0,
        (false, Choice::Override(_)) => 1,
        (false, _) => 2,
    }
}

fn listed(declared: &Declared) -> (bool, String) {
    (!declared.is_host(), declared.owner.clone())
}

#[must_use]
pub fn plan(declared: Vec<Declared>, settings: &Settings) -> Vec<Planned> {
    let overrides = settings.get(OVERRIDES_KEY).and_then(Value::as_object);
    let mut ordered: Vec<(Declared, Choice)> = declared
        .into_iter()
        .map(|declared| {
            let choice = choice(&declared, overrides);
            (declared, choice)
        })
        .collect();
    ordered.sort_by_cached_key(|(declared, choice)| (rank(declared, choice), listed(declared)));
    let mut claimed: BTreeMap<String, String> = BTreeMap::new();
    let mut planned: Vec<Planned> = ordered
        .into_iter()
        .map(|(declared, choice)| {
            let (binding, overridden, mut state) = match choice {
                Choice::Default(binding) => {
                    let state = if binding.is_some() {
                        State::Active
                    } else {
                        State::Unbound
                    };
                    (binding, false, state)
                }
                Choice::Override(binding) => {
                    let state = if binding.is_some() {
                        State::Active
                    } else {
                        State::Unbound
                    };
                    (binding, true, state)
                }
                Choice::Invalid => (None, true, State::Invalid),
            };
            let mut conflicts_with = None;
            if let Some(binding) = &binding {
                if let Some(owner) = claimed.get(binding) {
                    state = State::Conflict;
                    conflicts_with = Some(owner.clone());
                } else {
                    claimed.insert(binding.clone(), declared.title.clone());
                }
            }
            Planned {
                declared,
                binding,
                overridden,
                state,
                conflicts_with,
            }
        })
        .collect();
    planned.sort_by_cached_key(|planned| listed(&planned.declared));
    planned
}

pub fn current(layout: &Layout, settings: &Settings) -> Result<Vec<Planned>> {
    Ok(plan(declared(layout, settings)?, settings))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use veronica_core::settings::Settings;

    use super::{builtin_shortcuts, host_shortcuts, normalize, plan, Declared, State};
    use crate::registry::Origin;

    fn extension(owner: &str, id: &str, default: Option<&str>) -> Declared {
        Declared {
            owner: owner.to_string(),
            origin: Some(Origin::Catalog),
            id: id.to_string(),
            title: format!("{owner} {id}"),
            default: default.map(str::to_string),
            run: vec![owner.to_string(), id.to_string()],
        }
    }

    fn summary(
        settings: &Settings,
        declared: Vec<Declared>,
    ) -> Vec<(String, Option<String>, State)> {
        plan(declared, settings)
            .into_iter()
            .map(|planned| (planned.declared.key(), planned.binding, planned.state))
            .collect()
    }

    #[test]
    fn accelerators_normalize_to_one_spelling() {
        assert_eq!(normalize("ctrl+alt+v").as_deref(), Some("Ctrl+Alt+V"));
        assert_eq!(
            normalize("Alt + Control + p").as_deref(),
            Some("Ctrl+Alt+P")
        );
        assert_eq!(
            normalize("Super+Shift+f12").as_deref(),
            Some("Shift+Super+F12")
        );
        assert_eq!(normalize("Meta+space").as_deref(), Some("Super+Space"));
        assert_eq!(normalize("Shift+A"), None);
        assert_eq!(normalize("A"), None);
        assert_eq!(normalize("Ctrl+Ctrl+A"), None);
        assert_eq!(normalize("Ctrl+A+B"), None);
        assert_eq!(normalize("Ctrl+Alt"), None);
        assert_eq!(normalize("Ctrl+F25"), None);
        assert_eq!(normalize("Ctrl+Hyper"), None);
    }

    #[test]
    fn host_shortcuts_always_win_over_extensions() {
        let mut declared = vec![extension("media", "toggle", Some("Ctrl+Alt+V"))];
        declared.extend(host_shortcuts());
        let planned = plan(declared, &Settings::default());
        assert_eq!(planned[0].declared.id, "show");
        assert_eq!(planned[0].state, State::Active);
        let media = planned
            .iter()
            .find(|p| p.declared.owner == "media")
            .unwrap();
        assert_eq!(media.state, State::Conflict);
        assert_eq!(media.conflicts_with.as_deref(), Some("Show Veronica"));
    }

    #[test]
    fn extensions_conflict_in_id_order_and_overrides_take_precedence() {
        let declared = vec![
            extension("zeta", "go", Some("ctrl+alt+z")),
            extension("alpha", "go", Some("Ctrl+Alt+Z")),
        ];
        assert_eq!(
            summary(&Settings::default(), declared.clone()),
            [
                (
                    "alpha/go".to_string(),
                    Some("Ctrl+Alt+Z".to_string()),
                    State::Active
                ),
                (
                    "zeta/go".to_string(),
                    Some("Ctrl+Alt+Z".to_string()),
                    State::Conflict
                ),
            ]
        );
        let mut settings = Settings::default();
        settings.set(
            super::OVERRIDES_KEY,
            json!({ "zeta/go": "Ctrl+Alt+Z", "alpha/go": null }),
        );
        assert_eq!(
            summary(&settings, declared.clone()),
            [
                ("alpha/go".to_string(), None, State::Unbound),
                (
                    "zeta/go".to_string(),
                    Some("Ctrl+Alt+Z".to_string()),
                    State::Active
                ),
            ]
        );
        settings.set(super::OVERRIDES_KEY, json!({ "alpha/go": "Ctrl+Alt+A" }));
        assert_eq!(
            summary(&settings, declared.clone()),
            [
                (
                    "alpha/go".to_string(),
                    Some("Ctrl+Alt+A".to_string()),
                    State::Active
                ),
                (
                    "zeta/go".to_string(),
                    Some("Ctrl+Alt+Z".to_string()),
                    State::Active
                ),
            ]
        );
        settings.set(
            super::OVERRIDES_KEY,
            json!({ "zeta/go": "Hyper+Z", "alpha/go": 3 }),
        );
        let planned = plan(declared, &settings);
        assert!(planned
            .iter()
            .all(|p| p.state == State::Invalid && p.overridden));
    }

    #[test]
    fn a_manifest_without_a_default_stays_unbound() {
        let planned = plan(
            vec![extension("media", "toggle", None)],
            &Settings::default(),
        );
        assert_eq!(planned[0].state, State::Unbound);
        assert!(!planned[0].overridden);
    }

    #[test]
    fn built_in_features_keep_their_bindings() {
        let bindings: Vec<_> = ["clipboard", "micMute", "colorPicker", "emoji", "media"]
            .into_iter()
            .flat_map(builtin_shortcuts)
            .map(|declared| (declared.owner, declared.default.unwrap()))
            .collect();
        assert_eq!(
            bindings,
            [
                ("clipboard".to_string(), "Ctrl+Alt+B".to_string()),
                ("micMute".to_string(), "Ctrl+Alt+M".to_string()),
                ("colorPicker".to_string(), "Ctrl+Alt+P".to_string()),
                ("emoji".to_string(), "Ctrl+Alt+E".to_string()),
            ]
        );
    }
}
