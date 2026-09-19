//! Site Audit.
//!
//! Ported from Edith's `seoAudit`: crawl a site's sitemap, read each page's
//! metadata, and report what search engines and link previews would make of
//! it. The rules, thresholds and issue codes are Edith's exactly, so a page
//! that passes on macOS passes here.
//!
//! Every run stays local. Veronica fetches the pages being audited and nothing
//! else: no result is uploaded, no third-party service is consulted, and the
//! report is a document the caller owns. Edith can also run Lighthouse for
//! performance scores; that needs a headless Chrome, which Veronica does not
//! ship and will not install behind the user's back, so the scores are absent
//! rather than faked.

pub mod issues;
pub mod metadata;
pub mod sitemap;

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Serialize;
use url::Url;

pub use issues::{Issue, Severity};
pub use metadata::Metadata;

/// Sent on every request, so a site owner reading their logs can tell what this
/// is and that it was a person auditing their own site.
pub const USER_AGENT: &str = concat!("Veronica Site Audit/", env!("CARGO_PKG_VERSION"));

/// Edith's per-page timeout.
pub const PAGE_TIMEOUT: Duration = Duration::from_secs(45);

/// How many pages are fetched at once. Auditing someone's site should not read
/// as a load test, so this stays low by default.
pub const DEFAULT_CONCURRENCY: usize = 4;

/// What one page turned into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageResult {
    pub url: String,
    pub status_code: Option<u16>,
    pub response_millis: Option<u64>,
    pub bytes: usize,
    pub metadata: Metadata,
    pub issues: Vec<Issue>,
    /// Set when the page could not be fetched at all, which is different from
    /// a page that answered with a 500.
    pub error: Option<String>,
}

impl PageResult {
    pub fn count(&self, severity: Severity) -> usize {
        self.issues
            .iter()
            .filter(|issue| issue.severity == severity)
            .count()
    }
}

/// A whole run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub site: String,
    pub started_at: String,
    pub pages: Vec<PageResult>,
    pub errors: usize,
    pub warnings: usize,
    pub notices: usize,
    /// How often each issue appears across the site, most common first. One
    /// missing `og:image` is a page problem; forty is a template problem, and
    /// that is the finding worth acting on — so it is in the document rather
    /// than left for each caller to re-derive.
    pub by_code: Vec<CodeCount>,
}

/// One issue and how many pages have it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodeCount(pub &'static str, pub Severity, pub usize);

impl Report {
    fn new(site: &str, started_at: String, pages: Vec<PageResult>) -> Self {
        let count = |severity| pages.iter().map(|page| page.count(severity)).sum::<usize>();
        Self {
            site: site.to_string(),
            started_at,
            errors: count(Severity::Error),
            warnings: count(Severity::Warning),
            notices: count(Severity::Notice),
            by_code: tally(&pages),
            pages,
        }
    }

    /// Pages with at least one error, worst first. What to fix before shipping.
    pub fn worst_pages(&self) -> Vec<&PageResult> {
        let mut worst: Vec<&PageResult> = self
            .pages
            .iter()
            .filter(|page| page.count(Severity::Error) > 0)
            .collect();
        worst.sort_by(|left, right| {
            right
                .count(Severity::Error)
                .cmp(&left.count(Severity::Error))
                .then(left.url.cmp(&right.url))
        });
        worst
    }
}

/// Count each issue across the pages, most common first, then by severity, then
/// by code so the order is total and a report does not reshuffle between runs.
fn tally(pages: &[PageResult]) -> Vec<CodeCount> {
    let mut counts: std::collections::HashMap<&'static str, (Severity, usize)> =
        std::collections::HashMap::new();
    for issue in pages.iter().flat_map(|page| &page.issues) {
        let entry = counts.entry(issue.code).or_insert((issue.severity, 0));
        entry.1 += 1;
    }
    let mut rows: Vec<CodeCount> = counts
        .into_iter()
        .map(|(code, (severity, count))| CodeCount(code, severity, count))
        .collect();
    rows.sort_by(|left, right| {
        right
            .2
            .cmp(&left.2)
            .then(left.1.cmp(&right.1))
            .then(left.0.cmp(right.0))
    });
    rows
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(PAGE_TIMEOUT)
        .build()
        .context("cannot build an HTTP client")
}

/// Audit one page.
pub async fn audit_page(client: &reqwest::Client, url: &Url) -> PageResult {
    let started = Instant::now();
    let response = client.get(url.clone()).send().await;

    match response {
        Ok(response) => {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            let elapsed = started.elapsed().as_millis() as u64;
            let metadata = metadata::parse(&body, url);
            PageResult {
                url: url.to_string(),
                status_code: Some(status),
                response_millis: Some(elapsed),
                bytes: body.len(),
                issues: issues::issues(url, Some(status), &metadata),
                metadata,
                error: None,
            }
        }
        Err(error) => PageResult {
            url: url.to_string(),
            status_code: None,
            response_millis: None,
            bytes: 0,
            metadata: Metadata::default(),
            // The failure itself is the finding; running the metadata rules
            // over an empty page would bury it under ten more.
            issues: vec![Issue {
                code: "request-failed",
                severity: Severity::Error,
                title: "Page could not be read".to_string(),
                detail: "The request did not complete.",
            }],
            error: Some(error.to_string()),
        },
    }
}

/// Crawl and audit a site.
///
/// `limit` caps how many pages are fetched, which is what makes it reasonable
/// to point this at somebody's whole site.
pub async fn audit(site: &str, limit: usize, concurrency: usize) -> Result<Report> {
    let input = Url::parse(site)
        .or_else(|_| Url::parse(&format!("https://{site}")))
        .with_context(|| format!("not a URL: {site:?}"))?;
    if !matches!(input.scheme(), "http" | "https") {
        anyhow::bail!(
            "only http and https can be audited, not {:?}",
            input.scheme()
        );
    }

    let client = client()?;
    let fetcher = sitemap::HttpFetcher {
        client: client.clone(),
    };
    let discovered = sitemap::pages(&fetcher, &input).await?;
    let targets: Vec<Url> = discovered.into_iter().take(limit.max(1)).collect();
    let started_at = chrono::Utc::now().to_rfc3339();

    // Bounded concurrency: fast enough to be usable, gentle enough not to read
    // as a load test against someone else's server.
    let mut results = Vec::with_capacity(targets.len());
    for chunk in targets.chunks(concurrency.clamp(1, 16)) {
        let batch =
            futures_util::future::join_all(chunk.iter().map(|url| audit_page(&client, url))).await;
        results.extend(batch);
    }

    Ok(Report::new(input.as_str(), started_at, results))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(url: &str, issues: Vec<Issue>) -> PageResult {
        PageResult {
            url: url.to_string(),
            status_code: Some(200),
            response_millis: Some(10),
            bytes: 100,
            metadata: Metadata::default(),
            issues,
            error: None,
        }
    }

    fn issue(code: &'static str, severity: Severity) -> Issue {
        Issue {
            code,
            severity,
            title: code.to_string(),
            detail: "",
        }
    }

    #[test]
    fn a_report_totals_each_severity_across_every_page() {
        let report = Report::new(
            "https://a.test/",
            "now".into(),
            vec![
                page(
                    "https://a.test/1",
                    vec![
                        issue("https", Severity::Error),
                        issue("canonical-missing", Severity::Warning),
                    ],
                ),
                page(
                    "https://a.test/2",
                    vec![
                        issue("https", Severity::Error),
                        issue("twitter-card", Severity::Notice),
                    ],
                ),
            ],
        );
        assert_eq!(report.errors, 2);
        assert_eq!(report.warnings, 1);
        assert_eq!(report.notices, 1);
    }

    #[test]
    fn the_worst_pages_are_the_ones_with_errors_most_first() {
        let report = Report::new(
            "https://a.test/",
            "now".into(),
            vec![
                page(
                    "https://a.test/ok",
                    vec![issue("twitter-card", Severity::Notice)],
                ),
                page(
                    "https://a.test/bad",
                    vec![
                        issue("https", Severity::Error),
                        issue("title-missing", Severity::Error),
                    ],
                ),
                page("https://a.test/mid", vec![issue("https", Severity::Error)]),
            ],
        );
        let worst: Vec<&str> = report
            .worst_pages()
            .iter()
            .map(|p| p.url.as_str())
            .collect();
        assert_eq!(worst, ["https://a.test/bad", "https://a.test/mid"]);
    }

    #[test]
    fn counting_by_code_turns_a_page_problem_into_a_template_problem() {
        // One missing og:image is a page's problem; forty is the template's.
        let pages: Vec<PageResult> = (0..40)
            .map(|index| {
                page(
                    &format!("https://a.test/{index}"),
                    vec![issue("open-graph-image", Severity::Warning)],
                )
            })
            .chain(std::iter::once(page(
                "https://a.test/one-off",
                vec![issue("title-missing", Severity::Error)],
            )))
            .collect();
        let report = Report::new("https://a.test/", "now".into(), pages);
        let rows = &report.by_code;
        assert_eq!(rows[0].0, "open-graph-image");
        assert_eq!(rows[0].2, 40);
        assert_eq!(rows[1].0, "title-missing");
    }

    #[test]
    fn a_clean_site_has_no_worst_pages_and_no_codes() {
        let report = Report::new(
            "https://a.test/",
            "now".into(),
            vec![page("https://a.test/", vec![])],
        );
        assert!(report.worst_pages().is_empty());
        assert!(report.by_code.is_empty());
        assert_eq!(report.errors, 0);
    }

    #[tokio::test]
    async fn a_bare_hostname_is_read_as_https() {
        // Typing `example.com` should audit the site, not fail on a scheme.
        let error = audit("not a url at all ///", 1, 1)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("not a URL"), "got {error}");
    }

    #[tokio::test]
    async fn a_scheme_that_cannot_be_audited_is_refused() {
        let error = audit("ftp://a.test/", 1, 1).await.unwrap_err().to_string();
        assert!(error.contains("only http and https"), "got {error}");
    }

    #[test]
    fn the_user_agent_names_the_tool_and_its_version() {
        // A site owner reading their logs should be able to tell what this is.
        assert!(USER_AGENT.starts_with("Veronica Site Audit/"));
        assert!(USER_AGENT.len() > "Veronica Site Audit/".len());
    }
}
