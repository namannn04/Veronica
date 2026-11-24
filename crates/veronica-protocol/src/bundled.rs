use serde::{Deserialize, Serialize};

use crate::manifest::Profile;
use crate::version::PackageVersion;

pub const BUNDLED_SCHEMA: u32 = 1;
pub const BUNDLED_INDEX: &str = "index.json";
pub const NEVER_BUNDLED: [&str; 1] = ["fixture"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BundledIndex {
    pub schema: u32,
    pub extensions: Vec<BundledPackage>,
}

impl Default for BundledIndex {
    fn default() -> Self {
        Self {
            schema: BUNDLED_SCHEMA,
            extensions: Vec::new(),
        }
    }
}

impl BundledIndex {
    pub fn get(&self, id: &str) -> Option<&BundledPackage> {
        self.extensions.iter().find(|package| package.id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BundledPackage {
    pub id: String,
    pub version: PackageVersion,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

impl BundledPackage {
    pub fn file_name(&self) -> String {
        bundled_file_name(&self.id)
    }
}

pub fn bundled_file_name(id: &str) -> String {
    format!("{id}.tar.zst")
}
