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
    panic!("no family selector list in themes.css contains {needle}");
}

fn catalogue(family: &str) -> BTreeSet<String> {
    let source = read("apps/desktop/src/lib/preferences.ts");
    let mut found = BTreeSet::new();
    for entry in source.split("id: \"").skip(1) {
        let Some(id) = entry.split('"').next() else {
            continue;
        };
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
    let styles = read("apps/desktop/src/ui/themes.css");
    assert_eq!(
        css_family(&styles, "dark"),
        catalogue("dark"),
        "the dark family list in themes.css and the catalogue disagree"
    );
    assert_eq!(
        css_family(&styles, "light"),
        catalogue("light"),
        "the light family list in themes.css and the catalogue disagree"
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
    assert!(!set(DARK_THEMES).contains("system"));
    assert!(!set(LIGHT_THEMES).contains("system"));
    assert!(!read("extension/themes.js").contains("'system'"));
    assert!(!read("extension/stylesheet.css").contains("veronica-theme-system"));
}
