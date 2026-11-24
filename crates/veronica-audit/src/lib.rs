pub mod issues;
pub mod metadata;
pub mod sitemap;

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Serialize;
use url::Url;

pub use issues::{Issue, Severity};
pub use metadata::Metadata;

pub const USER_AGENT: &str = concat!("Veronica Site Audit/", env!("CARGO_PKG_VERSION"));

pub const PAGE_TIMEOUT: Duration = Duration::from_secs(45);

pub const DEFAULT_CONCURRENCY: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageResult {
    pub url: String,
    pub status_code: Option<u16>,
    pub response_millis: Option<u64>,
    pub bytes: usize,
    pub metadata: Metadata,
    pub issues: Vec<Issue>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub site: String,
    pub started_at: String,
    pub pages: Vec<PageResult>,
    pub errors: usize,
    pub warnings: usize,
    pub notices: usize,
    pub by_code: Vec<CodeCount>,
}

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

pub fn site_url(site: &str) -> Result<Url> {
    let site = site.trim();
    let candidate = if site.contains("://") {
        site.to_string()
    } else {
        let authority = site.split(['/', '?', '#']).next().unwrap_or_default();
        let host = match authority.strip_prefix('[') {
            Some(rest) => rest.split(']').next().unwrap_or_default(),
            None => authority
                .rsplit_once(':')
                .map_or(authority, |(host, _)| host),
        };
        let scheme = if is_development_host(host) {
            "http"
        } else {
            "https"
        };
        format!("{scheme}://{site}")
    };
    Url::parse(&candidate).with_context(|| format!("not a URL: {site:?}"))
}

pub fn is_development_host(host: &str) -> bool {
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_unspecified(),
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Err(_) => false,
    }
}

pub async fn audit(site: &str, limit: usize, concurrency: usize) -> Result<Report> {
    let input = site_url(site)?;
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
    let targets = sitemap::pages(&fetcher, &input, limit.max(1)).await?;
    let started_at = chrono::Utc::now().to_rfc3339();

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

    #[test]
    fn a_bare_address_gets_the_scheme_its_host_speaks() {
        let cases = [
            ("localhost:3000", "http://localhost:3000/"),
            ("127.0.0.1:8000/docs", "http://127.0.0.1:8000/docs"),
            ("192.168.1.20", "http://192.168.1.20/"),
            ("[::1]:8080", "http://[::1]:8080/"),
            ("printer.local", "http://printer.local/"),
            ("example.com:8080", "https://example.com:8080/"),
            ("example.com", "https://example.com/"),
            ("http://example.com", "http://example.com/"),
            ("  https://localhost:3000  ", "https://localhost:3000/"),
        ];
        for (typed, expected) in cases {
            assert_eq!(site_url(typed).unwrap().as_str(), expected, "{typed:?}");
        }
    }

    #[test]
    fn development_hosts_are_told_apart_from_public_ones() {
        for host in [
            "localhost",
            "app.localhost",
            "10.0.0.5",
            "172.16.0.1",
            "::1",
            "[::1]",
            "0.0.0.0",
        ] {
            assert!(is_development_host(host), "{host}");
        }
        for host in [
            "example.com",
            "8.8.8.8",
            "172.32.0.1",
            "localhost.example.com",
        ] {
            assert!(!is_development_host(host), "{host}");
        }
    }

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
        assert!(USER_AGENT.starts_with("Veronica Site Audit/"));
        assert!(USER_AGENT.len() > "Veronica Site Audit/".len());
    }
}
