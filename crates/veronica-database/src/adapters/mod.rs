pub mod mysql;
pub mod postgres;
pub mod redis;
pub mod sqlite;

use anyhow::Result;

use crate::identify::ObjectIdentifier;
use crate::mutation::Payload;
use crate::paging::{Page, PageRequest};
use crate::product::ProductIdentity;

#[async_trait::async_trait]
pub trait Adapter: Send {
    async fn identify(&mut self) -> Result<ProductIdentity>;

    async fn objects(&mut self, parent: Option<&ObjectIdentifier>)
        -> Result<Vec<ObjectIdentifier>>;

    async fn read(&mut self, object: &ObjectIdentifier, request: &PageRequest) -> Result<Page>;

    async fn query(&mut self, statement: &str, request: &PageRequest) -> Result<Page>;

    async fn count(&mut self, object: &ObjectIdentifier) -> Result<Option<u64>>;

    async fn execute(&mut self, payload: &Payload) -> Result<u64>;
}

pub fn is_read_only_sql(statement: &str) -> bool {
    let stripped = strip_sql_comments(statement);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return false;
    }
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

pub fn is_wrappable_query(statement: &str) -> bool {
    let stripped = strip_sql_comments(statement);
    let first = stripped
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(first.as_str(), "SELECT" | "WITH" | "VALUES")
}

pub fn client_window<T>(rows: impl IntoIterator<Item = T>, request: &PageRequest) -> Vec<T> {
    rows.into_iter()
        .skip(usize::try_from(request.offset).unwrap_or(usize::MAX))
        .take(request.page_size.get() as usize + 1)
        .collect()
}

pub fn has_statement_separator(statement: &str) -> bool {
    let mut chars = statement.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    while let Some(character) = chars.next() {
        match character {
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
        assert!(!is_read_only_sql("REINDEX orders"));
        assert!(!is_read_only_sql("SOMETHINGNEW FROM orders"));
        assert!(!is_read_only_sql(""));
        assert!(!is_read_only_sql("   "));
    }

    #[test]
    fn a_second_statement_is_refused() {
        assert!(!is_read_only_sql("SELECT 1; DROP TABLE orders"));
        assert!(!is_read_only_sql("SELECT 1;DROP TABLE orders;"));
        assert!(is_read_only_sql("SELECT 1;"));
    }

    #[test]
    fn only_query_expressions_are_wrapped_as_a_subquery() {
        for statement in [
            "SELECT 1",
            "  with x AS (SELECT 1) SELECT * FROM x",
            "VALUES (1)",
            "-- note\nselect 1",
        ] {
            assert!(is_wrappable_query(statement), "{statement:?}");
        }
        for statement in [
            "PRAGMA table_info(t)",
            "EXPLAIN SELECT 1",
            "SHOW TABLES",
            "DESCRIBE t",
            "desc t",
        ] {
            assert!(!is_wrappable_query(statement), "{statement:?}");
        }
    }

    #[test]
    fn a_client_window_skips_the_offset_and_keeps_one_extra_row() {
        let request = PageRequest {
            page_size: crate::paging::PageSize::new(2).unwrap(),
            offset: 1,
            ..PageRequest::default()
        };
        assert_eq!(client_window(0..10, &request), [1, 2, 3]);
        assert_eq!(client_window(0..2, &request), [1]);
    }

    #[test]
    fn a_comment_cannot_hide_a_write() {
        assert!(!is_read_only_sql("-- SELECT\nDROP TABLE orders"));
        assert!(!is_read_only_sql("/* SELECT */ DROP TABLE orders"));
        assert!(!is_read_only_sql("/*SELECT*/DELETE FROM orders"));
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
        assert_eq!(strip_sql_comments("SELECT/**/1").trim(), "SELECT 1");
    }

    #[test]
    fn an_escaped_quote_does_not_end_a_string() {
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
