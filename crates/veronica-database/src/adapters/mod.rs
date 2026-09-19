//! Talking to an actual database.
//!
//! One trait per operation Veronica performs, implemented once per product. The
//! shape is deliberate: an adapter turns a product's own vocabulary into the
//! models in this crate and does nothing else. It makes no policy decisions —
//! whether a mutation is *allowed* is settled by the guard before an adapter is
//! ever handed a payload — and it never sees a confirmation token.
//!
//! Two rules every adapter follows:
//!
//! - **Identifiers are quoted, values are parameters.** A table name goes
//!   through the product's own quoting function; a value never reaches a
//!   statement at all. That is what makes a table called `"; DROP TABLE --` a
//!   table with a silly name rather than an incident.
//! - **Every read is bounded.** A page has a ceiling, and the adapter asks for
//!   one row more than the page so "is there more" is known rather than
//!   guessed.

pub mod mysql;
pub mod postgres;
pub mod redis;
pub mod sqlite;

use anyhow::Result;

use crate::identify::ObjectIdentifier;
use crate::mutation::Payload;
use crate::paging::{Page, PageRequest};
use crate::product::ProductIdentity;

/// What a connected database can be asked.
#[async_trait::async_trait]
pub trait Adapter: Send {
    /// What the server turned out to be.
    async fn identify(&mut self) -> Result<ProductIdentity>;

    /// The children of an object, or the top level when `parent` is `None`.
    async fn objects(&mut self, parent: Option<&ObjectIdentifier>)
        -> Result<Vec<ObjectIdentifier>>;

    /// A bounded page of one object's records.
    async fn read(&mut self, object: &ObjectIdentifier, request: &PageRequest) -> Result<Page>;

    /// Run a statement the user wrote, bounded the same way.
    ///
    /// Read-only by contract: an adapter refuses anything that would write, so
    /// a mutation cannot reach the server without going through the guard.
    async fn query(&mut self, statement: &str, request: &PageRequest) -> Result<Page>;

    /// How many records a mutation would touch, where the product can say
    /// without performing it. `None` is a real answer and the preview shows it.
    async fn count(&mut self, object: &ObjectIdentifier) -> Result<Option<u64>>;

    /// Apply an authorised mutation. Returns how many records it affected.
    ///
    /// Reaching this method means the guard has already verified a signed,
    /// unexpired, single-use token against this exact payload.
    async fn execute(&mut self, payload: &Payload) -> Result<u64>;
}

/// Whether a statement only reads.
///
/// Conservative by construction: it recognises the handful of verbs that are
/// definitely reads and refuses everything else, rather than trying to spot
/// writes and letting through whatever it failed to think of. A new statement
/// type added to a product next year is refused by default, which is the
/// direction an error should fall in.
pub fn is_read_only_sql(statement: &str) -> bool {
    let stripped = strip_sql_comments(statement);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return false;
    }
    // More than one statement is refused outright: `SELECT 1; DROP TABLE x` is
    // the oldest trick there is, and no read needs a semicolon in the middle.
    // A semicolon inside a string literal is data, not a separator.
    let body = trimmed.strip_suffix(';').unwrap_or(trimmed);
    if has_statement_separator(body) {
        return false;
    }
    let first = body
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    match first.as_str() {
        "SELECT" | "SHOW" | "DESCRIBE" | "DESC" | "EXPLAIN" | "PRAGMA" | "VALUES" => true,
        // A CTE can end in INSERT/UPDATE/DELETE, so `WITH` is only a read when
        // nothing inside it writes.
        "WITH" => {
            let upper = body.to_ascii_uppercase();
            ![
                "INSERT", "UPDATE", "DELETE", "MERGE", "DROP", "CREATE", "ALTER", "TRUNCATE",
            ]
            .iter()
            .any(|verb| contains_word(&upper, verb))
        }
        _ => false,
    }
}

/// Whether a semicolon appears outside a string or a quoted identifier.
///
/// The distinction matters both ways: refusing `WHERE note = 'a;b'` would
/// block an ordinary query, and accepting `SELECT 1; DROP TABLE x` would be the
/// injection this check exists to stop.
pub fn has_statement_separator(statement: &str) -> bool {
    let mut chars = statement.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    while let Some(character) = chars.next() {
        match character {
            // SQL escapes a quote by doubling it, so `''` inside a string is a
            // literal quote and does not close it.
            '\'' if !in_double => {
                if in_single && chars.peek() == Some(&'\'') {
                    chars.next();
                } else {
                    in_single = !in_single;
                }
            }
            '"' if !in_single => {
                if in_double && chars.peek() == Some(&'"') {
                    chars.next();
                } else {
                    in_double = !in_double;
                }
            }
            ';' if !in_single && !in_double => return true,
            _ => {}
        }
    }
    false
}

/// Whether a word appears as a word, not inside an identifier.
fn contains_word(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(index, _)| {
        let before = index
            .checked_sub(1)
            .and_then(|i| haystack.as_bytes().get(i))
            .copied();
        let after = haystack.as_bytes().get(index + needle.len()).copied();
        let boundary = |byte: Option<u8>| {
            byte.is_none_or(|byte| !byte.is_ascii_alphanumeric() && byte != b'_')
        };
        boundary(before) && boundary(after)
    })
}

/// Remove comments before looking at a statement.
///
/// Without this, `--\nDROP TABLE x` and `/*SELECT*/DROP TABLE x` both slip past
/// a check on the first word.
pub fn strip_sql_comments(statement: &str) -> String {
    let mut out = String::with_capacity(statement.len());
    let mut chars = statement.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    while let Some(character) = chars.next() {
        match character {
            '\'' if !in_double => {
                in_single = !in_single;
                out.push(character);
            }
            '"' if !in_single => {
                in_double = !in_double;
                out.push(character);
            }
            '-' if !in_single && !in_double && chars.peek() == Some(&'-') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if !in_single && !in_double && chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
                // A comment separates tokens, so it becomes a space rather than
                // joining `SELECT/**/1` into one word.
                out.push(' ');
            }
            _ => out.push(character),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ordinary_read_verbs_are_allowed() {
        for statement in [
            "SELECT * FROM orders",
            "  select 1  ",
            "SHOW TABLES",
            "EXPLAIN SELECT 1",
            "PRAGMA table_info(orders)",
            "DESCRIBE orders",
            "VALUES (1)",
            "SELECT * FROM orders;",
        ] {
            assert!(
                is_read_only_sql(statement),
                "{statement:?} should be a read"
            );
        }
    }

    #[test]
    fn anything_that_writes_is_refused() {
        for statement in [
            "DELETE FROM orders",
            "UPDATE orders SET total = 0",
            "INSERT INTO orders VALUES (1)",
            "DROP TABLE orders",
            "TRUNCATE orders",
            "ALTER TABLE orders ADD COLUMN x INT",
            "CREATE TABLE x (a INT)",
            "GRANT ALL ON orders TO nobody",
            "VACUUM",
        ] {
            assert!(
                !is_read_only_sql(statement),
                "{statement:?} slipped through"
            );
        }
    }

    #[test]
    fn an_unrecognised_verb_is_refused_rather_than_allowed() {
        // A statement type added to a product next year must fail closed.
        assert!(!is_read_only_sql("REINDEX orders"));
        assert!(!is_read_only_sql("SOMETHINGNEW FROM orders"));
        assert!(!is_read_only_sql(""));
        assert!(!is_read_only_sql("   "));
    }

    #[test]
    fn a_second_statement_is_refused() {
        // The oldest trick there is.
        assert!(!is_read_only_sql("SELECT 1; DROP TABLE orders"));
        assert!(!is_read_only_sql("SELECT 1;DROP TABLE orders;"));
        // A single trailing semicolon is fine.
        assert!(is_read_only_sql("SELECT 1;"));
    }

    #[test]
    fn a_comment_cannot_hide_a_write() {
        assert!(!is_read_only_sql("-- SELECT\nDROP TABLE orders"));
        assert!(!is_read_only_sql("/* SELECT */ DROP TABLE orders"));
        assert!(!is_read_only_sql("/*SELECT*/DELETE FROM orders"));
        // And a comment on a genuine read does not break it.
        assert!(is_read_only_sql("-- recent orders\nSELECT * FROM orders"));
        assert!(is_read_only_sql("SELECT 1 /* trailing */"));
    }

    #[test]
    fn a_writing_cte_is_refused_and_a_reading_one_is_not() {
        assert!(is_read_only_sql(
            "WITH recent AS (SELECT * FROM orders) SELECT * FROM recent"
        ));
        assert!(!is_read_only_sql(
            "WITH gone AS (DELETE FROM orders RETURNING *) SELECT * FROM gone"
        ));
        assert!(!is_read_only_sql(
            "WITH x AS (SELECT 1) INSERT INTO y SELECT * FROM x"
        ));
    }

    #[test]
    fn a_column_named_after_a_verb_does_not_make_a_cte_a_write() {
        // `deleted_at` contains "DELETE", and a substring check would refuse a
        // perfectly ordinary query.
        assert!(is_read_only_sql(
            "WITH live AS (SELECT * FROM orders WHERE deleted_at IS NULL) SELECT * FROM live"
        ));
        assert!(is_read_only_sql(
            "WITH x AS (SELECT inserted_by, updated_on FROM t) SELECT * FROM x"
        ));
    }

    #[test]
    fn a_semicolon_inside_a_string_is_not_a_second_statement() {
        assert!(is_read_only_sql("SELECT * FROM t WHERE note = 'a;b'"));
    }

    #[test]
    fn comment_markers_inside_a_string_are_left_alone() {
        let stripped = strip_sql_comments("SELECT '-- not a comment' FROM t");
        assert!(stripped.contains("-- not a comment"));
        assert!(is_read_only_sql("SELECT '-- not a comment' FROM t"));
    }

    #[test]
    fn stripping_a_comment_leaves_a_token_boundary() {
        // `SELECT/**/1` must not become `SELECT1`.
        assert_eq!(strip_sql_comments("SELECT/**/1").trim(), "SELECT 1");
    }

    #[test]
    fn an_escaped_quote_does_not_end_a_string() {
        // `'it''s'` is one string containing an apostrophe, not two strings
        // with a bare word between them.
        assert!(!has_statement_separator("SELECT 'it''s; fine' FROM t"));
        assert!(is_read_only_sql("SELECT 'it''s; fine' FROM t"));
    }

    #[test]
    fn a_semicolon_after_a_closed_string_is_still_a_separator() {
        assert!(has_statement_separator("SELECT 'a'; DROP TABLE t"));
        assert!(!is_read_only_sql("SELECT 'a'; DROP TABLE t"));
    }

    #[test]
    fn a_quoted_identifier_can_contain_a_semicolon() {
        assert!(!has_statement_separator(r#"SELECT * FROM "odd;name""#));
    }
}
