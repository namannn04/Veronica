use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use serde::Serialize;

use crate::signing;

pub const TRUST_SOURCE: &str = "crates/veronica-host/src/trust.rs";
pub const DEVELOPMENT_SLOT: &str = "DEVELOPMENT_KEY";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledKey {
    pub name: String,
    pub id: String,
    pub public: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatus {
    pub key_id: String,
    pub public_key: String,
    pub trusted: bool,
    pub development: bool,
    pub compiled: Vec<CompiledKey>,
}

pub fn source_path(root: &Path) -> PathBuf {
    root.join(TRUST_SOURCE)
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let rest = &text[text.find(start)? + start.len()..];
    Some(&rest[..rest.find(end)?])
}

fn quoted_field(block: &str, field: &str) -> Option<String> {
    between(block, &format!("{field}: \""), "\"").map(str::to_string)
}

pub fn compiled_keys(source: &str) -> Result<Vec<CompiledKey>> {
    let slots = between(source, "pub const KEYS: [Option<TrustedKey>; 2] = [", "];")
        .context("trust.rs has no KEYS array of two slots")?;
    let mut keys = Vec::new();
    for slot in slots
        .split(',')
        .map(str::trim)
        .filter(|slot| !slot.is_empty())
    {
        if slot == "None" {
            continue;
        }
        let Some(name) = slot
            .strip_prefix("Some(")
            .and_then(|rest| rest.strip_suffix(')'))
        else {
            bail!("trust.rs KEYS slot {slot:?} is neither None nor Some(NAMED_KEY)");
        };
        let block = between(
            source,
            &format!("pub const {name}: TrustedKey = TrustedKey {{"),
            "};",
        )
        .with_context(|| format!("trust.rs does not define {name}"))?;
        keys.push(CompiledKey {
            name: name.to_string(),
            id: quoted_field(block, "id").with_context(|| format!("{name} has no id"))?,
            public: quoted_field(block, "public")
                .with_context(|| format!("{name} has no public key"))?,
        });
    }
    Ok(keys)
}

fn development_id(source: &str) -> Option<String> {
    let block = between(
        source,
        &format!("pub const {DEVELOPMENT_SLOT}: TrustedKey = TrustedKey {{"),
        "};",
    )?;
    quoted_field(block, "id")
}

pub fn status(key: &SigningKey, source: &str) -> Result<KeyStatus> {
    let compiled = compiled_keys(source)?;
    let public = key.verifying_key();
    let key_id = signing::key_id(&public);
    let public_key = signing::encode_public(&public);
    let development = development_id(source).as_deref() == Some(key_id.as_str());
    let listed = compiled
        .iter()
        .any(|candidate| candidate.id == key_id && candidate.public == public_key);
    Ok(KeyStatus {
        trusted: listed && !development,
        development,
        key_id,
        public_key,
        compiled,
    })
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::{compiled_keys, status};
    use crate::signing;

    fn source(slots: &str, key: &SigningKey) -> String {
        let development = signing::key_id(&SigningKey::from_bytes(&[1; 32]).verifying_key());
        let public = key.verifying_key();
        format!(
            "pub const DEVELOPMENT_KEY: TrustedKey = TrustedKey {{\n    id: \"{development}\",\n    public: \"AAAA\",\n}};\n\npub const RELEASE_KEY: TrustedKey = TrustedKey {{\n    id: \"{}\",\n    public: \"{}\",\n}};\n\npub const KEYS: [Option<TrustedKey>; 2] = [{slots}];\n",
            signing::key_id(&public),
            signing::encode_public(&public),
        )
    }

    #[test]
    fn a_release_key_in_a_slot_is_trusted() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let text = source("Some(RELEASE_KEY), None", &key);
        assert_eq!(compiled_keys(&text).unwrap().len(), 1);
        let report = status(&key, &text).unwrap();
        assert!(report.trusted);
        assert!(!report.development);
    }

    #[test]
    fn a_key_outside_the_slots_or_in_the_development_slot_is_not_trusted() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let text = source("Some(DEVELOPMENT_KEY), None", &key);
        assert_eq!(compiled_keys(&text).unwrap()[0].name, "DEVELOPMENT_KEY");
        assert!(!status(&key, &text).unwrap().trusted);
        let other = SigningKey::from_bytes(&[9; 32]);
        let text = source("Some(RELEASE_KEY), None", &key);
        assert!(!status(&other, &text).unwrap().trusted);
        let development = SigningKey::from_bytes(&[1; 32]);
        let report = status(&development, &source("Some(DEVELOPMENT_KEY), None", &key)).unwrap();
        assert!(report.development);
        assert!(!report.trusted);
    }

    #[test]
    fn unexpected_slot_shapes_are_refused() {
        let key = SigningKey::from_bytes(&[7; 32]);
        assert!(compiled_keys(&source("Some(MISSING), None", &key)).is_err());
        assert!(compiled_keys(&source("Some(TrustedKey { id: \"x\" }), None", &key)).is_err());
        assert!(compiled_keys("pub const KEYS: [Option<TrustedKey>; 3] = [None];").is_err());
    }
}
