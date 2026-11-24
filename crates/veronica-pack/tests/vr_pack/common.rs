use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

pub(crate) fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

pub(crate) fn write(root: &Path, path: &str, contents: &str) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, contents).unwrap();
}

pub(crate) fn run(root: &Path, program: &str, args: &[&str]) {
    let status = Command::new(program)
        .current_dir(root)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "{program} {args:?} failed");
}

pub(crate) fn git(root: &Path, args: &[&str]) {
    let mut full = vec![
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.com",
        "-c",
        "commit.gpgsign=false",
    ];
    full.extend_from_slice(args);
    run(root, "git", &full);
}

pub(crate) fn commit_all(root: &Path, message: &str) {
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", message]);
}

pub(crate) fn repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    copy_tree(&fixtures().join("repo"), root);
    write(
        root,
        "Cargo.toml",
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/shared\", \"crates/other\", \"extensions/alpha/backend\"]\n",
    );
    write(
        root,
        "crates/shared/Cargo.toml",
        "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(
        root,
        "crates/shared/src/lib.rs",
        "pub const VALUE: u8 = 1;\n",
    );
    write(
        root,
        "crates/other/Cargo.toml",
        "[package]\nname = \"other\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(
        root,
        "crates/other/src/lib.rs",
        "pub const VALUE: u8 = 2;\n",
    );
    write(
        root,
        "extensions/alpha/backend/Cargo.toml",
        "[package]\nname = \"veronica-ext-alpha\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nshared = { path = \"../../../crates/shared\" }\n\n[dev-dependencies]\nother = { path = \"../../../crates/other\" }\n",
    );
    write(
        root,
        "extensions/alpha/backend/src/main.rs",
        "fn main() {\n    println!(\"{}\", shared::VALUE);\n}\n",
    );
    write(
        root,
        "rust-toolchain.toml",
        "[toolchain]\nchannel = \"stable\"\n",
    );
    write(root, "LICENSE", "license text\n");
    write(root, "NOTICE", "notice text\n");
    write(root, ".gitignore", "node_modules/\n/target/\n");
    run(root, "git", &["init", "-q"]);
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    run(root, &cargo, &["generate-lockfile", "--offline"]);
    commit_all(root, "fixture");
    dir
}
