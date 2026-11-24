use serde::{Deserialize, Serialize};

use crate::connection::ConnectionIdentity;
use crate::identify::{RecordIdentity, TargetIdentifier};
use crate::paging::Filter;
use crate::product::Product;
use crate::value::{Value, ValueKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Insert,
    Update,
    UpdateMany,
    Delete,
    DeleteMany,
    Truncate,
    DropObject,
    SchemaChange,
    PermissionChange,
    TerminateSession,
    Maintenance,
    Reindex,
    AsynchronousMutation,
}

impl Action {
    pub fn is_strongest(self) -> bool {
        matches!(
            self,
            Action::Truncate
                | Action::DropObject
                | Action::SchemaChange
                | Action::PermissionChange
                | Action::TerminateSession
                | Action::Maintenance
                | Action::Reindex
                | Action::AsynchronousMutation
        )
    }

    pub fn key(self) -> &'static str {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::String(name)) => Box::leak(name.into_boxed_str()),
            _ => "update",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    SingleRecord,
    SelectedRecords,
    Predicate,
    EntireObject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransactionBehavior {
    Transactional,
    Nontransactional,
    Asynchronous,
    ProductDependent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RollbackAvailability {
    Available,
    Unavailable,
    Conditional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionMode {
    Synchronous,
    Asynchronous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Impact {
    pub estimated_records: Option<u64>,
    pub is_exact: bool,
}

impl Impact {
    pub fn unknown() -> Self {
        Self {
            estimated_records: None,
            is_exact: false,
        }
    }

    pub fn exact(records: u64) -> Self {
        Self {
            estimated_records: Some(records),
            is_exact: true,
        }
    }

    pub fn describe(&self) -> String {
        match (self.estimated_records, self.is_exact) {
            (Some(1), true) => "1 record".to_string(),
            (Some(count), true) => format!("{count} records"),
            (Some(count), false) => format!("about {count} records"),
            (None, _) => "an unknown number of records".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PayloadKind {
    Sql,
    Keyspace,
    Document,
    Search,
    Analytical,
    Administrative,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Parameter {
    pub name: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterPreview {
    pub name: String,
    pub value_kind: ValueKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Payload {
    Relational {
        product: Product,
        statement: String,
        parameters: Vec<Parameter>,
    },
    Keyspace {
        product: Product,
        command: String,
        arguments: Vec<Parameter>,
    },
    Document {
        product: Product,
        operation: String,
        parameters: Vec<Parameter>,
        body: Value,
    },
    Search {
        product: Product,
        operation: String,
        parameters: Vec<Parameter>,
        body: Value,
    },
    Analytical {
        product: Product,
        statement: String,
        parameters: Vec<Parameter>,
    },
    Administrative {
        product: Product,
        command: String,
        parameters: Vec<Parameter>,
        body: Option<Value>,
    },
}

impl Payload {
    pub fn product(&self) -> Product {
        match self {
            Payload::Relational { product, .. }
            | Payload::Keyspace { product, .. }
            | Payload::Document { product, .. }
            | Payload::Search { product, .. }
            | Payload::Analytical { product, .. }
            | Payload::Administrative { product, .. } => *product,
        }
    }

    pub fn kind(&self) -> PayloadKind {
        match self {
            Payload::Relational { .. } => PayloadKind::Sql,
            Payload::Keyspace { .. } => PayloadKind::Keyspace,
            Payload::Document { .. } => PayloadKind::Document,
            Payload::Search { .. } => PayloadKind::Search,
            Payload::Analytical { .. } => PayloadKind::Analytical,
            Payload::Administrative { .. } => PayloadKind::Administrative,
        }
    }

    pub fn command(&self) -> &str {
        match self {
            Payload::Relational { statement, .. } | Payload::Analytical { statement, .. } => {
                statement
            }
            Payload::Keyspace { command, .. } | Payload::Administrative { command, .. } => command,
            Payload::Document { operation, .. } | Payload::Search { operation, .. } => operation,
        }
    }

    fn parameters(&self) -> &[Parameter] {
        match self {
            Payload::Relational { parameters, .. }
            | Payload::Document { parameters, .. }
            | Payload::Search { parameters, .. }
            | Payload::Analytical { parameters, .. }
            | Payload::Administrative { parameters, .. } => parameters,
            Payload::Keyspace { arguments, .. } => arguments,
        }
    }

    fn body(&self) -> Option<&Value> {
        match self {
            Payload::Document { body, .. } | Payload::Search { body, .. } => Some(body),
            Payload::Administrative { body, .. } => body.as_ref(),
            _ => None,
        }
    }

    pub fn preview(&self) -> MutationPreview {
        MutationPreview {
            product: self.product(),
            kind: self.kind(),
            command: self.command().to_string(),
            parameters: self
                .parameters()
                .iter()
                .map(|parameter| ParameterPreview {
                    name: parameter.name.clone(),
                    value_kind: parameter.value.kind(),
                })
                .collect(),
            body: self.body().map(redact),
        }
    }

    pub fn is_family_consistent(&self) -> bool {
        use crate::product::Family;
        matches!(
            (self.kind(), self.product().family()),
            (PayloadKind::Sql, Family::Relational)
                | (PayloadKind::Keyspace, Family::KeyValue)
                | (PayloadKind::Document, Family::Document)
                | (PayloadKind::Search, Family::Search)
                | (PayloadKind::Analytical, Family::Analytical)
                | (PayloadKind::Administrative, _)
        )
    }
}

fn redact(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(redact).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|field| crate::value::ObjectField {
                    name: field.name.clone(),
                    value: redact(&field.value),
                })
                .collect(),
        ),
        other => Value::ProductSpecific(crate::value::ProductValue {
            type_name: format!("{:?}", other.kind()),
            rendered: format!("<{:?}>", other.kind()),
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationPreview {
    pub product: Product,
    pub kind: PayloadKind,
    pub command: String,
    pub parameters: Vec<ParameterPreview>,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContextKind {
    Database,
    Cluster,
    LogicalDatabase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationContext {
    pub kind: ContextKind,
    pub value: String,
    pub catalog: Option<String>,
    pub schema: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WarningKind {
    UnboundedScope,
    Irreversible,
    ProductionEnvironment,
    NotTransactional,
    Asynchronous,
    UnknownImpact,
    NoConcurrencyToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    pub kind: WarningKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub payload: Payload,
    pub action: Action,
    pub scope: Scope,
    pub impact: Impact,
    pub transaction_behavior: TransactionBehavior,
    pub rollback_availability: RollbackAvailability,
    pub execution_mode: ExecutionMode,
    pub target: TargetIdentifier,
    pub context: MutationContext,
    #[serde(default)]
    pub selected_records: Vec<RecordIdentity>,
    pub predicate: Option<Filter>,
}

impl Plan {
    pub fn warnings(&self, environment_is_production: bool) -> Vec<Warning> {
        let mut warnings = Vec::new();
        if self.scope == Scope::EntireObject {
            warnings.push(Warning {
                kind: WarningKind::UnboundedScope,
                detail: "This reaches every record in the object.".into(),
            });
        }
        if self.rollback_availability != RollbackAvailability::Available {
            warnings.push(Warning {
                kind: WarningKind::Irreversible,
                detail: match self.rollback_availability {
                    RollbackAvailability::Unavailable => {
                        "This cannot be undone once it has run.".into()
                    }
                    _ => "This may not be possible to undo.".to_string(),
                },
            });
        }
        if environment_is_production {
            warnings.push(Warning {
                kind: WarningKind::ProductionEnvironment,
                detail: "This connection is marked production.".into(),
            });
        }
        if self.transaction_behavior != TransactionBehavior::Transactional {
            warnings.push(Warning {
                kind: WarningKind::NotTransactional,
                detail: "This runs outside a transaction, so a failure part-way leaves \
                         the change half applied."
                    .into(),
            });
        }
        if self.execution_mode == ExecutionMode::Asynchronous {
            warnings.push(Warning {
                kind: WarningKind::Asynchronous,
                detail: "This returns before the server has finished.".into(),
            });
        }
        if self.impact.estimated_records.is_none() {
            warnings.push(Warning {
                kind: WarningKind::UnknownImpact,
                detail: "How many records this affects is not known in advance.".into(),
            });
        }
        if self.scope == Scope::SingleRecord
            && self
                .selected_records
                .iter()
                .any(|record| record.concurrency_tokens.is_empty())
        {
            warnings.push(Warning {
                kind: WarningKind::NoConcurrencyToken,
                detail: "The record carries no version, so a change made by someone else \
                         since you selected it will be overwritten."
                    .into(),
            });
        }
        warnings
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfirmationStrength {
    Explicit,
    Target,
    ConnectionAndTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequiredConfirmation {
    pub strength: ConfirmationStrength,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Effect {
    pub action: Action,
    pub connection: ConnectionIdentity,
    pub context: MutationContext,
    pub target: TargetIdentifier,
    pub selected_records: Vec<RecordIdentity>,
    pub predicate: Option<Filter>,
    pub scope: Scope,
    pub impact: Impact,
    pub transaction_behavior: TransactionBehavior,
    pub rollback_availability: RollbackAvailability,
    pub execution_mode: ExecutionMode,
    pub execution_digest: String,
    pub display_digest: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identify::{IdentityComponent, ObjectIdentifier, ObjectKind, RecordIdentityKind};
    use crate::value::ObjectField;
    use uuid::Uuid;

    fn payload() -> Payload {
        Payload::Relational {
            product: Product::Postgresql,
            statement: "DELETE FROM \"public\".\"orders\" WHERE id = $1".into(),
            parameters: vec![Parameter {
                name: "id".into(),
                value: Value::String("hunter2-is-the-secret".into()),
            }],
        }
    }

    fn plan(action: Action, scope: Scope) -> Plan {
        Plan {
            payload: payload(),
            action,
            scope,
            impact: Impact::exact(1),
            transaction_behavior: TransactionBehavior::Transactional,
            rollback_availability: RollbackAvailability::Available,
            execution_mode: ExecutionMode::Synchronous,
            target: TargetIdentifier::object(
                Uuid::nil(),
                ObjectIdentifier::new(ObjectKind::Table, vec!["public".into(), "orders".into()]),
            ),
            context: MutationContext {
                kind: ContextKind::Database,
                value: "app".into(),
                catalog: None,
                schema: Some("public".into()),
            },
            selected_records: vec![RecordIdentity {
                kind: RecordIdentityKind::PrimaryKey,
                components: vec![IdentityComponent {
                    name: "id".into(),
                    value: Value::SignedInteger(7),
                }],
                concurrency_tokens: vec![IdentityComponent {
                    name: "version".into(),
                    value: Value::SignedInteger(1),
                }],
            }],
            predicate: None,
        }
    }

    #[test]
    fn a_preview_never_carries_a_parameter_value() {
        let preview = payload().preview();
        let json = serde_json::to_string(&preview).unwrap();
        assert!(
            !json.contains("hunter2"),
            "the preview leaked a value: {json}"
        );
        assert_eq!(preview.parameters.len(), 1);
        assert_eq!(preview.parameters[0].name, "id");
        assert_eq!(preview.parameters[0].value_kind, ValueKind::String);
    }

    #[test]
    fn the_statement_is_shown_in_full_because_it_holds_no_values() {
        let preview = payload().preview();
        assert!(preview.command.contains("DELETE FROM"));
        assert!(preview.command.contains("$1"), "values are parameters");
    }

    #[test]
    fn a_document_body_is_reviewed_by_its_shape_not_its_contents() {
        let payload = Payload::Document {
            product: Product::MongoDb,
            operation: "updateOne".into(),
            parameters: vec![],
            body: Value::Object(vec![
                ObjectField {
                    name: "apiKey".into(),
                    value: Value::String("sk-live-do-not-print-me".into()),
                },
                ObjectField {
                    name: "tags".into(),
                    value: Value::Array(vec![Value::String("secret-tag".into())]),
                },
            ]),
        };
        let preview = payload.preview();
        let json = serde_json::to_string(&preview).unwrap();
        assert!(!json.contains("sk-live"), "leaked: {json}");
        assert!(!json.contains("secret-tag"), "leaked: {json}");
        assert!(json.contains("apiKey"));
        assert!(json.contains("tags"));
    }

    #[test]
    fn a_payload_aimed_at_the_wrong_family_is_refused() {
        let wrong = Payload::Keyspace {
            product: Product::Postgresql,
            command: "DEL".into(),
            arguments: vec![],
        };
        assert!(!wrong.is_family_consistent());

        let right = Payload::Keyspace {
            product: Product::Redis,
            command: "DEL".into(),
            arguments: vec![],
        };
        assert!(right.is_family_consistent());
        assert!(payload().is_family_consistent());
    }

    #[test]
    fn an_administrative_payload_is_valid_for_every_family() {
        for product in Product::ALL {
            let payload = Payload::Administrative {
                product,
                command: "ping".into(),
                parameters: vec![],
                body: None,
            };
            assert!(payload.is_family_consistent(), "{product:?}");
        }
    }

    #[test]
    fn the_strongest_actions_are_ediths_eight() {
        let strongest: Vec<Action> = [
            Action::Insert,
            Action::Update,
            Action::UpdateMany,
            Action::Delete,
            Action::DeleteMany,
            Action::Truncate,
            Action::DropObject,
            Action::SchemaChange,
            Action::PermissionChange,
            Action::TerminateSession,
            Action::Maintenance,
            Action::Reindex,
            Action::AsynchronousMutation,
        ]
        .into_iter()
        .filter(|action| action.is_strongest())
        .collect();
        assert_eq!(strongest.len(), 8, "got {strongest:?}");
        assert!(!Action::Delete.is_strongest(), "one row is not the loudest");
        assert!(Action::Truncate.is_strongest());
    }

    #[test]
    fn a_routine_plan_earns_no_warnings() {
        let warnings = plan(Action::Delete, Scope::SingleRecord).warnings(false);
        assert!(warnings.is_empty(), "got {warnings:#?}");
    }

    #[test]
    fn warnings_are_derived_so_no_caller_can_omit_one() {
        let mut plan = plan(Action::Truncate, Scope::EntireObject);
        plan.rollback_availability = RollbackAvailability::Unavailable;
        plan.transaction_behavior = TransactionBehavior::Nontransactional;
        plan.execution_mode = ExecutionMode::Asynchronous;
        plan.impact = Impact::unknown();

        let kinds: Vec<WarningKind> = plan
            .warnings(true)
            .into_iter()
            .map(|warning| warning.kind)
            .collect();
        for expected in [
            WarningKind::UnboundedScope,
            WarningKind::Irreversible,
            WarningKind::ProductionEnvironment,
            WarningKind::NotTransactional,
            WarningKind::Asynchronous,
            WarningKind::UnknownImpact,
        ] {
            assert!(
                kinds.contains(&expected),
                "missing {expected:?} in {kinds:?}"
            );
        }
    }

    #[test]
    fn a_record_with_no_version_warns_that_an_edit_can_be_lost() {
        let mut plan = plan(Action::Update, Scope::SingleRecord);
        plan.selected_records[0].concurrency_tokens.clear();
        let kinds: Vec<WarningKind> = plan
            .warnings(false)
            .into_iter()
            .map(|warning| warning.kind)
            .collect();
        assert_eq!(kinds, [WarningKind::NoConcurrencyToken]);
    }

    #[test]
    fn an_impact_says_what_it_knows_and_admits_what_it_does_not() {
        assert_eq!(Impact::exact(1).describe(), "1 record");
        assert_eq!(Impact::exact(42).describe(), "42 records");
        assert_eq!(Impact::unknown().describe(), "an unknown number of records");
        assert_eq!(
            Impact {
                estimated_records: Some(1_000),
                is_exact: false
            }
            .describe(),
            "about 1000 records"
        );
    }
}
