//! Settings, stored as one JSON document.
//!
//! Edith uses `UserDefaults`; Ubuntu has no equivalent that both a GUI app and
//! a CLI can share cheaply, so Veronica keeps a single JSON file under
//! `XDG_CONFIG_HOME`. Unknown keys survive a round trip, so a settings file
//! written by a newer build is not destroyed by an older one.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(flatten)]
    values: BTreeMap<String, Value>,
}

impl Settings {
    /// Read the settings file. A missing file is the default state, not an
    /// error, because that is simply a first launch.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("{} is not valid JSON", path.display())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    /// Write atomically. A crash mid-write would otherwise leave a truncated
    /// file that fails to parse on the next launch.
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

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn set(&mut self, key: &str, value: Value) {
        self.values.insert(key.to_string(), value);
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.values.remove(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.values.keys()
    }

    pub fn as_map(&self) -> &BTreeMap<String, Value> {
        &self.values
    }

    /// Boolean read with a default. Extensions are opt-out, so an absent key
    /// means "use the shipped default" rather than "off".
    pub fn bool_or(&self, key: &str, default: bool) -> bool {
        self.values
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(default)
    }

    pub fn string(&self, key: &str) -> Option<&str> {
        self.values.get(key).and_then(Value::as_str)
    }

    pub fn f64_or(&self, key: &str, default: f64) -> f64 {
        self.values
            .get(key)
            .and_then(Value::as_f64)
            .unwrap_or(default)
    }

    /// Whether an extension is enabled, honouring its shipped default.
    pub fn extension_enabled(&self, entry: &crate::extensions::ExtensionEntry) -> bool {
        self.bool_or(entry.defaults_key, entry.featured)
    }

    /// Parse a CLI `key=value` pair into the right JSON type, so
    /// `vr config set preventSleep true` stores a boolean rather than the
    /// string "true".
    pub fn coerce(raw: &str) -> Value {
        let trimmed = raw.trim();
        match trimmed {
            "true" => return Value::Bool(true),
            "false" => return Value::Bool(false),
            "null" => return Value::Null,
            _ => {}
        }
        if let Ok(n) = trimmed.parse::<i64>() {
            return Value::from(n);
        }
        if let Ok(n) = trimmed.parse::<f64>() {
            if n.is_finite() {
                return Value::from(n);
            }
        }
        // Accept a JSON literal so arrays and objects are settable too.
        if trimmed.starts_with(['{', '[', '"']) {
            if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                return value;
            }
        }
        Value::String(raw.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coerces_scalars_to_their_json_types() {
        assert_eq!(Settings::coerce("true"), Value::Bool(true));
        assert_eq!(Settings::coerce("false"), Value::Bool(false));
        assert_eq!(Settings::coerce("42"), Value::from(42));
        assert_eq!(Settings::coerce("0.5"), Value::from(0.5));
        assert_eq!(Settings::coerce("hello"), Value::String("hello".into()));
    }

    #[test]
    fn coerces_json_literals_for_compound_values() {
        assert_eq!(Settings::coerce("[1,2]"), serde_json::json!([1, 2]));
        assert_eq!(Settings::coerce(r#""quoted""#), Value::String("quoted".into()));
    }

    #[test]
    fn a_missing_file_is_a_first_launch_not_an_error() {
        let settings = Settings::load(Path::new("/nonexistent/veronica/settings.json")).unwrap();
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn unknown_keys_survive_a_round_trip() {
        let dir = std::env::temp_dir().join(format!("veronica-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, br#"{"fromNewerBuild": {"nested": 1}}"#).unwrap();

        let mut settings = Settings::load(&path).unwrap();
        settings.set("preventSleep", Value::Bool(true));
        settings.save(&path).unwrap();

        let reloaded = Settings::load(&path).unwrap();
        assert_eq!(
            reloaded.get("fromNewerBuild"),
            Some(&serde_json::json!({"nested": 1}))
        );
        assert!(reloaded.bool_or("preventSleep", false));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn extension_default_follows_the_featured_flag() {
        let settings = Settings::default();
        let usage = crate::extensions::entry("usage").unwrap();
        let color = crate::extensions::entry("colorPicker").unwrap();
        assert!(settings.extension_enabled(usage), "featured is on by default");
        assert!(!settings.extension_enabled(color), "unfeatured is off");
    }
}
