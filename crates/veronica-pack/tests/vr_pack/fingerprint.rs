use std::path::Path;

use veronica_pack::fingerprint::{fingerprint, input_paths, published_at, stamp};

use crate::common::{commit_all, repo, write};

fn print(root: &Path, id: &str) -> String {
    fingerprint(root, &input_paths(root, id).unwrap()).unwrap()
}

#[test]
fn inputs_follow_the_backend_dependency_closure() {
    let dir = repo();
    assert_eq!(
        input_paths(dir.path(), "alpha").unwrap(),
        [
            "Cargo.lock",
            "crates/shared",
            "extensions/alpha",
            "extensions/alpha/backend",
            "packages",
            "rust-toolchain.toml",
        ]
    );
    assert_eq!(
        input_paths(dir.path(), "beta").unwrap(),
        [
            "Cargo.lock",
            "extensions/beta",
            "packages",
            "rust-toolchain.toml"
        ]
    );
    write(dir.path(), "bun.lock", "{}\n");
    assert!(input_paths(dir.path(), "beta")
        .unwrap()
        .contains(&"bun.lock".to_string()));
}

#[test]
fn fingerprints_are_stable_and_order_independent() {
    let dir = repo();
    let root = dir.path();
    let first = print(root, "alpha");
    assert!(first.starts_with("sha256:"));
    assert_eq!(first, print(root, "alpha"));
    let mut reversed = input_paths(root, "alpha").unwrap();
    reversed.reverse();
    assert_eq!(first, fingerprint(root, &reversed).unwrap());
    assert_ne!(first, print(root, "beta"));
}

#[test]
fn fingerprints_change_only_with_their_inputs() {
    let dir = repo();
    let root = dir.path();
    let alpha = print(root, "alpha");
    let beta = print(root, "beta");

    write(
        root,
        "crates/other/src/lib.rs",
        "pub const VALUE: u8 = 3;\n",
    );
    write(root, "extensions/alpha/ui/node_modules/x.js", "ignored\n");
    write(root, "README.md", "unrelated\n");
    assert_eq!(print(root, "alpha"), alpha);
    assert_eq!(print(root, "beta"), beta);

    write(
        root,
        "crates/shared/src/lib.rs",
        "pub const VALUE: u8 = 9;\n",
    );
    let changed = print(root, "alpha");
    assert_ne!(changed, alpha);
    assert_eq!(print(root, "beta"), beta);

    write(
        root,
        "extensions/alpha/notes.txt",
        "untracked but not ignored\n",
    );
    assert_ne!(print(root, "alpha"), changed);

    write(root, "packages/kit/index.js", "export const kit = 2;\n");
    assert_ne!(print(root, "beta"), beta);
}

#[test]
fn publish_time_comes_from_the_commit_or_source_date_epoch() {
    let dir = repo();
    let root = dir.path();
    assert_eq!(published_at(root, None).unwrap(), "2026-11-20T04:30:00Z");
    assert_eq!(
        published_at(root, Some("1795564800")).unwrap(),
        "2026-11-25T00:00:00Z"
    );
    assert!(published_at(root, Some("soon")).is_err());
}

#[test]
fn versions_count_the_commits_that_touch_the_inputs() {
    let dir = repo();
    let root = dir.path();
    let alpha = stamp(root, "alpha").unwrap();
    assert_eq!(alpha.version.to_string(), "1.2.1");
    assert_eq!(stamp(root, "beta").unwrap().version.to_string(), "0.3.1");

    write(
        root,
        "crates/shared/src/lib.rs",
        "pub const VALUE: u8 = 9;\n",
    );
    commit_all(root, "shared");
    write(root, "README.md", "unrelated\n");
    commit_all(root, "readme");

    let later = stamp(root, "alpha").unwrap();
    assert_eq!(later.version.to_string(), "1.2.2");
    assert_ne!(later.commit, alpha.commit);
    assert_eq!(stamp(root, "beta").unwrap().version.to_string(), "0.3.1");
}
