use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::Value;
use veronica_core::fsutil::write_atomic;

pub(crate) const QUOTA_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_KEY_BYTES: usize = 256;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("storing {needed} bytes would exceed the {QUOTA_BYTES} byte quota")]
pub(crate) struct QuotaExceeded {
    pub needed: usize,
}

#[derive(Debug)]
pub(crate) struct Storage {
    file: PathBuf,
    entries: BTreeMap<String, Value>,
}

impl Storage {
    pub(crate) fn open(file: PathBuf) -> Result<Self> {
        let entries = match std::fs::read(&file) {
            Ok(raw) => serde_json::from_slice(&raw)
                .with_context(|| format!("{} is not a valid store", file.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", file.display()))
            }
        };
        Ok(Self { file, entries })
    }

    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        self.entries.get(key)
    }

    pub(crate) fn keys(&self) -> Vec<&String> {
        self.entries.keys().collect()
    }

    fn encoded(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&self.entries)?)
    }

    pub(crate) fn set(&mut self, key: &str, value: Value) -> Result<()> {
        let before = self.entries.insert(key.to_string(), value);
        let body = self.encoded()?;
        if body.len() > QUOTA_BYTES {
            match before {
                Some(value) => self.entries.insert(key.to_string(), value),
                None => self.entries.remove(key),
            };
            return Err(QuotaExceeded { needed: body.len() }.into());
        }
        write_atomic(&self.file, &body)
    }

    pub(crate) fn remove(&mut self, key: &str) -> Result<bool> {
        if self.entries.remove(key).is_none() {
            return Ok(false);
        }
        write_atomic(&self.file, &self.encoded()?)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn values_persist_and_the_quota_refuses_without_losing_data() {
        let root = tempfile::tempdir().expect("temporary directory");
        let file = root.path().join("data/probe/interface-storage.json");
        let mut storage = Storage::open(file.clone()).expect("store");
        assert!(storage.keys().is_empty());
        storage
            .set("view", json!({ "tab": "albums" }))
            .expect("set");
        storage.set("volume", json!(7)).expect("set");

        let oversized = json!("x".repeat(QUOTA_BYTES));
        let refused = storage.set("view", oversized).expect_err("over quota");
        assert!(refused.downcast_ref::<QuotaExceeded>().is_some());
        let near = json!("x".repeat(QUOTA_BYTES - 64));
        storage.set("blob", near).expect("under quota");
        assert!(storage.set("more", json!("x".repeat(64))).is_err());

        let mut reopened = Storage::open(file).expect("store");
        assert_eq!(reopened.get("view"), Some(&json!({ "tab": "albums" })));
        assert_eq!(reopened.keys(), ["blob", "view", "volume"]);
        assert_eq!(reopened.get("more"), None);
        assert!(reopened.remove("blob").expect("remove"));
        assert!(!reopened.remove("blob").expect("remove"));
    }
}
