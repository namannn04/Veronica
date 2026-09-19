//! Reading a page's metadata out of its HTML.
//!
//! A direct port of Edith's `HTMLMetadataParser`: the same fields, the same
//! attribute lookups, the same word count. It scans tags with regular
//! expressions rather than building a DOM, exactly as Edith does — an audit
//! reads a handful of head elements, and a real parser would be a large
//! dependency for the sake of markup it never descends into.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use url::Url;

/// Everything an audit reads off one page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub canonical_url: Option<String>,
    pub robots: Option<String>,
    pub language: Option<String>,
    /// The first `h1`, which is the page's own heading.
    pub heading: Option<String>,
    pub open_graph_title: Option<String>,
    pub open_graph_description: Option<String>,
    pub open_graph_image_url: Option<String>,
    pub open_graph_type: Option<String>,
    pub twitter_card: Option<String>,
    pub twitter_title: Option<String>,
    pub twitter_description: Option<String>,
    pub twitter_image_url: Option<String>,
    pub word_count: usize,
}

fn tag_pattern(name: &str) -> Regex {
    Regex::new(&format!(r"(?is)<{name}\b[^>]*>")).expect("the tag names below are literals")
}

fn element_pattern(name: &str) -> Regex {
    Regex::new(&format!(r"(?is)<{name}\b[^>]*>(.*?)</{name}\s*>"))
        .expect("the tag names below are literals")
}

static META: LazyLock<Regex> = LazyLock::new(|| tag_pattern("meta"));
static LINK: LazyLock<Regex> = LazyLock::new(|| tag_pattern("link"));
static HTML: LazyLock<Regex> = LazyLock::new(|| tag_pattern("html"));
static TITLE: LazyLock<Regex> = LazyLock::new(|| element_pattern("title"));
static H1: LazyLock<Regex> = LazyLock::new(|| element_pattern("h1"));
static BODY: LazyLock<Regex> = LazyLock::new(|| element_pattern("body"));
/// `key="value"` and `key='value'` as two alternatives rather than one with a
/// backreference, which Rust's regex engine does not support — and does not
/// need to, since there are exactly two quote characters.
static ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)([A-Za-z_:][A-Za-z0-9_:.-]*)\s*=\s*(?:"([^"]*)"|'([^']*)')"#)
        .expect("a literal pattern")
});
static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<[^>]+>").expect("a literal"));
/// Script and style hold code, not prose; counting either as words would make
/// an empty page look full.
static NON_PROSE: LazyLock<Regex> = LazyLock::new(|| {
    // Spelled out per tag for the same reason: no backreference to close with.
    // Built with `concat!` because a raw string has no line continuation — a
    // trailing backslash there is a literal backslash, and the pattern would
    // silently stop matching.
    Regex::new(concat!(
        r"(?is)<script\b[^>]*>.*?</script\s*>",
        r"|<style\b[^>]*>.*?</style\s*>",
        r"|<noscript\b[^>]*>.*?</noscript\s*>",
        r"|<template\b[^>]*>.*?</template\s*>",
    ))
    .expect("a literal")
});

/// The attributes of one tag, lower-cased and entity-decoded.
fn attributes(tag: &str) -> HashMap<String, String> {
    ATTRIBUTE
        .captures_iter(tag)
        .filter_map(|caps| {
            // Whichever quote style matched; a name with no value is not an
            // attribute this reads.
            let value = caps.get(2).or_else(|| caps.get(3))?;
            Some((caps[1].to_lowercase(), decode(value.as_str())))
        })
        .collect()
}

/// The `content` of the first meta tag whose `key` attribute equals `value`.
fn meta_content(metas: &[HashMap<String, String>], key: &str, value: &str) -> Option<String> {
    metas
        .iter()
        .find(|meta| {
            meta.get(key)
                .is_some_and(|found| found.eq_ignore_ascii_case(value))
        })
        .and_then(|meta| meta.get("content"))
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
}

/// The text of one element, stripped of nested tags and collapsed to single
/// spaces, which is how a title with a `<span>` in it should read.
fn element_text(pattern: &Regex, html: &str) -> Option<String> {
    let raw = pattern.captures(html)?.get(1)?.as_str();
    let stripped = TAGS.replace_all(raw, " ");
    let text = decode(&stripped)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Some(text).filter(|text| !text.is_empty())
}

/// The five named entities that actually turn up in titles and descriptions,
/// plus numeric references. A full entity table would be a dependency for the
/// sake of characters an audit never sees.
pub fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find(';').filter(|end| *end <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let replacement = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            // The typographic entities a hand-written title actually contains.
            // A full table would be a dependency for characters an audit never
            // meets; these are the ones it meets constantly.
            "mdash" => Some('\u{2014}'),
            "ndash" => Some('\u{2013}'),
            "hellip" => Some('\u{2026}'),
            "lsquo" => Some('\u{2018}'),
            "rsquo" => Some('\u{2019}'),
            "ldquo" => Some('\u{201C}'),
            "rdquo" => Some('\u{201D}'),
            "middot" => Some('\u{00B7}'),
            "bull" => Some('\u{2022}'),
            "times" => Some('\u{00D7}'),
            "copy" => Some('\u{00A9}'),
            "reg" => Some('\u{00AE}'),
            "trade" => Some('\u{2122}'),
            _ => entity
                .strip_prefix('#')
                .and_then(|number| match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => number.parse().ok(),
                })
                .and_then(char::from_u32),
        };
        match replacement {
            Some(character) => {
                out.push(character);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Resolve a possibly relative URL against the page it was found on, so a
/// `href="/about"` is reported as something a reader can open.
fn resolved(value: Option<String>, base: &Url) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    base.join(trimmed).ok().map(|url| url.to_string())
}

/// Parse one page.
pub fn parse(html: &str, base: &Url) -> Metadata {
    let metas: Vec<HashMap<String, String>> = META
        .find_iter(html)
        .map(|found| attributes(found.as_str()))
        .collect();
    let links: Vec<HashMap<String, String>> = LINK
        .find_iter(html)
        .map(|found| attributes(found.as_str()))
        .collect();
    let html_tag = HTML.find(html).map(|found| attributes(found.as_str()));

    let body = element_text(&BODY, &NON_PROSE.replace_all(html, " ")).unwrap_or_default();

    Metadata {
        title: element_text(&TITLE, html),
        description: meta_content(&metas, "name", "description"),
        canonical_url: resolved(
            links
                .iter()
                .find(|link| {
                    link.get("rel").is_some_and(|rel| {
                        rel.split_whitespace()
                            .any(|token| token.eq_ignore_ascii_case("canonical"))
                    })
                })
                .and_then(|link| link.get("href"))
                .cloned(),
            base,
        ),
        robots: meta_content(&metas, "name", "robots"),
        language: html_tag
            .and_then(|tag| tag.get("lang").cloned())
            .filter(|lang| !lang.trim().is_empty()),
        heading: element_text(&H1, html),
        open_graph_title: meta_content(&metas, "property", "og:title"),
        open_graph_description: meta_content(&metas, "property", "og:description"),
        open_graph_image_url: resolved(meta_content(&metas, "property", "og:image"), base),
        open_graph_type: meta_content(&metas, "property", "og:type"),
        twitter_card: meta_content(&metas, "name", "twitter:card"),
        twitter_title: meta_content(&metas, "name", "twitter:title"),
        twitter_description: meta_content(&metas, "name", "twitter:description"),
        twitter_image_url: resolved(meta_content(&metas, "name", "twitter:image"), base),
        word_count: body.split_whitespace().count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.com/docs/page").unwrap()
    }

    const PAGE: &str = r#"
<!doctype html>
<html lang="en-GB">
<head>
  <title>Veronica &mdash; a control center for Ubuntu</title>
  <meta name="description" content="Everything in one app.">
  <meta name="robots" content="index,follow">
  <link rel="stylesheet" href="/style.css">
  <link rel="canonical" href="/docs/page">
  <meta property="og:title" content="Veronica">
  <meta property="og:description" content="A control center.">
  <meta property="og:image" content="../card.png">
  <meta property="og:type" content="website">
  <meta name="twitter:card" content="summary_large_image">
</head>
<body>
  <h1>Veronica <span>for Ubuntu</span></h1>
  <p>One app, five words here.</p>
  <script>const noise = "these words are not prose";</script>
  <style>.a { content: "nor these"; }</style>
</body>
</html>"#;

    #[test]
    fn the_head_is_read_field_by_field() {
        let meta = parse(PAGE, &base());
        assert_eq!(
            meta.title.as_deref(),
            Some("Veronica — a control center for Ubuntu")
        );
        assert_eq!(meta.description.as_deref(), Some("Everything in one app."));
        assert_eq!(meta.robots.as_deref(), Some("index,follow"));
        assert_eq!(meta.language.as_deref(), Some("en-GB"));
        assert_eq!(meta.open_graph_title.as_deref(), Some("Veronica"));
        assert_eq!(meta.open_graph_type.as_deref(), Some("website"));
        assert_eq!(meta.twitter_card.as_deref(), Some("summary_large_image"));
    }

    #[test]
    fn a_relative_canonical_and_image_are_resolved_against_the_page() {
        // Reporting `../card.png` would be useless to anyone reading the audit.
        let meta = parse(PAGE, &base());
        assert_eq!(
            meta.canonical_url.as_deref(),
            Some("https://example.com/docs/page")
        );
        assert_eq!(
            meta.open_graph_image_url.as_deref(),
            Some("https://example.com/card.png")
        );
    }

    #[test]
    fn the_canonical_link_is_found_among_other_link_tags() {
        // A stylesheet link comes first; matching the first `link` would take it.
        assert!(parse(PAGE, &base())
            .canonical_url
            .is_some_and(|url| !url.contains("style.css")));
    }

    #[test]
    fn a_rel_with_several_tokens_still_counts_as_canonical() {
        let html = r#"<link rel="alternate canonical" href="/x">"#;
        assert_eq!(
            parse(html, &base()).canonical_url.as_deref(),
            Some("https://example.com/x")
        );
    }

    #[test]
    fn a_heading_with_a_nested_element_reads_as_one_line() {
        assert_eq!(
            parse(PAGE, &base()).heading.as_deref(),
            Some("Veronica for Ubuntu")
        );
    }

    #[test]
    fn script_and_style_are_not_prose() {
        // Counting them would make a page of JavaScript look like a page of copy.
        let meta = parse(PAGE, &base());
        assert_eq!(meta.word_count, 8, "got {}", meta.word_count);
    }

    #[test]
    fn a_missing_field_is_none_rather_than_an_empty_string() {
        let meta = parse("<html><head></head><body></body></html>", &base());
        assert_eq!(meta.title, None);
        assert_eq!(meta.description, None);
        assert_eq!(meta.canonical_url, None);
        assert_eq!(meta.language, None);
        assert_eq!(meta.heading, None);
        assert_eq!(meta.word_count, 0);
    }

    #[test]
    fn an_empty_content_attribute_counts_as_missing() {
        // `<meta name="description" content="">` is not a description.
        let html = r#"<meta name="description" content="  ">"#;
        assert_eq!(parse(html, &base()).description, None);
    }

    #[test]
    fn attribute_names_and_values_match_regardless_of_case_and_quoting() {
        let html = r#"<META NAME='Description' CONTENT='Single quoted'>"#;
        assert_eq!(
            parse(html, &base()).description.as_deref(),
            Some("Single quoted")
        );
    }

    #[test]
    fn entities_are_decoded_in_the_fields_people_read() {
        assert_eq!(decode("Tom &amp; Jerry"), "Tom & Jerry");
        assert_eq!(decode("&lt;tag&gt;"), "<tag>");
        assert_eq!(decode("&quot;quoted&quot;"), "\"quoted\"");
        assert_eq!(decode("caf&#233;"), "café");
        assert_eq!(decode("&#x2014;"), "—");
    }

    #[test]
    fn something_that_is_not_an_entity_is_left_alone() {
        // A bare ampersand in a title is common and must survive intact.
        assert_eq!(decode("Rock & Roll"), "Rock & Roll");
        assert_eq!(decode("a &notanentity; b"), "a &notanentity; b");
        assert_eq!(decode("100% & rising"), "100% & rising");
    }

    #[test]
    fn markup_that_is_broken_produces_a_partial_read_rather_than_a_panic() {
        // Half the web is malformed; an audit that crashes on it is useless.
        for html in [
            "<html lang=",
            "<title>unclosed",
            "<meta name=description content=unquoted>",
            "",
            "<<<>>>",
        ] {
            let _ = parse(html, &base());
        }
    }
}
