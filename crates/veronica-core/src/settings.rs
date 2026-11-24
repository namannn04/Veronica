use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(flatten)]
    values: BTreeMap<String, Value>,
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(err).with_context(|| format!("cannot read {}", path.display())),
        };
        match serde_json::from_slice(&bytes) {
            Ok(settings) => Ok(settings),
            Err(error) => {
                let aside = path.with_extension(format!(
                    "json.corrupt-{}",
                    chrono::Utc::now().format("%Y%m%dT%H%M%S")
                ));
                std::fs::rename(path, &aside).with_context(|| {
                    format!(
                        "{} is not valid JSON ({error}) and cannot be set aside",
                        path.display()
                    )
                })?;
                eprintln!(
                    "veronica: {} was not valid JSON ({error}); kept it as {} and started from defaults",
                    path.display(),
                    aside.display()
                );
                Ok(Self::default())
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        crate::fsutil::write_atomic(path, &serde_json::to_vec_pretty(self)?)
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

    pub fn extension_enabled(&self, entry: &crate::extensions::ExtensionEntry) -> bool {
        self.bool_or(entry.defaults_key, entry.featured)
    }

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
    fn a_corrupt_file_is_set_aside_and_defaults_are_used() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("settings.json");
        std::fs::write(&path, br#"{"appearance": "dark","#).unwrap();

        let settings = Settings::load(&path).expect("a corrupt file must not be fatal");
        assert_eq!(settings, Settings::default());
        assert!(!path.exists(), "the corrupt file is moved, not overwritten");
        let kept: Vec<_> = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(kept.len(), 1);
        assert!(kept[0].starts_with("settings.json.corrupt-"), "{kept:?}");
        assert_eq!(
            std::fs::read(root.path().join(&kept[0])).unwrap(),
            br#"{"appearance": "dark","#
        );

        let mut settings = settings;
        settings.set("appearance", Value::from("light"));
        settings.save(&path).unwrap();
        assert_eq!(
            Settings::load(&path).unwrap().string("appearance"),
            Some("light")
        );
    }

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
        assert_eq!(
            Settings::coerce(r#""quoted""#),
            Value::String("quoted".into())
        );
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
        assert!(
            settings.extension_enabled(usage),
            "featured is on by default"
        );
        assert!(!settings.extension_enabled(color), "unfeatured is off");
    }
}
