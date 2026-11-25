use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use veronica_core::fsutil::write_atomic;
use veronica_protocol::manifest::{Manifest, SurfaceAction, SurfaceTemplate};

pub const MAX_STATE_BYTES: usize = 16 * 1024;
pub const MAX_LIST_ITEMS: usize = 20;
pub const MAX_HEATMAP_CELLS: usize = 400;
const MAX_HEAT_LEVEL: u64 = 4;
const MAX_TEXT_CHARS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Placement {
    Home,
    Notch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Surface {
    pub extension: String,
    pub id: String,
    pub placement: Placement,
    pub template: SurfaceTemplate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens: Option<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blur_category: Option<String>,
    #[serde(default)]
    pub actions: Vec<SurfaceAction>,
    pub state: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SurfaceError {
    #[error("{extension} declares no home card or notch activity named {surface}")]
    Undeclared { extension: String, surface: String },
    #[error("the state of {surface} is larger than 16 KiB")]
    TooLarge { surface: String },
    #[error("the state of {surface} does not fit the {template} template: {reason}")]
    Shape {
        surface: String,
        template: &'static str,
        reason: String,
    },
}

pub fn template_name(template: SurfaceTemplate) -> &'static str {
    match template {
        SurfaceTemplate::Ring => "ring",
        SurfaceTemplate::Meter => "meter",
        SurfaceTemplate::Stat => "stat",
        SurfaceTemplate::Text => "text",
        SurfaceTemplate::List => "list",
        SurfaceTemplate::NowPlaying => "nowPlaying",
        SurfaceTemplate::Actions => "actions",
        SurfaceTemplate::Heatmap => "heatmap",
    }
}

fn text(state: &Value, key: &str, required: bool) -> Result<(), String> {
    match state.get(key) {
        None | Some(Value::Null) if !required => Ok(()),
        Some(Value::String(value)) if value.chars().count() <= MAX_TEXT_CHARS => Ok(()),
        Some(Value::String(_)) => Err(format!("{key} is longer than {MAX_TEXT_CHARS} characters")),
        _ => Err(format!("{key} must be text")),
    }
}

fn fraction(state: &Value, key: &str) -> Result<(), String> {
    match state.get(key).and_then(Value::as_f64) {
        Some(value) if (0.0..=1.0).contains(&value) => Ok(()),
        _ => Err(format!("{key} must be a number from 0 to 1")),
    }
}

fn flag(state: &Value, key: &str) -> Result<(), String> {
    match state.get(key) {
        None | Some(Value::Null | Value::Bool(_)) => Ok(()),
        _ => Err(format!("{key} must be true or false")),
    }
}

fn list(state: &Value) -> Result<(), String> {
    let items = state
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| "items must be a list".to_string())?;
    if items.len() > MAX_LIST_ITEMS {
        return Err(format!("items holds more than {MAX_LIST_ITEMS} entries"));
    }
    for item in items {
        text(item, "title", true)?;
        text(item, "detail", false)?;
    }
    Ok(())
}

fn is_day(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

fn heatmap(state: &Value) -> Result<(), String> {
    let cells = state
        .get("cells")
        .and_then(Value::as_array)
        .ok_or_else(|| "cells must be a list".to_string())?;
    if cells.len() > MAX_HEATMAP_CELLS {
        return Err(format!("cells holds more than {MAX_HEATMAP_CELLS} days"));
    }
    for cell in cells {
        if !cell
            .get("period")
            .and_then(Value::as_str)
            .is_some_and(is_day)
        {
            return Err("each cell needs a period written as YYYY-MM-DD".into());
        }
        if cell
            .get("level")
            .and_then(Value::as_u64)
            .is_none_or(|level| level > MAX_HEAT_LEVEL)
        {
            return Err(format!(
                "each cell needs a level from 0 to {MAX_HEAT_LEVEL}"
            ));
        }
        for key in ["cost", "tokens"] {
            match cell.get(key) {
                None | Some(Value::Number(_)) => {}
                _ => return Err(format!("{key} must be a number")),
            }
        }
    }
    text(state, "caption", false)
}

pub fn check_state(template: SurfaceTemplate, state: &Value) -> Result<(), String> {
    if !state.is_object() {
        return Err("the state must be an object".into());
    }
    match template {
        SurfaceTemplate::Ring | SurfaceTemplate::Meter => {
            fraction(state, "value")?;
            text(state, "label", false)?;
            text(state, "caption", false)
        }
        SurfaceTemplate::Stat => {
            match state.get("value") {
                Some(Value::Number(_)) => {}
                Some(Value::String(_)) => text(state, "value", true)?,
                _ => return Err("value must be text or a number".into()),
            }
            text(state, "label", false)?;
            text(state, "caption", false)
        }
        SurfaceTemplate::Text => text(state, "text", true),
        SurfaceTemplate::List => list(state),
        SurfaceTemplate::NowPlaying => {
            text(state, "title", true)?;
            text(state, "artist", false)?;
            text(state, "album", false)?;
            flag(state, "playing")
        }
        SurfaceTemplate::Actions => text(state, "caption", false),
        SurfaceTemplate::Heatmap => heatmap(state),
    }
}

pub fn declared(manifest: &Manifest, surface: &str, state: &Value) -> Vec<Surface> {
    let home = manifest
        .home_cards
        .iter()
        .filter(|card| card.id == surface)
        .map(|card| Surface {
            extension: manifest.id.clone(),
            id: card.id.clone(),
            placement: Placement::Home,
            template: card.template,
            title: Some(card.title.clone()),
            opens: card.opens.clone(),
            priority: 0,
            blur_category: None,
            actions: card.actions.clone(),
            state: state.clone(),
        });
    let notch = manifest
        .notch_activities
        .iter()
        .filter(|activity| activity.id == surface)
        .map(|activity| Surface {
            extension: manifest.id.clone(),
            id: activity.id.clone(),
            placement: Placement::Notch,
            template: activity.template,
            title: None,
            opens: None,
            priority: activity.priority,
            blur_category: activity.blur_category.clone(),
            actions: activity.actions.clone(),
            state: state.clone(),
        });
    home.chain(notch).collect()
}

pub fn validate(
    manifest: &Manifest,
    surface: &str,
    state: &Value,
) -> Result<Vec<Surface>, SurfaceError> {
    let surfaces = declared(manifest, surface, state);
    if surfaces.is_empty() {
        return Err(SurfaceError::Undeclared {
            extension: manifest.id.clone(),
            surface: surface.to_string(),
        });
    }
    if state.is_null() {
        return Ok(surfaces);
    }
    if state.to_string().len() > MAX_STATE_BYTES {
        return Err(SurfaceError::TooLarge {
            surface: surface.to_string(),
        });
    }
    for declared in &surfaces {
        check_state(declared.template, state).map_err(|reason| SurfaceError::Shape {
            surface: surface.to_string(),
            template: template_name(declared.template),
            reason,
        })?;
    }
    Ok(surfaces)
}

type Key = (String, String, Placement);

#[derive(Debug)]
pub struct SurfaceStore {
    file: PathBuf,
    surfaces: Mutex<BTreeMap<Key, Surface>>,
}

impl SurfaceStore {
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            surfaces: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn update(
        &self,
        manifest: &Manifest,
        surface: &str,
        state: &Value,
    ) -> Result<bool, SurfaceError> {
        let surfaces = validate(manifest, surface, state)?;
        let mut stored = self.surfaces.lock().unwrap_or_else(PoisonError::into_inner);
        let mut changed = false;
        for item in surfaces {
            let key = (item.extension.clone(), item.id.clone(), item.placement);
            if state.is_null() {
                changed |= stored.remove(&key).is_some();
            } else if stored.get(&key) != Some(&item) {
                stored.insert(key, item);
                changed = true;
            }
        }
        if changed {
            self.save(&stored);
        }
        Ok(changed)
    }

    pub fn remove_extension(&self, extension: &str) -> bool {
        let mut stored = self.surfaces.lock().unwrap_or_else(PoisonError::into_inner);
        let before = stored.len();
        stored.retain(|(owner, _, _), _| owner != extension);
        let changed = stored.len() != before;
        if changed {
            self.save(&stored);
        }
        changed
    }

    pub fn list(&self) -> Vec<Surface> {
        let stored = self.surfaces.lock().unwrap_or_else(PoisonError::into_inner);
        ordered(stored.values().cloned().collect())
    }

    pub fn for_extension(&self, extension: &str) -> Vec<Surface> {
        self.list()
            .into_iter()
            .filter(|surface| surface.extension == extension)
            .collect()
    }

    fn save(&self, stored: &BTreeMap<Key, Surface>) {
        let list: Vec<&Surface> = stored.values().collect();
        if let Ok(body) = serde_json::to_vec_pretty(&list) {
            if let Some(parent) = self.file.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = write_atomic(&self.file, &body);
        }
    }
}

fn ordered(mut surfaces: Vec<Surface>) -> Vec<Surface> {
    surfaces.sort_by(|a, b| {
        a.placement
            .cmp(&b.placement)
            .then(b.priority.cmp(&a.priority))
            .then_with(|| a.extension.cmp(&b.extension))
            .then_with(|| a.id.cmp(&b.id))
    });
    surfaces
}

pub fn load(file: &Path) -> Vec<Surface> {
    fs::read(file)
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .map(ordered)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn manifest() -> Manifest {
        serde_json::from_value(json!({
            "schema": 1,
            "id": "media",
            "version": "1.0.0",
            "title": "Music",
            "summary": "Players",
            "category": "media",
            "engines": { "host": ">=0.1.0", "hostApi": 1 },
            "targets": ["linux-x86_64"],
            "homeCards": [
                { "id": "now-playing", "template": "nowPlaying", "title": "Now playing", "opens": "main",
                  "actions": [{ "id": "toggle", "label": "Play or pause", "run": ["media", "toggle"] }] },
                { "id": "volume", "template": "meter", "title": "Volume" }
            ],
            "notchActivities": [
                { "id": "now-playing", "template": "nowPlaying", "priority": 50 }
            ]
        }))
        .expect("manifest")
    }

    #[test]
    fn every_template_checks_its_own_shape() {
        let good = [
            (
                SurfaceTemplate::Ring,
                json!({ "value": 0.4, "label": "Session" }),
            ),
            (SurfaceTemplate::Meter, json!({ "value": 1 })),
            (
                SurfaceTemplate::Stat,
                json!({ "value": "$4.20", "label": "Today" }),
            ),
            (SurfaceTemplate::Stat, json!({ "value": 12 })),
            (SurfaceTemplate::Text, json!({ "text": "All quiet" })),
            (
                SurfaceTemplate::List,
                json!({ "items": [{ "title": "a", "detail": "b" }] }),
            ),
            (
                SurfaceTemplate::NowPlaying,
                json!({ "title": "Song", "playing": true }),
            ),
            (SurfaceTemplate::Actions, json!({})),
            (
                SurfaceTemplate::Heatmap,
                json!({ "cells": [{ "period": "2026-11-24", "level": 3, "cost": 4.2, "tokens": 900 }] }),
            ),
        ];
        for (template, state) in good {
            assert_eq!(check_state(template, &state), Ok(()), "{state}");
        }
        let bad = [
            (SurfaceTemplate::Ring, json!({ "value": 1.5 })),
            (SurfaceTemplate::Meter, json!({ "label": "no value" })),
            (SurfaceTemplate::Stat, json!({ "value": [1] })),
            (SurfaceTemplate::Text, json!({ "text": 3 })),
            (
                SurfaceTemplate::List,
                json!({ "items": [{ "detail": "untitled" }] }),
            ),
            (
                SurfaceTemplate::NowPlaying,
                json!({ "title": "Song", "playing": "yes" }),
            ),
            (SurfaceTemplate::Actions, json!("not an object")),
            (
                SurfaceTemplate::Heatmap,
                json!({ "cells": [{ "period": "today", "level": 1 }] }),
            ),
            (
                SurfaceTemplate::Heatmap,
                json!({ "cells": [{ "period": "2026-11-24", "level": 9 }] }),
            ),
        ];
        for (template, state) in bad {
            assert!(check_state(template, &state).is_err(), "{state}");
        }
        let long = json!({ "items": vec![json!({ "title": "x" }); MAX_LIST_ITEMS + 1] });
        assert!(check_state(SurfaceTemplate::List, &long).is_err());
    }

    #[test]
    fn only_declared_surfaces_are_accepted_for_both_placements() {
        let manifest = manifest();
        let state = json!({ "title": "Song" });
        let surfaces = validate(&manifest, "now-playing", &state).expect("declared");
        let placements: Vec<Placement> = surfaces.iter().map(|surface| surface.placement).collect();
        assert_eq!(placements, [Placement::Home, Placement::Notch]);
        assert_eq!(surfaces[0].actions[0].run, ["media", "toggle"]);
        assert!(matches!(
            validate(&manifest, "lyrics", &state),
            Err(SurfaceError::Undeclared { .. })
        ));
        assert!(matches!(
            validate(&manifest, "volume", &state),
            Err(SurfaceError::Shape {
                template: "meter",
                ..
            })
        ));
        let huge = json!({ "title": "x".repeat(MAX_STATE_BYTES) });
        assert!(matches!(
            validate(&manifest, "now-playing", &huge),
            Err(SurfaceError::TooLarge { .. })
        ));
    }

    #[test]
    fn the_store_persists_orders_and_clears_surfaces() {
        let directory = tempfile::tempdir().expect("directory");
        let file = directory.path().join("surfaces.json");
        let store = SurfaceStore::new(file.clone());
        let manifest = manifest();
        assert_eq!(
            store.update(&manifest, "volume", &json!({ "value": 0.5 })),
            Ok(true)
        );
        assert_eq!(
            store.update(&manifest, "volume", &json!({ "value": 0.5 })),
            Ok(false)
        );
        assert_eq!(
            store.update(&manifest, "now-playing", &json!({ "title": "Song" })),
            Ok(true)
        );
        let ids: Vec<(Placement, String)> = load(&file)
            .into_iter()
            .map(|surface| (surface.placement, surface.id))
            .collect();
        assert_eq!(
            ids,
            [
                (Placement::Home, "now-playing".to_string()),
                (Placement::Home, "volume".to_string()),
                (Placement::Notch, "now-playing".to_string()),
            ]
        );
        assert_eq!(
            store.update(&manifest, "now-playing", &Value::Null),
            Ok(true)
        );
        assert_eq!(store.for_extension("media").len(), 1);
        assert!(store.remove_extension("media"));
        assert!(load(&file).is_empty());
        assert!(!store.remove_extension("media"));
    }
}
