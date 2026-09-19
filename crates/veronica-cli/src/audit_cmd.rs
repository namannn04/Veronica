//! `vr audit` — Site Audit.
//!
//! Edith's Site Audit on the command line: crawl a site's sitemap, read each
//! page's metadata, and report what search engines and link previews would make
//! of it. The rules are Edith's exactly.
//!
//! Every run stays local. Veronica fetches the pages being audited and nothing
//! else — no result leaves this computer and no third-party service is
//! consulted.

use anyhow::Result;
use serde_json::json;
use veronica_audit::{Severity, DEFAULT_CONCURRENCY};

use crate::format::{self, Output};

#[derive(clap::Subcommand)]
pub enum AuditCommand {
    /// Crawl a site and report every page's issues.
    Site {
        /// A URL, a bare hostname, or a sitemap. `https://` is assumed.
        site: String,
        /// How many pages to fetch at most.
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// How many pages to fetch at once. Kept low so an audit does not read
        /// as a load test against someone else's server.
        #[arg(long, default_value_t = DEFAULT_CONCURRENCY)]
        concurrency: usize,
        /// Only show pages that have at least one issue of this severity or
        /// worse: `error`, `warning` or `notice`.
        #[arg(long)]
        severity: Option<String>,
    },
    /// Audit one page, without crawling anything.
    Page { url: String },
    /// The pages a site's sitemap lists, without auditing them.
    Sitemap {
        site: String,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

fn severity(raw: &str) -> Result<Severity> {
    match raw.to_lowercase().as_str() {
        "error" | "errors" => Ok(Severity::Error),
        "warning" | "warnings" | "warn" => Ok(Severity::Warning),
        "notice" | "notices" => Ok(Severity::Notice),
        _ => anyhow::bail!("unknown severity '{raw}'; try error, warning or notice"),
    }
}

fn parse_site(raw: &str) -> Result<url::Url> {
    url::Url::parse(raw)
        .or_else(|_| url::Url::parse(&format!("https://{raw}")))
        .map_err(|_| anyhow::anyhow!("not a URL: {raw:?}"))
}

pub async fn run(command: &AuditCommand, output: Output) -> Result<()> {
    match command {
        AuditCommand::Site {
            site,
            limit,
            concurrency,
            severity: filter,
        } => {
            // Severity is validated before the crawl, so a typo does not cost
            // fifty requests to somebody's server first.
            let floor = filter.as_deref().map(severity).transpose()?;
            let report = veronica_audit::audit(site, *limit, *concurrency).await?;

            output.emit(&report, || {
                use std::fmt::Write;
                let mut out = format!(
                    "{}\n{} page{} · {} error{} · {} warning{} · {} notice{}\n",
                    report.site,
                    report.pages.len(),
                    if report.pages.len() == 1 { "" } else { "s" },
                    report.errors,
                    if report.errors == 1 { "" } else { "s" },
                    report.warnings,
                    if report.warnings == 1 { "" } else { "s" },
                    report.notices,
                    if report.notices == 1 { "" } else { "s" },
                );

                // The site-wide counts first: one missing og:image is a page's
                // problem, forty is the template's, and that is the finding
                // worth acting on.
                let rows: Vec<Vec<String>> = report
                    .by_code
                    .iter()
                    .filter(|entry| floor.is_none_or(|floor| entry.1 <= floor))
                    .map(|entry| {
                        vec![
                            entry.2.to_string(),
                            entry.1.title().to_string(),
                            entry.0.to_string(),
                        ]
                    })
                    .collect();
                if !rows.is_empty() {
                    let _ = write!(
                        out,
                        "\n{}\n",
                        format::table(&["pages", "severity", "issue"], &rows)
                    );
                }

                for page in &report.pages {
                    let shown: Vec<_> = page
                        .issues
                        .iter()
                        .filter(|issue| floor.is_none_or(|floor| issue.severity <= floor))
                        .collect();
                    if shown.is_empty() {
                        continue;
                    }
                    let _ = write!(
                        out,
                        "\n{}{}\n",
                        page.url,
                        page.status_code
                            .map(|status| format!("  ({status})"))
                            .unwrap_or_default()
                    );
                    for issue in shown {
                        let _ = writeln!(out, "  {:<8} {}", issue.severity.title(), issue.title);
                    }
                }

                if report.errors == 0 && report.warnings == 0 && report.notices == 0 {
                    let _ = write!(out, "\nNothing to fix.");
                }
                out.trim_end().to_string()
            })
        }

        AuditCommand::Page { url } => {
            let parsed = parse_site(url)?;
            let client = reqwest::Client::builder()
                .user_agent(veronica_audit::USER_AGENT)
                .timeout(veronica_audit::PAGE_TIMEOUT)
                .build()?;
            let page = veronica_audit::audit_page(&client, &parsed).await;

            output.emit(&page, || {
                use std::fmt::Write;
                let mut out = format!("{}\n", page.url);
                if let Some(error) = &page.error {
                    let _ = write!(out, "could not be read: {error}");
                    return out;
                }
                let _ = writeln!(
                    out,
                    "HTTP {}  ·  {} ms  ·  {} bytes  ·  {} words",
                    page.status_code.unwrap_or(0),
                    page.response_millis.unwrap_or(0),
                    page.bytes,
                    page.metadata.word_count
                );
                let field = |label: &str, value: &Option<String>| {
                    vec![
                        label.to_string(),
                        value.clone().unwrap_or_else(|| "—".to_string()),
                    ]
                };
                let meta = &page.metadata;
                let _ = write!(
                    out,
                    "\n{}\n",
                    format::table(
                        &["field", "value"],
                        &[
                            field("title", &meta.title),
                            field("description", &meta.description),
                            field("canonical", &meta.canonical_url),
                            field("robots", &meta.robots),
                            field("language", &meta.language),
                            field("h1", &meta.heading),
                            field("og:title", &meta.open_graph_title),
                            field("og:description", &meta.open_graph_description),
                            field("og:image", &meta.open_graph_image_url),
                            field("twitter:card", &meta.twitter_card),
                        ]
                    )
                );
                if page.issues.is_empty() {
                    let _ = write!(out, "\nNothing to fix.");
                } else {
                    for issue in &page.issues {
                        let _ = writeln!(
                            out,
                            "\n{:<8} {}\n         {}",
                            issue.severity.title(),
                            issue.title,
                            issue.detail
                        );
                    }
                }
                out.trim_end().to_string()
            })
        }

        AuditCommand::Sitemap { site, limit } => {
            let parsed = parse_site(site)?;
            let client = reqwest::Client::builder()
                .user_agent(veronica_audit::USER_AGENT)
                .timeout(veronica_audit::PAGE_TIMEOUT)
                .build()?;
            let fetcher = veronica_audit::sitemap::HttpFetcher { client };
            let pages: Vec<String> = veronica_audit::sitemap::pages(&fetcher, &parsed)
                .await?
                .into_iter()
                .take(*limit)
                .map(|url| url.to_string())
                .collect();
            output.emit(&json!({ "site": parsed.as_str(), "pages": pages }), || {
                pages.join("\n")
            })
        }
    }
}
