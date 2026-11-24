use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veronica_protocol::PackageVersion;

use crate::repo::{extension_dir, git, read_manifest, EXTENSIONS_DIR};
use crate::{hex, sha256_hex};

const SHARED_INPUTS: [&str; 3] = ["Cargo.lock", "rust-toolchain.toml", "packages"];
const OPTIONAL_INPUTS: [&str; 1] = ["bun.lock"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stamp {
    pub id: String,
    pub version: PackageVersion,
    pub fingerprint: String,
    pub commit: String,
    pub inputs: Vec<String>,
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<MetadataPackage>,
    resolve: Option<Resolve>,
    workspace_root: String,
}

#[derive(Deserialize)]
struct MetadataPackage {
    id: String,
    name: String,
    manifest_path: String,
    source: Option<String>,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<NodeDep>,
}

#[derive(Deserialize)]
struct NodeDep {
    pkg: String,
    dep_kinds: Vec<DepKind>,
}

#[derive(Deserialize)]
struct DepKind {
    kind: Option<String>,
}

pub fn backend_package(id: &str) -> String {
    format!("veronica-ext-{id}")
}

fn cargo_metadata(root: &Path) -> Result<Metadata> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command
        .current_dir(root)
        .args(["metadata", "--format-version", "1"]);
    if root.join("Cargo.lock").is_file() {
        command.arg("--locked");
    }
    let output = command.output().context("could not run cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout).context("cargo metadata printed unexpected JSON")
}

fn local_closure(root: &Path, package: &str) -> Result<Vec<String>> {
    if !root.join("Cargo.toml").is_file() {
        return Ok(Vec::new());
    }
    let metadata = cargo_metadata(root)?;
    let workspace = Path::new(&metadata.workspace_root);
    let local: BTreeMap<&str, &MetadataPackage> = metadata
        .packages
        .iter()
        .filter(|candidate| candidate.source.is_none())
        .map(|candidate| (candidate.id.as_str(), candidate))
        .collect();
    let Some(start) = local.values().find(|candidate| candidate.name == package) else {
        return Ok(Vec::new());
    };
    let nodes: BTreeMap<&str, &Node> = metadata
        .resolve
        .iter()
        .flat_map(|resolve| &resolve.nodes)
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut seen = BTreeSet::from([start.id.as_str()]);
    let mut queue = vec![start.id.as_str()];
    while let Some(current) = queue.pop() {
        let deps = nodes.get(current).map(|node| node.deps.as_slice());
        for dep in deps.unwrap_or_default() {
            let builds = dep
                .dep_kinds
                .iter()
                .any(|kind| kind.kind.as_deref() != Some("dev"));
            if builds && local.contains_key(dep.pkg.as_str()) && seen.insert(dep.pkg.as_str()) {
                queue.push(dep.pkg.as_str());
            }
        }
    }
    let mut paths = Vec::new();
    for package_id in seen {
        let manifest = Path::new(&local[package_id].manifest_path);
        let dir = manifest.parent().unwrap_or(manifest);
        let relative = dir
            .strip_prefix(workspace)
            .with_context(|| format!("{} lies outside the workspace", dir.display()))?;
        paths.push(relative.to_string_lossy().into_owned());
    }
    Ok(paths)
}

pub fn input_paths(root: &Path, id: &str) -> Result<Vec<String>> {
    let mut paths = BTreeSet::from([format!("{EXTENSIONS_DIR}/{id}")]);
    paths.extend(local_closure(root, &backend_package(id))?);
    paths.extend(SHARED_INPUTS.iter().map(ToString::to_string));
    paths.extend(
        OPTIONAL_INPUTS
            .iter()
            .filter(|path| root.join(path).exists())
            .map(ToString::to_string),
    );
    paths.retain(|path| !path.is_empty());
    Ok(paths.into_iter().collect())
}

pub fn fingerprint(root: &Path, paths: &[String]) -> Result<String> {
    let mut args = vec![
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
        "--",
    ];
    args.extend(paths.iter().map(String::as_str));
    let listing = git(root, &args)?;
    let files: BTreeSet<&str> = listing
        .split('\0')
        .filter(|file| !file.is_empty())
        .collect();
    let mut hasher = Sha256::new();
    for file in files {
        let full = root.join(file);
        if !full.is_file() {
            continue;
        }
        let executable = fs::metadata(&full)?.permissions().mode() & 0o111 != 0;
        let content = fs::read(&full).with_context(|| format!("reading {}", full.display()))?;
        hasher.update(file.as_bytes());
        hasher.update([0, u8::from(executable)]);
        hasher.update(sha256_hex(&content).as_bytes());
        hasher.update(b"\n");
    }
    Ok(format!("sha256:{}", hex(&hasher.finalize())))
}

pub fn change_count(root: &Path, paths: &[String]) -> Result<u32> {
    let mut args = vec!["rev-list", "--count", "HEAD", "--"];
    args.extend(paths.iter().map(String::as_str));
    let count = git(root, &args)?;
    count
        .trim()
        .parse()
        .with_context(|| format!("git printed an unexpected count {count:?}"))
}

pub fn stamp(root: &Path, id: &str) -> Result<Stamp> {
    let manifest = read_manifest(&extension_dir(root, id))?;
    let inputs = input_paths(root, id)?;
    Ok(Stamp {
        id: id.to_string(),
        version: manifest.version.with_patch(change_count(root, &inputs)?),
        fingerprint: fingerprint(root, &inputs)?,
        commit: crate::repo::head_commit(root)?,
        inputs,
    })
}
