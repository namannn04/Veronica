use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, SecondsFormat};
use veronica_protocol::catalog::{
    package_url, Catalog, CatalogExtension, CatalogProfile, CatalogSource, CatalogVersion,
    Category, PackageMetadata, TargetArtifact, CATALOG_SCHEMA,
};
use veronica_protocol::manifest::Profile;
use veronica_protocol::PackageVersion;

pub const REPOSITORY: &str = "namannn04/veronica";
pub const KEPT_VERSIONS: usize = 3;
const LABS_CATEGORY: &str = "labs";
const PROFILES: [(Profile, &str, &str); 2] = [
    (Profile::Simple, "Simple", "Everyday tools"),
    (
        Profile::Developer,
        "Developer",
        "Agents, machines and services",
    ),
];

pub struct CatalogOptions {
    pub revision: u64,
    pub expires_days: u32,
    pub commit: String,
}

fn collect_json(dir: &Path, found: &mut Vec<std::path::PathBuf>) -> Result<()> {
    for item in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = item?.path();
        if path.is_dir() {
            collect_json(&path, found)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            found.push(path);
        }
    }
    Ok(())
}

pub fn read_metadata(dir: &Path) -> Result<Vec<PackageMetadata>> {
    let mut paths = Vec::new();
    collect_json(dir, &mut paths)?;
    paths.sort();
    paths
        .iter()
        .map(|path| {
            let text = fs::read_to_string(path)?;
            serde_json::from_str(&text)
                .with_context(|| format!("{} is not package metadata", path.display()))
        })
        .collect()
}

fn timestamp(seconds: i64) -> Result<String> {
    Ok(DateTime::from_timestamp(seconds, 0)
        .context("timestamp is out of range")?
        .to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn title_case(id: &str) -> String {
    id.split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn version_entry(
    id: &str,
    version: PackageVersion,
    builds: &[PackageMetadata],
    published_at: &str,
) -> Result<CatalogVersion> {
    let first = &builds[0];
    let mut targets = BTreeMap::new();
    for build in builds {
        if build.fingerprint != first.fingerprint {
            bail!("{id} {version} was built from different inputs per target");
        }
        let artifact = TargetArtifact {
            url: package_url(id, version, build.target),
            size: build.size,
            sha256: build.sha256.clone(),
            unpacked_size: build.unpacked_size,
        };
        if targets.insert(build.target, artifact).is_some() {
            bail!("{id} {version} has two {} builds", build.target.as_str());
        }
    }
    Ok(CatalogVersion {
        version,
        engines: first.engines.clone(),
        published_at: published_at.to_string(),
        fingerprint: first.fingerprint.clone(),
        yanked: false,
        targets,
    })
}

pub fn build(metadata: Vec<PackageMetadata>, options: &CatalogOptions) -> Result<Catalog> {
    let revision = i64::try_from(options.revision).context("revision is out of range")?;
    let generated_at = timestamp(revision)?;
    let expires_at =
        timestamp(revision + Duration::days(options.expires_days.into()).num_seconds())?;

    let mut grouped: BTreeMap<String, BTreeMap<PackageVersion, Vec<PackageMetadata>>> =
        BTreeMap::new();
    for item in metadata {
        if item.manifest.id != item.id {
            bail!(
                "metadata for {} carries the manifest of {}",
                item.id,
                item.manifest.id
            );
        }
        grouped
            .entry(item.id.clone())
            .or_default()
            .entry(item.version)
            .or_default()
            .push(item);
    }

    let mut extensions = Vec::new();
    for (id, versions) in &grouped {
        let kept: Vec<_> = versions.iter().rev().take(KEPT_VERSIONS).collect();
        let (latest, builds) = kept[0];
        let manifest = &builds[0].manifest;
        extensions.push(CatalogExtension {
            id: id.clone(),
            title: manifest.title.clone(),
            summary: manifest.summary.clone(),
            category: manifest.category.clone(),
            icon: manifest.icon.clone(),
            labs: manifest.labs,
            profiles: manifest.profiles.clone(),
            suggest_for: manifest.suggest_for.clone(),
            permissions: manifest.permissions.clone(),
            requires: manifest.requires.clone(),
            contracts: manifest.contracts.clone(),
            latest: *latest,
            versions: kept
                .iter()
                .map(|(version, builds)| version_entry(id, **version, builds, &generated_at))
                .collect::<Result<_>>()?,
        });
    }

    let mut category_ids: BTreeSet<&str> = extensions
        .iter()
        .map(|extension| extension.category.as_str())
        .collect();
    if extensions.iter().any(|extension| extension.labs) {
        category_ids.insert(LABS_CATEGORY);
    }
    let categories = category_ids
        .into_iter()
        .map(|id| Category {
            id: id.to_string(),
            title: title_case(id),
            hidden: id == LABS_CATEGORY,
        })
        .collect();
    let profiles = PROFILES
        .iter()
        .map(|(profile, title, description)| CatalogProfile {
            id: *profile,
            title: (*title).to_string(),
            description: (*description).to_string(),
            extensions: extensions
                .iter()
                .filter(|extension| extension.profiles.contains(profile))
                .map(|extension| extension.id.clone())
                .collect(),
        })
        .collect();

    Ok(Catalog {
        schema: CATALOG_SCHEMA,
        revision: options.revision,
        generated_at,
        expires_at,
        source: CatalogSource {
            repository: REPOSITORY.to_string(),
            commit: options.commit.clone(),
        },
        categories,
        profiles,
        extensions,
    }
    .normalized())
}
