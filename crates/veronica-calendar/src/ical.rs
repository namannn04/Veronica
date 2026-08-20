//! Just enough iCalendar to read a property.
//!
//! GNOME's calendar server expands recurrences but drops the location and
//! description, so those are fetched per event from Evolution Data Server, which
//! returns a raw `VEVENT`. Only a couple of properties are needed, so this reads
//! them directly rather than pulling in a full parser — no recurrence rules, no
//! timezone components, no calendar-level structure.

/// Undo line folding.
///
/// iCalendar wraps lines at 75 octets and marks the continuation with a leading
/// space or tab. A URL in a `LOCATION` is long enough to be folded routinely, so
/// scanning without unfolding first finds a truncated link.
pub fn unfold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        let line = line.trim_end_matches('\r');
        if index > 0 {
            match line.strip_prefix([' ', '\t']) {
                Some(continuation) => out.push_str(continuation),
                None => {
                    out.push('\n');
                    out.push_str(line);
                }
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Read a property value, ignoring any parameters on the name.
///
/// A property line is `NAME[;PARAM=VALUE...]:VALUE`, so the split has to be on
/// the first colon that follows the name, and parameters may themselves contain
/// a quoted colon.
pub fn property(text: &str, name: &str) -> Option<String> {
    let unfolded = unfold(text);
    for line in unfolded.lines() {
        let Some((head, value)) = split_property(line) else {
            continue;
        };
        // The name is everything before the first parameter separator.
        let key = head.split(';').next().unwrap_or(head);
        if key.eq_ignore_ascii_case(name) {
            return Some(unescape(value));
        }
    }
    None
}

/// Split a content line into its name-with-parameters and its value.
fn split_property(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut quoted = false;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            // A colon inside a quoted parameter value is not the separator.
            b':' if !quoted => return Some((&line[..index], &line[index + 1..])),
            _ => {}
        }
    }
    None
}

/// Undo the text escaping the spec defines for property values.
pub fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(',') => out.push(','),
            Some(';') => out.push(';'),
            // An unknown escape keeps the character as written.
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Every place a meeting link tends to hide, in the order worth trying.
///
/// Google puts the link in `X-GOOGLE-CONFERENCE`, Outlook in the description,
/// and most people paste it into the location.
pub const LINK_PROPERTIES: &[&str] = &[
    "X-GOOGLE-CONFERENCE",
    "LOCATION",
    "URL",
    "DESCRIPTION",
    "X-MICROSOFT-SKYPETEAMSMEETINGURL",
];

/// Find a meeting link anywhere in a `VEVENT`.
pub fn join_url(vevent: &str) -> Option<String> {
    for name in LINK_PROPERTIES {
        if let Some(value) = property(vevent, name) {
            if let Some(url) = crate::links::extract(&value) {
                return Some(url);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VEVENT\r\nUID:veronica-test-1\r\nDTSTART:20260820T040000\r\nSUMMARY:Standup\r\nLOCATION:https://meet.google.com/abc-defg-hij\r\nEND:VEVENT\r\n";

    #[test]
    fn reads_a_plain_property() {
        assert_eq!(property(SAMPLE, "SUMMARY").as_deref(), Some("Standup"));
        assert_eq!(
            property(SAMPLE, "LOCATION").as_deref(),
            Some("https://meet.google.com/abc-defg-hij")
        );
    }

    #[test]
    fn property_names_match_case_insensitively() {
        assert_eq!(property(SAMPLE, "summary").as_deref(), Some("Standup"));
    }

    #[test]
    fn a_missing_property_is_none() {
        assert_eq!(property(SAMPLE, "DESCRIPTION"), None);
    }

    #[test]
    fn unfolds_a_wrapped_value_before_reading_it() {
        // A long URL is folded across lines; reading without unfolding first
        // returns a truncated link.
        let folded = "BEGIN:VEVENT\r\nLOCATION:https://meet.google.com/very-long-\r\n room-identifier-here\r\nEND:VEVENT";
        assert_eq!(
            property(folded, "LOCATION").as_deref(),
            Some("https://meet.google.com/very-long-room-identifier-here")
        );
    }

    #[test]
    fn unfolds_tab_continuations_too() {
        let folded = "SUMMARY:Part one\r\n\tand part two";
        assert_eq!(property(folded, "SUMMARY").as_deref(), Some("Part oneand part two"));
    }

    #[test]
    fn ignores_parameters_on_the_property_name() {
        let line = "DTSTART;TZID=Europe/London:20260820T090000";
        assert_eq!(property(line, "DTSTART").as_deref(), Some("20260820T090000"));
    }

    #[test]
    fn a_colon_inside_a_quoted_parameter_is_not_the_separator() {
        let line = "ATTENDEE;CN=\"Smith, J: Lead\";ROLE=REQ:mailto:j@example.com";
        assert_eq!(property(line, "ATTENDEE").as_deref(), Some("mailto:j@example.com"));
    }

    #[test]
    fn unescapes_the_sequences_the_spec_defines() {
        assert_eq!(unescape(r"line one\nline two"), "line one\nline two");
        assert_eq!(unescape(r"a\, b\; c"), "a, b; c");
        assert_eq!(unescape(r"back\\slash"), r"back\slash");
        // A trailing backslash is kept rather than dropped.
        assert_eq!(unescape(r"trailing\"), r"trailing\");
    }

    #[test]
    fn finds_the_link_in_the_location() {
        assert_eq!(
            join_url(SAMPLE).as_deref(),
            Some("https://meet.google.com/abc-defg-hij")
        );
    }

    #[test]
    fn finds_a_link_buried_in_an_escaped_description() {
        let vevent = "BEGIN:VEVENT\r\nSUMMARY:Review\r\nDESCRIPTION:Agenda\\nJoin at https://zoom.us/j/999\\nBring notes\r\nEND:VEVENT";
        assert_eq!(join_url(vevent).as_deref(), Some("https://zoom.us/j/999"));
    }

    #[test]
    fn prefers_the_conference_property_over_a_location_room_name() {
        let vevent = "BEGIN:VEVENT\r\nLOCATION:https://zoom.us/j/111\r\nX-GOOGLE-CONFERENCE:https://meet.google.com/xyz\r\nEND:VEVENT";
        assert_eq!(
            join_url(vevent).as_deref(),
            Some("https://meet.google.com/xyz")
        );
    }

    #[test]
    fn a_physical_location_is_not_a_join_link() {
        let vevent = "BEGIN:VEVENT\r\nLOCATION:Meeting room 3, second floor\r\nEND:VEVENT";
        assert_eq!(join_url(vevent), None);
    }

    #[test]
    fn an_ordinary_url_in_the_description_is_not_a_join_link() {
        let vevent = "BEGIN:VEVENT\r\nDESCRIPTION:Notes at https://example.com/doc\r\nEND:VEVENT";
        assert_eq!(join_url(vevent), None);
    }
}
