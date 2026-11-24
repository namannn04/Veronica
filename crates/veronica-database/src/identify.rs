use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::value::Value;

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
        match serde_json::to_value(self) {
            Ok(serde_json::Value::String(name)) => Box::leak(name.into_boxed_str()),
            _ => "other",
        }
    }

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectIdentifier {
    pub kind: ObjectKind,
    pub path: Vec<String>,
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

    pub fn for_path(product: crate::product::Product, path: Vec<String>) -> Self {
        use crate::product::{Family, Product};
        let kind = match (product.family(), path.len()) {
            (Family::KeyValue, _) => ObjectKind::Key,
            (Family::Document, 2) => ObjectKind::Collection,
            (_, 1) if product == Product::Postgresql => ObjectKind::Schema,
            (_, 1) if matches!(product, Product::Mysql | Product::MariaDb) => ObjectKind::Database,
            _ => ObjectKind::Table,
        };
        Self::new(kind, path)
    }

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordIdentity {
    pub kind: RecordIdentityKind,
    pub components: Vec<IdentityComponent>,
    #[serde(default)]
    pub concurrency_tokens: Vec<IdentityComponent>,
}

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
    fn a_bare_path_names_the_namespace_each_product_has() {
        use crate::product::Product;
        let one = || vec!["public".to_string()];
        let two = || vec!["public".to_string(), "orders".to_string()];
        assert_eq!(
            ObjectIdentifier::for_path(Product::Postgresql, one()).kind,
            ObjectKind::Schema
        );
        assert_eq!(
            ObjectIdentifier::for_path(Product::Postgresql, two()).kind,
            ObjectKind::Table
        );
        assert_eq!(
            ObjectIdentifier::for_path(Product::Mysql, one()).kind,
            ObjectKind::Database
        );
        assert_eq!(
            ObjectIdentifier::for_path(Product::MariaDb, one()).kind,
            ObjectKind::Database
        );
        assert_eq!(
            ObjectIdentifier::for_path(Product::Sqlite, one()).kind,
            ObjectKind::Table
        );
        assert_eq!(
            ObjectIdentifier::for_path(Product::MongoDb, two()).kind,
            ObjectKind::Collection
        );
        assert_eq!(
            ObjectIdentifier::for_path(Product::Redis, one()).kind,
            ObjectKind::Key
        );
    }

    #[test]
    fn a_path_is_kept_as_components_not_a_joined_string() {
        let object =
            ObjectIdentifier::new(ObjectKind::Table, vec!["public".into(), "my.table".into()]);
        assert_eq!(object.path.len(), 2);
        assert_eq!(object.name(), "my.table");
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
