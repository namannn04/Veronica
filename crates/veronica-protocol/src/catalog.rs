use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::manifest::{
    Contracts, Engines, Manifest, Permissions, Profile, Requirements, SuggestSignal, Target,
};
use crate::version::PackageVersion;

pub const ENVELOPE_SCHEMA: u32 = 1;
pub const CATALOG_SCHEMA: u32 = 1;
pub const CATALOG_URL: &str = "https://veronica.namandadhich.in/extensions/v1/catalog.signed.json";
pub const PACKAGE_URL_PREFIX: &str = "https://github.com/namannn04/veronica/releases/download/ext/";
pub const FILE_MODE_REGULAR: u32 = 0o644;
pub const FILE_MODE_EXECUTABLE: u32 = 0o755;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedEnvelope {
    pub schema: u32,
    pub payload: String,
    pub signatures: Vec<EnvelopeSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvelopeSignature {
    pub key_id: String,
    pub algorithm: SignatureAlgorithm,
    pub signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignatureAlgorithm {
    Ed25519,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub schema: u32,
    pub revision: u64,
    pub generated_at: String,
    pub expires_at: String,
    pub source: CatalogSource,
    pub categories: Vec<Category>,
    pub profiles: Vec<CatalogProfile>,
    pub extensions: Vec<CatalogExtension>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogSource {
    pub repository: String,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Category {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogProfile {
    pub id: Profile,
    pub title: String,
    pub description: String,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogExtension {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub labs: bool,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub suggest_for: Vec<SuggestSignal>,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub requires: Requirements,
    #[serde(default)]
    pub contracts: Contracts,
    pub latest: PackageVersion,
    pub versions: Vec<CatalogVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogVersion {
    pub version: PackageVersion,
    pub engines: Engines,
    pub published_at: String,
    pub fingerprint: String,
    #[serde(default)]
    pub yanked: bool,
    pub targets: BTreeMap<Target, TargetArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetArtifact {
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub unpacked_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageMetadata {
    pub id: String,
    pub version: PackageVersion,
    pub target: Target,
    pub size: u64,
    pub sha256: String,
    pub unpacked_size: u64,
    pub fingerprint: String,
    pub commit: String,
    pub engines: Engines,
    pub manifest: Manifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileEntry {
    pub sha256: String,
    pub size: u64,
    pub mode: u32,
}

pub type FileIndex = BTreeMap<String, FileEntry>;

impl FileEntry {
    pub const fn has_allowed_mode(&self) -> bool {
        matches!(self.mode, FILE_MODE_REGULAR | FILE_MODE_EXECUTABLE)
    }
}

pub fn package_asset_name(id: &str, version: PackageVersion, target: Target) -> String {
    format!("veronica-ext-{id}-{version}-{}.tar.zst", target.as_str())
}

pub fn package_url(id: &str, version: PackageVersion, target: Target) -> String {
    format!(
        "{PACKAGE_URL_PREFIX}{id}/v{version}/{}",
        package_asset_name(id, version, target)
    )
}

impl SignedEnvelope {
    pub fn to_canonical_string(&self) -> serde_json::Result<String> {
        canonical::to_string_pretty(self)
    }
}

impl Catalog {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.extensions
            .sort_by(|left, right| left.id.cmp(&right.id));
        for extension in &mut self.extensions {
            extension
                .versions
                .sort_by_key(|entry| std::cmp::Reverse(entry.version));
        }
        self
    }

    pub fn to_payload_bytes(&self) -> serde_json::Result<Vec<u8>> {
        Ok(canonical::to_string(&self.clone().normalized())?.into_bytes())
    }

    pub fn extension(&self, id: &str) -> Option<&CatalogExtension> {
        self.extensions.iter().find(|extension| extension.id == id)
    }
}

impl CatalogExtension {
    pub fn latest_version(&self) -> Option<&CatalogVersion> {
        self.versions
            .iter()
            .find(|version| version.version == self.latest)
    }
}

impl CatalogVersion {
    pub fn artifact(&self, target: Target) -> Option<&TargetArtifact> {
        self.targets.get(&target)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{package_url, Catalog, FileEntry, SignedEnvelope, PACKAGE_URL_PREFIX};
    use crate::manifest::{Profile, Target};

    fn sample() -> serde_json::Value {
        json!({
            "schema": 1,
            "revision": 1_795_564_800,
            "generatedAt": "2026-11-24T12:00:00Z",
            "expiresAt": "2026-12-24T12:00:00Z",
            "source": { "repository": "namannn04/veronica", "commit": "0123abcd" },
            "categories": [
                { "id": "agents", "title": "Agents" },
                { "id": "labs", "title": "Labs", "hidden": true }
            ],
            "profiles": [
                { "id": "simple", "title": "Simple", "description": "Everyday tools",
                  "extensions": ["media", "calendar"] },
                { "id": "developer", "title": "Developer", "description": "Agents and machines",
                  "extensions": ["usage", "media"] }
            ],
            "extensions": [
                { "id": "usage", "title": "Usage", "summary": "Agent limits.", "category": "agents",
                  "latest": "1.2.3", "versions": [
                    { "version": "1.2.3", "engines": { "host": ">=0.2.0", "hostApi": 1 },
                      "publishedAt": "2026-11-20T00:00:00Z", "fingerprint": "sha256:aa",
                      "targets": { "linux-x86_64": { "url": "u", "size": 1, "sha256": "s",
                                                      "unpackedSize": 2 } } }
                  ] },
                { "id": "media", "title": "Music", "summary": "Players.", "category": "media",
                  "icon": "music", "profiles": ["simple", "developer"],
                  "suggestFor": [{ "path": "~/Music" }],
                  "permissions": {}, "requires": {}, "contracts": {},
                  "latest": "1.0.7", "versions": [
                    { "version": "1.0.6", "engines": { "host": ">=0.2.0", "hostApi": 1 },
                      "publishedAt": "2026-11-01T00:00:00Z", "fingerprint": "sha256:bb",
                      "yanked": true, "targets": {} },
                    { "version": "1.0.7", "engines": { "host": ">=0.2.0", "hostApi": 1 },
                      "publishedAt": "2026-11-22T00:00:00Z", "fingerprint": "sha256:cc",
                      "targets": { "linux-x86_64": {
                        "url": package_url("media", "1.0.7".parse().unwrap(), Target::LinuxX86_64),
                        "size": 1_830_211, "sha256": "dd", "unpackedSize": 5_120_000 } } }
                  ] }
            ]
        })
    }

    #[test]
    fn parses_the_documented_shape() {
        let catalog: Catalog = serde_json::from_value(sample()).unwrap();
        assert_eq!(catalog.profiles[0].id, Profile::Simple);
        assert!(catalog.categories[1].hidden);
        let media = catalog.extension("media").unwrap();
        let latest = media.latest_version().unwrap();
        assert_eq!(latest.version.to_string(), "1.0.7");
        let artifact = latest.artifact(Target::LinuxX86_64).unwrap();
        assert_eq!(
            artifact.url,
            "https://github.com/namannn04/veronica/releases/download/ext/media/v1.0.7/veronica-ext-media-1.0.7-linux-x86_64.tar.zst"
        );
        assert!(artifact.url.starts_with(PACKAGE_URL_PREFIX));
        assert!(media.versions[0].yanked);
    }

    #[test]
    fn payload_bytes_are_deterministic() {
        let catalog: Catalog = serde_json::from_value(sample()).unwrap();
        let mut shuffled = catalog.clone();
        shuffled.extensions.reverse();
        for extension in &mut shuffled.extensions {
            extension.versions.reverse();
        }
        let bytes = catalog.to_payload_bytes().unwrap();
        assert_eq!(bytes, shuffled.to_payload_bytes().unwrap());
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with(r#"{"categories":[{"hidden":false,"id":"agents""#));
        assert!(text.find(r#""id":"media""#) < text.find(r#""id":"usage""#));
        assert!(text.find(r#""version":"1.0.7""#) < text.find(r#""version":"1.0.6""#));
        let reread: Catalog = serde_json::from_str(&text).unwrap();
        assert_eq!(reread, catalog.normalized());
    }

    #[test]
    fn envelope_round_trips_and_rejects_unknown_algorithms() {
        let raw = json!({
            "schema": 1,
            "payload": "e30=",
            "signatures": [{ "keyId": "ext-2026-1", "algorithm": "ed25519", "signature": "c2ln" }]
        });
        let envelope: SignedEnvelope = serde_json::from_value(raw.clone()).unwrap();
        let text = envelope.to_canonical_string().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).unwrap(),
            raw
        );
        assert!(text.ends_with("}\n"));
        let mut bad = raw;
        bad["signatures"][0]["algorithm"] = json!("rsa");
        assert!(serde_json::from_value::<SignedEnvelope>(bad).is_err());
    }

    #[test]
    fn file_modes_are_limited() {
        let entry = |mode| FileEntry {
            sha256: String::new(),
            size: 0,
            mode,
        };
        assert!(entry(420).has_allowed_mode());
        assert!(entry(493).has_allowed_mode());
        assert!(!entry(0o777).has_allowed_mode());
    }
}
