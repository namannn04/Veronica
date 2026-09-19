//! The audit rules.
//!
//! A direct port of Edith's `SEOIssueAnalyzer`: the same eleven checks, the
//! same codes, the same severities, and the same thresholds — 30 to 60
//! characters for a title, 70 to 160 for a description. Keeping the numbers
//! identical is the point: a page that passes on macOS has to pass here.

use serde::Serialize;
use url::Url;

use crate::metadata::Metadata;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// Search engines will not index this, or will index the wrong thing.
    Error,
    /// Indexed, but the result will read badly.
    Warning,
    /// Worth knowing, not worth blocking a release for.
    Notice,
}

impl Severity {
    pub fn title(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Notice => "notice",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub code: &'static str,
    pub severity: Severity,
    pub title: String,
    pub detail: &'static str,
}

/// Edith's title window, in characters.
pub const TITLE_RANGE: std::ops::RangeInclusive<usize> = 30..=60;
/// And its description window.
pub const DESCRIPTION_RANGE: std::ops::RangeInclusive<usize> = 70..=160;

fn issue(code: &'static str, severity: Severity, title: String, detail: &'static str) -> Issue {
    Issue {
        code,
        severity,
        title,
        detail,
    }
}

/// Every issue one page has.
pub fn issues(url: &Url, status: Option<u16>, metadata: &Metadata) -> Vec<Issue> {
    let mut found = Vec::new();

    if let Some(status) = status {
        if !(200..300).contains(&status) {
            found.push(issue(
                "http-status",
                Severity::Error,
                format!("Page returned HTTP {status}"),
                "Search engines may not index this response.",
            ));
        }
    }

    match &metadata.title {
        None => found.push(issue(
            "title-missing",
            Severity::Error,
            "Title is missing".to_string(),
            "Add a unique title that describes this page.",
        )),
        Some(title) => {
            // Characters, not bytes: a title in Japanese is not four times as
            // long as it looks.
            let length = title.chars().count();
            if !TITLE_RANGE.contains(&length) {
                found.push(issue(
                    "title-length",
                    Severity::Warning,
                    format!("Title length is {length} characters"),
                    "Aim for 30 to 60 characters.",
                ));
            }
        }
    }

    match &metadata.description {
        None => found.push(issue(
            "description-missing",
            Severity::Warning,
            "Meta description is missing".to_string(),
            "Add a concise summary for search results.",
        )),
        Some(description) => {
            let length = description.chars().count();
            if !DESCRIPTION_RANGE.contains(&length) {
                found.push(issue(
                    "description-length",
                    Severity::Notice,
                    format!("Description length is {length} characters"),
                    "Aim for 70 to 160 characters.",
                ));
            }
        }
    }

    if metadata.canonical_url.is_none() {
        found.push(issue(
            "canonical-missing",
            Severity::Warning,
            "Canonical URL is missing".to_string(),
            "Declare the preferred URL for this page.",
        ));
    }
    if metadata.heading.is_none() {
        found.push(issue(
            "heading-missing",
            Severity::Warning,
            "H1 is missing".to_string(),
            "Add one primary heading.",
        ));
    }
    if metadata.language.is_none() {
        found.push(issue(
            "language-missing",
            Severity::Notice,
            "Document language is missing".to_string(),
            "Set the lang attribute on the HTML element.",
        ));
    }
    if metadata.open_graph_title.is_none() || metadata.open_graph_description.is_none() {
        found.push(issue(
            "open-graph-copy",
            Severity::Warning,
            "Open Graph copy is incomplete".to_string(),
            "Set both og:title and og:description.",
        ));
    }
    if metadata.open_graph_image_url.is_none() {
        found.push(issue(
            "open-graph-image",
            Severity::Warning,
            "Open Graph image is missing".to_string(),
            "Add an og:image for shared links.",
        ));
    }
    if metadata.twitter_card.is_none() {
        found.push(issue(
            "twitter-card",
            Severity::Notice,
            "X card type is missing".to_string(),
            "Set twitter:card for consistent previews.",
        ));
    }

    // A local development server is not a public page, so its lack of TLS is
    // not a finding.
    let host = url.host_str().unwrap_or_default();
    if url.scheme() != "https" && host != "localhost" && host != "127.0.0.1" {
        found.push(issue(
            "https",
            Severity::Error,
            "Page is not using HTTPS".to_string(),
            "Serve public pages over HTTPS.",
        ));
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    /// A page with nothing wrong with it.
    fn clean() -> Metadata {
        Metadata {
            title: Some("A title that is comfortably inside the range".to_string()),
            description: Some(
                "A description that is long enough to be useful in a search result and \
                 short enough to survive being shown."
                    .to_string(),
            ),
            canonical_url: Some("https://example.com/".to_string()),
            language: Some("en".to_string()),
            heading: Some("A heading".to_string()),
            open_graph_title: Some("Title".to_string()),
            open_graph_description: Some("Description".to_string()),
            open_graph_image_url: Some("https://example.com/card.png".to_string()),
            twitter_card: Some("summary".to_string()),
            ..Metadata::default()
        }
    }

    fn codes(issues: &[Issue]) -> Vec<&str> {
        issues.iter().map(|issue| issue.code).collect()
    }

    #[test]
    fn a_page_with_nothing_wrong_reports_nothing() {
        let found = issues(&url("https://example.com/"), Some(200), &clean());
        assert!(found.is_empty(), "got {found:#?}");
    }

    #[test]
    fn a_missing_title_is_an_error_and_a_short_one_is_a_warning() {
        let mut meta = clean();
        meta.title = None;
        let found = issues(&url("https://example.com/"), Some(200), &meta);
        assert_eq!(codes(&found), ["title-missing"]);
        assert_eq!(found[0].severity, Severity::Error);

        meta.title = Some("Short".to_string());
        let found = issues(&url("https://example.com/"), Some(200), &meta);
        assert_eq!(codes(&found), ["title-length"]);
        assert_eq!(found[0].severity, Severity::Warning);
        assert!(found[0].title.contains('5'), "the length is named");
    }

    #[test]
    fn ediths_thresholds_are_kept_exactly() {
        // A page that passes on macOS has to pass here.
        assert_eq!(*TITLE_RANGE.start(), 30);
        assert_eq!(*TITLE_RANGE.end(), 60);
        assert_eq!(*DESCRIPTION_RANGE.start(), 70);
        assert_eq!(*DESCRIPTION_RANGE.end(), 160);
    }

    #[test]
    fn the_boundaries_themselves_pass() {
        let mut meta = clean();
        for length in [30, 60] {
            meta.title = Some("x".repeat(length));
            assert!(
                !codes(&issues(&url("https://example.com/"), Some(200), &meta))
                    .contains(&"title-length"),
                "{length} characters should be inside the range"
            );
        }
        meta.title = Some("x".repeat(61));
        assert!(
            codes(&issues(&url("https://example.com/"), Some(200), &meta))
                .contains(&"title-length")
        );
    }

    #[test]
    fn length_is_counted_in_characters_not_bytes() {
        // A title in Japanese is not four times as long as it looks.
        let mut meta = clean();
        meta.title = Some("あ".repeat(40));
        assert!(
            !codes(&issues(&url("https://example.com/"), Some(200), &meta))
                .contains(&"title-length"),
            "40 characters is inside the range whatever they encode to"
        );
    }

    #[test]
    fn a_non_success_status_is_an_error_and_a_success_is_not() {
        let mut meta = clean();
        meta.title = None; // so the list is not empty either way
        for status in [200u16, 204, 299] {
            assert!(
                !codes(&issues(&url("https://example.com/"), Some(status), &meta))
                    .contains(&"http-status"),
                "{status} is a success"
            );
        }
        for status in [301u16, 404, 500] {
            let found = issues(&url("https://example.com/"), Some(status), &meta);
            assert!(codes(&found).contains(&"http-status"), "{status}");
        }
    }

    #[test]
    fn a_page_that_could_not_be_reached_reports_no_status_issue() {
        // The caller reports the failure itself; a second one would be noise.
        let found = issues(&url("https://example.com/"), None, &clean());
        assert!(found.is_empty());
    }

    #[test]
    fn plain_http_is_an_error_except_on_a_development_server() {
        assert!(
            codes(&issues(&url("http://example.com/"), Some(200), &clean())).contains(&"https")
        );
        for local in ["http://localhost:3000/", "http://127.0.0.1:8080/"] {
            assert!(
                !codes(&issues(&url(local), Some(200), &clean())).contains(&"https"),
                "{local} is a development server, not a public page"
            );
        }
    }

    #[test]
    fn incomplete_open_graph_copy_is_one_issue_not_two() {
        let mut meta = clean();
        meta.open_graph_description = None;
        let found = issues(&url("https://example.com/"), Some(200), &meta);
        assert_eq!(codes(&found), ["open-graph-copy"]);
    }

    #[test]
    fn every_rule_has_a_distinct_code() {
        // The code is what a report groups by, so two rules sharing one would
        // silently merge.
        let bare = Metadata::default();
        let found = issues(&url("http://example.com/"), Some(500), &bare);
        let mut seen = codes(&found);
        let count = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), count, "duplicate code in {found:#?}");
        assert_eq!(count, 10, "an empty page over plain http hits ten rules");
    }
}
