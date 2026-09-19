//! Finding the pages a site says it has.
//!
//! A port of Edith's `SitemapCrawler`, with the same three candidate sources
//! tried in the same order: the URL itself when it is XML, whatever
//! `robots.txt` points at, and `/sitemap.xml` on the origin. A sitemap index is
//! followed one level into its children, and a site with no sitemap at all
//! audits the single URL it was given rather than failing.
//!
//! The XML is read with a regular expression over `<loc>` elements rather than
//! a parser. A sitemap is a flat list of URLs in one element; a full XML
//! dependency would buy nothing and would still have to handle the same
//! malformed files.

use std::collections::HashSet;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;
use url::Url;

/// Edith's ceilings, kept so a hostile or broken sitemap cannot run forever.
pub const MAX_PAGES: usize = 20_000;
pub const MAX_SITEMAPS: usize = 1_000;

static LOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<loc>\s*(.*?)\s*</loc>").expect("a literal pattern"));
static SITEMAP_INDEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<sitemapindex\b").expect("a literal pattern"));
static ROBOTS_SITEMAP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?im)^\s*sitemap\s*:\s*(\S+)\s*$").expect("a literal pattern"));

/// One parsed sitemap: the URLs it lists, and whether they are pages or more
/// sitemaps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub locations: Vec<Url>,
    /// True for a `<sitemapindex>`, whose locations are themselves sitemaps.
    pub is_index: bool,
}

/// Parse a sitemap or sitemap index.
///
/// Entries that are not URLs are dropped rather than failing the document: one
/// bad line in a generated sitemap should not lose the other nine thousand.
pub fn parse(xml: &str) -> Document {
    Document {
        locations: LOC
            .captures_iter(xml)
            .filter_map(|caps| Url::parse(crate::metadata::decode(caps[1].trim()).trim()).ok())
            .collect(),
        is_index: SITEMAP_INDEX.is_match(xml),
    }
}

/// The sitemap URLs a `robots.txt` advertises.
pub fn robots_sitemaps(robots: &str, base: &Url) -> Vec<Url> {
    ROBOTS_SITEMAP
        .captures_iter(robots)
        .filter_map(|caps| base.join(caps[1].trim()).ok())
        .collect()
}

fn unique(urls: impl IntoIterator<Item = Url>) -> Vec<Url> {
    let mut seen = HashSet::new();
    urls.into_iter()
        .filter(|url| seen.insert(url.clone()))
        .collect()
}

/// The origin, which is where `robots.txt` and `sitemap.xml` live.
pub fn origin(url: &Url) -> Option<Url> {
    let mut origin = url.clone();
    origin.set_path("");
    origin.set_query(None);
    origin.set_fragment(None);
    Some(origin)
}

/// Everything a crawl needs to fetch, so the walk itself is testable without a
/// network.
pub trait Fetch {
    fn get(&self, url: &Url) -> impl std::future::Future<Output = Result<String>> + Send;
}

/// Discover the pages to audit, starting from whatever the user typed.
pub async fn pages<F: Fetch + Sync>(fetcher: &F, input: &Url) -> Result<Vec<Url>> {
    let mut candidates = Vec::new();
    if input.path().to_lowercase().ends_with(".xml") {
        candidates.push(input.clone());
    }
    if let Some(origin) = origin(input) {
        if let Ok(robots) = fetcher.get(&origin.join("/robots.txt")?).await {
            candidates.extend(robots_sitemaps(&robots, &origin));
        }
        candidates.push(origin.join("/sitemap.xml")?);
    }

    let mut visited = HashSet::new();
    for candidate in unique(candidates) {
        let found = crawl(fetcher, &candidate, &mut visited).await;
        if !found.is_empty() {
            return Ok(unique(found).into_iter().take(MAX_PAGES).collect());
        }
    }

    // A URL that claimed to be a sitemap and turned out not to be is an error;
    // a page with no sitemap is simply a page, and auditing it alone is useful.
    if input.path().to_lowercase().ends_with(".xml") {
        anyhow::bail!("the sitemap at {input} did not contain any pages");
    }
    Ok(vec![input.clone()])
}

async fn crawl<F: Fetch + Sync>(
    fetcher: &F,
    sitemap: &Url,
    visited: &mut HashSet<Url>,
) -> Vec<Url> {
    if visited.len() >= MAX_SITEMAPS || !visited.insert(sitemap.clone()) {
        return Vec::new();
    }
    let Ok(xml) = fetcher.get(sitemap).await else {
        return Vec::new();
    };
    let document = parse(&xml);
    if !document.is_index {
        return document.locations;
    }

    // An index's children are sitemaps; one that fails is skipped rather than
    // losing the whole index.
    let mut pages = Vec::new();
    for child in document.locations {
        if pages.len() >= MAX_PAGES {
            break;
        }
        pages.extend(Box::pin(crawl(fetcher, &child, visited)).await);
    }
    unique(pages)
}

/// Fetch pages over HTTP.
pub struct HttpFetcher {
    pub client: reqwest::Client,
}

impl Fetch for HttpFetcher {
    async fn get(&self, url: &Url) -> Result<String> {
        let response = self
            .client
            .get(url.clone())
            .send()
            .await
            .with_context(|| format!("cannot reach {url}"))?
            .error_for_status()
            .with_context(|| format!("{url} did not answer with a document"))?;
        response
            .text()
            .await
            .with_context(|| format!("cannot read {url}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Canned(HashMap<String, String>);

    impl Fetch for Canned {
        async fn get(&self, url: &Url) -> Result<String> {
            self.0
                .get(url.as_str())
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("404 {url}"))
        }
    }

    fn canned(pairs: &[(&str, &str)]) -> Canned {
        Canned(
            pairs
                .iter()
                .map(|(url, body)| (url.to_string(), body.to_string()))
                .collect(),
        )
    }

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    #[test]
    fn a_sitemap_is_its_loc_elements() {
        let document = parse(
            r#"<urlset><url><loc>https://a.test/one</loc></url>
               <url><loc> https://a.test/two </loc></url></urlset>"#,
        );
        assert!(!document.is_index);
        assert_eq!(
            document
                .locations
                .iter()
                .map(Url::as_str)
                .collect::<Vec<_>>(),
            ["https://a.test/one", "https://a.test/two"]
        );
    }

    #[test]
    fn an_index_is_recognised_as_holding_sitemaps_rather_than_pages() {
        let document = parse(
            r#"<sitemapindex><sitemap><loc>https://a.test/s1.xml</loc></sitemap></sitemapindex>"#,
        );
        assert!(document.is_index);
    }

    #[test]
    fn an_entry_that_is_not_a_url_is_dropped_not_fatal() {
        // One bad line in a generated sitemap must not lose the other nine
        // thousand.
        let document = parse(
            r#"<urlset><url><loc>not a url</loc></url>
               <url><loc>https://a.test/ok</loc></url></urlset>"#,
        );
        assert_eq!(document.locations.len(), 1);
    }

    #[test]
    fn an_escaped_url_is_decoded_before_parsing() {
        let document =
            parse(r#"<urlset><url><loc>https://a.test/?a=1&amp;b=2</loc></url></urlset>"#);
        assert_eq!(document.locations[0].as_str(), "https://a.test/?a=1&b=2");
    }

    #[test]
    fn robots_sitemap_lines_are_found_whatever_their_case_and_spacing() {
        let robots =
            "User-agent: *\nDisallow:\nSitemap: https://a.test/s.xml\n  sitemap:/rel.xml\n";
        let found = robots_sitemaps(robots, &url("https://a.test/"));
        assert_eq!(
            found.iter().map(Url::as_str).collect::<Vec<_>>(),
            ["https://a.test/s.xml", "https://a.test/rel.xml"]
        );
    }

    #[tokio::test]
    async fn the_origins_sitemap_is_found_from_any_page_on_the_site() {
        let fetcher = canned(&[(
            "https://a.test/sitemap.xml",
            "<urlset><url><loc>https://a.test/one</loc></url></urlset>",
        )]);
        let pages = pages(&fetcher, &url("https://a.test/deep/page"))
            .await
            .unwrap();
        assert_eq!(
            pages.iter().map(Url::as_str).collect::<Vec<_>>(),
            ["https://a.test/one"]
        );
    }

    #[tokio::test]
    async fn robots_is_preferred_over_guessing_the_conventional_path() {
        let fetcher = canned(&[
            (
                "https://a.test/robots.txt",
                "Sitemap: https://a.test/custom.xml",
            ),
            (
                "https://a.test/custom.xml",
                "<urlset><url><loc>https://a.test/from-robots</loc></url></urlset>",
            ),
            (
                "https://a.test/sitemap.xml",
                "<urlset><url><loc>https://a.test/conventional</loc></url></urlset>",
            ),
        ]);
        let pages = pages(&fetcher, &url("https://a.test/")).await.unwrap();
        assert_eq!(pages[0].as_str(), "https://a.test/from-robots");
    }

    #[tokio::test]
    async fn an_index_is_followed_into_its_children() {
        let fetcher = canned(&[
            (
                "https://a.test/sitemap.xml",
                "<sitemapindex><sitemap><loc>https://a.test/a.xml</loc></sitemap>\
                 <sitemap><loc>https://a.test/b.xml</loc></sitemap></sitemapindex>",
            ),
            (
                "https://a.test/a.xml",
                "<urlset><url><loc>https://a.test/1</loc></url></urlset>",
            ),
            (
                "https://a.test/b.xml",
                "<urlset><url><loc>https://a.test/2</loc></url></urlset>",
            ),
        ]);
        let pages = pages(&fetcher, &url("https://a.test/")).await.unwrap();
        assert_eq!(
            pages.iter().map(Url::as_str).collect::<Vec<_>>(),
            ["https://a.test/1", "https://a.test/2"]
        );
    }

    #[tokio::test]
    async fn a_child_sitemap_that_fails_does_not_lose_the_index() {
        let fetcher = canned(&[
            (
                "https://a.test/sitemap.xml",
                "<sitemapindex><sitemap><loc>https://a.test/gone.xml</loc></sitemap>\
                 <sitemap><loc>https://a.test/b.xml</loc></sitemap></sitemapindex>",
            ),
            (
                "https://a.test/b.xml",
                "<urlset><url><loc>https://a.test/2</loc></url></urlset>",
            ),
        ]);
        let pages = pages(&fetcher, &url("https://a.test/")).await.unwrap();
        assert_eq!(
            pages.iter().map(Url::as_str).collect::<Vec<_>>(),
            ["https://a.test/2"]
        );
    }

    #[tokio::test]
    async fn a_sitemap_that_points_at_itself_terminates() {
        let fetcher = canned(&[(
            "https://a.test/sitemap.xml",
            "<sitemapindex><sitemap><loc>https://a.test/sitemap.xml</loc></sitemap></sitemapindex>",
        )]);
        // The point is that this returns at all.
        let pages = pages(&fetcher, &url("https://a.test/page")).await.unwrap();
        assert_eq!(
            pages.iter().map(Url::as_str).collect::<Vec<_>>(),
            ["https://a.test/page"]
        );
    }

    #[tokio::test]
    async fn a_site_with_no_sitemap_audits_the_page_it_was_given() {
        let fetcher = canned(&[]);
        let pages = pages(&fetcher, &url("https://a.test/one")).await.unwrap();
        assert_eq!(
            pages.iter().map(Url::as_str).collect::<Vec<_>>(),
            ["https://a.test/one"]
        );
    }

    #[tokio::test]
    async fn a_url_that_claimed_to_be_a_sitemap_and_was_not_is_an_error() {
        // Auditing sitemap.xml as if it were a page would be nonsense.
        let fetcher = canned(&[("https://a.test/s.xml", "<urlset></urlset>")]);
        assert!(pages(&fetcher, &url("https://a.test/s.xml")).await.is_err());
    }

    #[tokio::test]
    async fn duplicate_locations_are_audited_once() {
        let fetcher = canned(&[(
            "https://a.test/sitemap.xml",
            "<urlset><url><loc>https://a.test/x</loc></url>\
             <url><loc>https://a.test/x</loc></url></urlset>",
        )]);
        let pages = pages(&fetcher, &url("https://a.test/")).await.unwrap();
        assert_eq!(pages.len(), 1);
    }
}
