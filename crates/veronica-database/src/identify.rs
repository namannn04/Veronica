//! Naming a thing inside a database.
//!
//! A port of Edith's `Identifiers.swift`. An object is a kind plus a path —
//! `table["public", "orders"]` — rather than a string, because a string has to
//! be parsed and a parser is where quoting bugs turn into the wrong table being
//! dropped. A record is the columns that identify it, so a mutation targets a
//! row rather than a rendered predicate.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::value::Value;

/// The twenty-five kinds Edith names, covering all five families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectKind {
    Server,
    Cluster,
    Node,
    Catalog,
    Database,
    Schema,
    Table,
    View,
    MaterializedView,
    Column,
    Index,
    Constraint,
    Sequence,
    Routine,
    Type,
    Role,
    Keyspace,
    Key,
    Collection,
    Alias,
    DataStream,
    Template,
    Pipeline,
    Snapshot,
    Dictionary,
    Partition,
    Part,
    Other,
}

impl ObjectKind {
    pub fn key(self) -> &'static str {
        // Derived from the serde name so the two can never disagree.
        match serde_json::to_value(self) {
            Ok(serde_json::Value::String(name)) => Box::leak(name.into_boxed_str()),
            _ => "other",
        }
    }

    /// Whether this kind holds records a query can read.
    pub fn is_queryable(self) -> bool {
        matches!(
            self,
            ObjectKind::Table
                | ObjectKind::View
                | ObjectKind::MaterializedView
                | ObjectKind::Collection
                | ObjectKind::Keyspace
                | ObjectKind::Alias
                | ObjectKind::DataStream
        )
    }
}

/// A thing, named by where it lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectIdentifier {
    pub kind: ObjectKind,
    /// Outermost first: `["public", "orders"]`. Never pre-quoted, never
    /// pre-joined — quoting is the adapter's job, once, at the edge.
    pub path: Vec<String>,
    /// What the product itself calls it, where that differs.
    pub native_identifier: Option<String>,
}

impl ObjectIdentifier {
    pub fn new(kind: ObjectKind, path: Vec<String>) -> Self {
        Self {
            kind,
            path,
            native_identifier: None,
        }
    }

    /// For display only. Never put this back into a statement: it is lossy by
    /// design, because a name containing a dot would come back as two.
    pub fn display_path(&self) -> String {
        self.path.join(".")
    }

    pub fn name(&self) -> &str {
        self.path.last().map(String::as_str).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordIdentityKind {
    PrimaryKey,
    UniqueKey,
    RowId,
    DocumentId,
    SearchDocument,
    Key,
    ExplicitPredicate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityComponent {
    pub name: String,
    pub value: Value,
}

/// Which record, and what it looked like when you selected it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordIdentity {
    pub kind: RecordIdentityKind,
    pub components: Vec<IdentityComponent>,
    /// Values that must still hold for the write to go ahead — a version
    /// column, an `xmin`, an `_etag`. Optimistic concurrency: without these a
    /// stale row edited by two people silently loses one of them.
    #[serde(default)]
    pub concurrency_tokens: Vec<IdentityComponent>,
}

/// What an operation is aimed at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetIdentifier {
    pub connection_id: Uuid,
    pub object: Option<ObjectIdentifier>,
    pub record: Option<RecordIdentity>,
}

impl TargetIdentifier {
    pub fn connection(connection_id: Uuid) -> Self {
        Self {
            connection_id,
            object: None,
            record: None,
        }
    }

    pub fn object(connection_id: Uuid, object: ObjectIdentifier) -> Self {
        Self {
            connection_id,
            object: Some(object),
            record: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_kept_as_components_not_a_joined_string() {
        // Joining and re-splitting is where a table named `a.b` becomes two.
        let object =
            ObjectIdentifier::new(ObjectKind::Table, vec!["public".into(), "my.table".into()]);
        assert_eq!(object.path.len(), 2);
        assert_eq!(object.name(), "my.table");
        // The display form is lossy, which is why it says so on the method.
        assert_eq!(object.display_path(), "public.my.table");
    }

    #[test]
    fn the_kinds_that_hold_records_are_the_ones_a_query_can_read() {
        for kind in [
            ObjectKind::Table,
            ObjectKind::View,
            ObjectKind::Collection,
            ObjectKind::Keyspace,
        ] {
            assert!(kind.is_queryable(), "{kind:?}");
        }
        for kind in [ObjectKind::Role, ObjectKind::Index, ObjectKind::Server] {
            assert!(!kind.is_queryable(), "{kind:?}");
        }
    }

    #[test]
    fn object_kinds_serialise_under_their_camel_case_names() {
        assert_eq!(
            serde_json::to_string(&ObjectKind::MaterializedView).unwrap(),
            "\"materializedView\""
        );
        assert_eq!(ObjectKind::MaterializedView.key(), "materializedView");
        assert_eq!(ObjectKind::Table.key(), "table");
    }

    #[test]
    fn a_record_identity_can_carry_concurrency_tokens() {
        // Without them, two people editing one row silently lose an edit.
        let identity = RecordIdentity {
            kind: RecordIdentityKind::PrimaryKey,
            components: vec![IdentityComponent {
                name: "id".into(),
                value: Value::SignedInteger(7),
            }],
            concurrency_tokens: vec![IdentityComponent {
                name: "version".into(),
                value: Value::SignedInteger(3),
            }],
        };
        let json = serde_json::to_string(&identity).unwrap();
        let decoded: RecordIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, identity);
        assert_eq!(decoded.concurrency_tokens.len(), 1);
    }

    #[test]
    fn an_identity_with_no_tokens_still_decodes() {
        let identity: RecordIdentity =
            serde_json::from_str(r#"{"kind":"primaryKey","components":[]}"#).unwrap();
        assert!(identity.concurrency_tokens.is_empty());
    }
}
