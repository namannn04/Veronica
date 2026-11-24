use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;
use veronica_protocol::manifest::RESERVED_ROOTS;
use veronica_protocol::Manifest;

use crate::repo::read_manifest;

pub const HOST_CONTRACTS: [&str; 5] = [
    "host.presenter@1",
    "host.privacy@1",
    "host.appearance@1",
    "host.session@1",
    "host.capabilities@1",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub extension: String,
    pub message: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub extensions: Vec<String>,
    pub problems: Vec<Problem>,
}

impl ValidationReport {
    pub fn is_ok(&self) -> bool {
        self.problems.is_empty()
    }
}

fn problem(extension: &str, message: String) -> Problem {
    Problem {
        extension: extension.to_string(),
        message,
    }
}

pub fn validate_dirs(dirs: &[PathBuf]) -> ValidationReport {
    let mut report = ValidationReport::default();
    let mut manifests = Vec::new();
    for dir in dirs {
        let name = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        match read_manifest(dir) {
            Ok(manifest) => {
                if manifest.id != name {
                    report.problems.push(problem(
                        &name,
                        format!("directory {name:?} holds manifest id {:?}", manifest.id),
                    ));
                }
                report.extensions.push(manifest.id.clone());
                manifests.push(manifest);
            }
            Err(error) => report.problems.push(problem(&name, format!("{error:#}"))),
        }
    }
    report.problems.extend(cross_check(&manifests));
    report.extensions.sort();
    report
}

struct Claims<'a> {
    manifests: &'a [Manifest],
    owners: BTreeMap<(&'static str, String), usize>,
    problems: Vec<Problem>,
}

impl Claims<'_> {
    fn claim(&mut self, what: &'static str, name: String, index: usize) {
        let id = &self.manifests[index].id;
        match self.owners.get(&(what, name.clone())) {
            Some(&owner) if owner != index => self.problems.push(problem(
                id,
                format!(
                    "{what} {name:?} is already claimed by {:?}",
                    self.manifests[owner].id
                ),
            )),
            Some(_) => {}
            None => {
                self.owners.insert((what, name), index);
            }
        }
    }
}

pub fn cross_check(manifests: &[Manifest]) -> Vec<Problem> {
    let mut claims = Claims {
        manifests,
        owners: BTreeMap::new(),
        problems: Vec::new(),
    };
    for (index, manifest) in manifests.iter().enumerate() {
        let id = manifest.id.as_str();
        claims.claim("extension id", id.to_string(), index);
        for name in manifest
            .roots()
            .flat_map(veronica_protocol::manifest::Command::names)
        {
            if RESERVED_ROOTS.contains(&name) {
                claims.problems.push(problem(
                    id,
                    format!("command root {name:?} is reserved by the host"),
                ));
            }
            claims.claim("command root", name.to_string(), index);
        }
        for (route, command) in manifest.leaf_routes() {
            if command.mcp_exposed() {
                claims.claim("MCP tool", Manifest::mcp_tool_name(&route, command), index);
            }
        }
        for provided in &manifest.contracts.provides {
            let contract = provided.id.to_string();
            if HOST_CONTRACTS.contains(&contract.as_str()) {
                claims.problems.push(problem(
                    id,
                    format!("contract {contract} is provided by the host"),
                ));
            }
            claims.claim("contract", contract, index);
        }
    }
    let mut problems = claims.problems;
    for manifest in manifests {
        for used in &manifest.contracts.uses {
            let contract = used.to_string();
            let provided = claims.owners.contains_key(&("contract", contract.clone()));
            if !HOST_CONTRACTS.contains(&contract.as_str()) && !provided {
                problems.push(problem(
                    &manifest.id,
                    format!("uses contract {contract}, which nothing provides"),
                ));
            }
        }
    }
    problems
}
