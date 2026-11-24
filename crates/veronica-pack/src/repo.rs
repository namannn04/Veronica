use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use veronica_protocol::Manifest;

pub const EXTENSIONS_DIR: &str = "extensions";
pub const MANIFEST_FILE: &str = "manifest.json";

pub fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("could not run git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("git printed output that is not UTF-8")
}

pub fn find_root(start: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(
        git(start, &["rev-parse", "--show-toplevel"])?.trim(),
    ))
}

pub fn head_commit(root: &Path) -> Result<String> {
    Ok(git(root, &["rev-parse", "HEAD"])?.trim().to_string())
}

pub fn extension_dir(root: &Path, id: &str) -> PathBuf {
    root.join(EXTENSIONS_DIR).join(id)
}

pub fn extension_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    let base = root.join(EXTENSIONS_DIR);
    if !base.is_dir() {
        return Ok(Vec::new());
    }
    let mut dirs = Vec::new();
    for entry in fs::read_dir(&base).with_context(|| format!("reading {}", base.display()))? {
        let path = entry?.path();
        if path.join(MANIFEST_FILE).is_file() {
            dirs.push(path);
        }
    }
    dirs.sort();
    Ok(dirs)
}

pub fn read_manifest(dir: &Path) -> Result<Manifest> {
    let path = dir.join(MANIFEST_FILE);
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    Manifest::parse(&text).with_context(|| format!("{} is invalid", path.display()))
}
