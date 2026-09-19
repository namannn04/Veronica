//! One theme, four files.
//!
//! An appearance has to be declared in the desktop stylesheet, in the
//! catalogue the picker reads, in the shell extension's list and in
//! [`veronica_core::appearance`]. Miss one and nothing fails to build:
//!
//! - missing from `styles.css`'s family list, and the theme inherits the other
//!   family's chart palette - light series on a dark page;
//! - missing from `preferences.ts`, and it cannot be chosen at all;
//! - missing from `extension/themes.js`, and the notch silently falls back to
//!   `system` while the app is themed, and the previous theme's style class is
//!   left stuck on the popup;
//! - missing from `appearance.rs`, and Quinjet opens light inside a dark
//!   desktop.
//!
//! So the four lists are compared here instead.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use veronica_core::appearance::{DARK_THEMES, LIGHT_THEMES};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repository root")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn set(items: impl IntoIterator<Item = impl Into<String>>) -> BTreeSet<String> {
    items.into_iter().map(Into::into).collect()
}

/// Every id inside a `[...]` array assigned to `name` in a JavaScript file.
fn js_array(source: &str, name: &str) -> BTreeSet<String> {
    let after = source
        .split_once(&format!("{name} = ["))
        .unwrap_or_else(|| panic!("{name} is not declared as an array"))
        .1;
    let body = after
        .split_once(']')
        .unwrap_or_else(|| panic!("{name} is not closed"))
        .0;
    set(body
        .split(',')
        .map(|item| item.trim().trim_matches('\'').trim_matches('"').to_string())
        .filter(|item| !item.is_empty()))
}

/// CSS with `/* ... */` removed, so a comment sitting immediately above a rule
/// is not read as part of its first selector.
fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some((before, after)) = rest.split_once("/*") {
        out.push_str(before);
        out.push(' ');
        match after.split_once("*/") {
            Some((_, tail)) => rest = tail,
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The ids in the one `:root[data-theme=...]` selector list that names `anchor`
/// alongside at least one other theme - that is, a family list rather than a
/// theme's own block.
fn css_family(source: &str, anchor: &str) -> BTreeSet<String> {
    let needle = format!(":root[data-theme=\"{anchor}\"]");
    let source = strip_comments(source);
    for rule in source.split('{') {
        let head = rule.rsplit('}').next().unwrap_or(rule);
        let ids: BTreeSet<String> = head
            .split(",")
            .filter_map(|selector| {
                let selector = selector.trim();
                let rest = selector.strip_prefix(":root[data-theme=\"")?;
                Some(rest.split('"').next()?.to_string())
            })
            .collect();
        if ids.len() > 1 && head.contains(&needle) {
            return ids;
        }
    }
    panic!("no family selector list in styles.css contains {needle}");
}

/// The ids the appearance picker offers, by family, excluding `system` - which
/// is the absence of a theme rather than one of them.
fn catalogue(family: &str) -> BTreeSet<String> {
    let source = read("apps/desktop/src/lib/preferences.ts");
    let mut found = BTreeSet::new();
    for entry in source.split("id: \"").skip(1) {
        let Some(id) = entry.split('"').next() else {
            continue;
        };
        // The entry ends at the next one; `family` must come from this entry.
        let entry = entry.split("id: \"").next().unwrap_or(entry);
        if entry.contains(&format!("family: \"{family}\"")) {
            found.insert(id.to_string());
        }
    }
    assert!(!found.is_empty(), "no {family} themes in the catalogue");
    found
}

#[test]
fn the_stylesheet_and_the_catalogue_offer_the_same_themes() {
    let styles = read("apps/desktop/src/styles.css");
    assert_eq!(
        css_family(&styles, "dark"),
        catalogue("dark"),
        "the dark family list in styles.css and the catalogue disagree"
    );
    assert_eq!(
        css_family(&styles, "light"),
        catalogue("light"),
        "the light family list in styles.css and the catalogue disagree"
    );
}

#[test]
fn the_shell_extension_knows_every_theme_the_app_offers() {
    let source = read("extension/themes.js");
    assert_eq!(js_array(&source, "DARK_THEMES"), catalogue("dark"));
    assert_eq!(js_array(&source, "LIGHT_THEMES"), catalogue("light"));
}

#[test]
fn the_core_resolves_every_theme_the_app_offers() {
    assert_eq!(set(DARK_THEMES), catalogue("dark"));
    assert_eq!(set(LIGHT_THEMES), catalogue("light"));
}

#[test]
fn every_theme_has_notch_rules_of_its_own() {
    // Without a block the popup keeps Graphite's island under a Nord app.
    // Graphite itself is the exception, and the only one: it is what the notch
    // already looks like, so it is styled by the rules above these.
    let stylesheet = read("extension/stylesheet.css");
    for theme in DARK_THEMES.iter().chain(LIGHT_THEMES.iter()) {
        if *theme == "dark" {
            continue;
        }
        assert!(
            stylesheet.contains(&format!(".veronica-theme-{theme} ")),
            "{theme} has no rules in extension/stylesheet.css"
        );
    }
}

#[test]
fn system_is_not_a_theme_class() {
    // `system` means "ask the desktop", and resolves to one of the others
    // before any class is set. A `veronica-theme-system` rule would mean
    // somebody had modelled it as a palette of its own.
    assert!(!set(DARK_THEMES).contains("system"));
    assert!(!set(LIGHT_THEMES).contains("system"));
    assert!(!read("extension/themes.js").contains("'system'"));
    assert!(!read("extension/stylesheet.css").contains("veronica-theme-system"));
}
