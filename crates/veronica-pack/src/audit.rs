use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use veronica_protocol::manifest::{Backend, Permissions};

use crate::repo::MANIFEST_FILE;

pub const NETWORK_MARKERS: [&str; 6] = [
    "getaddrinfo",
    "reqwest",
    "hyper",
    "rustls",
    "ureq",
    "openssl",
];
pub const DBUS_MARKERS: [&str; 3] = ["org.freedesktop.DBus", "zbus", "DBUS_SESSION_BUS_ADDRESS"];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackedManifest {
    id: String,
    #[serde(default)]
    backend: Option<Backend>,
    #[serde(default)]
    permissions: Permissions,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub declared: bool,
    pub detected: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReport {
    pub id: String,
    pub network: Usage,
    pub dbus: Usage,
    pub problems: Vec<String>,
}

impl AuditReport {
    pub fn is_ok(&self) -> bool {
        self.problems.is_empty()
    }
}

fn read_package(path: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut archive = tar::Archive::new(zstd::stream::Decoder::new(file)?);
    let mut files = BTreeMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = entry.path()?.to_string_lossy().into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        files.insert(name, bytes);
    }
    Ok(files)
}

fn detect(binary: &[u8], markers: &[&str]) -> Vec<String> {
    markers
        .iter()
        .filter(|marker| {
            binary
                .windows(marker.len())
                .any(|window| window == marker.as_bytes())
        })
        .map(ToString::to_string)
        .collect()
}

pub fn audit(path: &Path) -> Result<AuditReport> {
    let files = read_package(path)?;
    let manifest: PackedManifest = serde_json::from_slice(
        files
            .get(MANIFEST_FILE)
            .context("package has no manifest.json")?,
    )
    .context("package manifest is invalid")?;
    let binary = match &manifest.backend {
        Some(backend) => files
            .get(&backend.entry)
            .with_context(|| format!("package lacks its backend {}", backend.entry))?
            .as_slice(),
        None => &[],
    };
    let permissions = &manifest.permissions;
    let network = Usage {
        declared: !permissions.network.is_empty(),
        detected: detect(binary, &NETWORK_MARKERS),
    };
    let dbus = Usage {
        declared: !permissions.dbus.session.is_empty() || !permissions.dbus.system.is_empty(),
        detected: detect(binary, &DBUS_MARKERS),
    };
    let mut problems = Vec::new();
    for (name, usage) in [("network", &network), ("D-Bus", &dbus)] {
        if !usage.declared && !usage.detected.is_empty() {
            problems.push(format!(
                "backend shows {name} use ({}) that the manifest does not declare",
                usage.detected.join(", ")
            ));
        }
    }
    Ok(AuditReport {
        id: manifest.id,
        network,
        dbus,
        problems,
    })
}
