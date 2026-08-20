//! Recovering a meeting link from an event's text.
//!
//! Edith offers one-tap join links. GNOME's calendar server does not pass the
//! location or description through, so when a link is present it has to be
//! recognised from whatever text is available.

/// Hosts whose URLs are meeting links rather than ordinary web pages.
const MEETING_HOSTS: &[&str] = &[
    "meet.google.com",
    "zoom.us",
    "teams.microsoft.com",
    "teams.live.com",
    "whereby.com",
    "meet.jit.si",
    "webex.com",
    "chime.aws",
    "discord.gg",
    "slack.com",
    "around.co",
    "gather.town",
];

/// Find the first meeting URL in a block of text.
pub fn extract(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(trim_punctuation)
        .filter(|token| token.starts_with("https://") || token.starts_with("http://"))
        .find(|url| is_meeting_url(url))
        .map(str::to_string)
}

/// Whether a URL points at a known meeting service.
pub fn is_meeting_url(url: &str) -> bool {
    let Some(host) = host_of(url) else {
        return false;
    };
    MEETING_HOSTS.iter().any(|candidate| {
        // Match the host or any subdomain of it, but never a host that merely
        // ends with the same letters, so "notzoom.us" is not a match.
        host == *candidate || host.ends_with(&format!(".{candidate}"))
    })
}

fn host_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?;
    // Drop any userinfo and port.
    let host = host.rsplit('@').next()?;
    let host = host.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// Strip trailing punctuation a URL picks up from prose, e.g. "…1234567890."
fn trim_punctuation(token: &str) -> &str {
    token.trim_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | ')' | '(' | '<' | '>' | '"' | '\''))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_bare_meeting_url() {
        assert_eq!(
            extract("https://meet.google.com/abc-defg-hij").as_deref(),
            Some("https://meet.google.com/abc-defg-hij")
        );
    }

    #[test]
    fn finds_a_link_inside_prose_and_trims_trailing_punctuation() {
        assert_eq!(
            extract("Join at https://zoom.us/j/1234567890. See you there").as_deref(),
            Some("https://zoom.us/j/1234567890")
        );
        assert_eq!(
            extract("Call (https://meet.jit.si/room) now").as_deref(),
            Some("https://meet.jit.si/room")
        );
    }

    #[test]
    fn matches_subdomains_of_a_meeting_host() {
        assert!(is_meeting_url("https://acme.zoom.us/j/123"));
        assert!(is_meeting_url("https://teams.microsoft.com/l/meetup-join/x"));
    }

    #[test]
    fn does_not_match_a_host_that_merely_ends_with_the_same_letters() {
        assert!(!is_meeting_url("https://notzoom.us/j/123"));
        assert!(!is_meeting_url("https://evilmeet.google.com.attacker.test/x"));
    }

    #[test]
    fn ignores_ordinary_links() {
        assert_eq!(extract("Agenda at https://example.com/doc"), None);
        assert_eq!(extract("no links here at all"), None);
        assert_eq!(extract(""), None);
    }

    #[test]
    fn ignores_a_url_with_no_host() {
        assert!(!is_meeting_url("https://"));
        assert!(!is_meeting_url("mailto:someone@zoom.us"));
    }

    #[test]
    fn strips_userinfo_and_port_before_matching() {
        assert!(is_meeting_url("https://user@meet.google.com:443/abc"));
    }

    #[test]
    fn picks_the_first_meeting_link_when_several_urls_are_present() {
        let text = "Docs https://example.com/x then https://zoom.us/j/1 or https://meet.jit.si/2";
        assert_eq!(extract(text).as_deref(), Some("https://zoom.us/j/1"));
    }
}
