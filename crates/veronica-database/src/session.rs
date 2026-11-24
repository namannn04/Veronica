use anyhow::{Context, Result};
use uuid::Uuid;

use crate::adapters::{mysql, postgres, redis, sqlite, Adapter};
use crate::capabilities::Report;
use crate::connection::{ConnectionDefinition, SecretPurpose};
use crate::guard::{Guard, Preview};
use crate::identify::ObjectIdentifier;
use crate::mutation::Plan;
use crate::paging::{Page, PageRequest};
use crate::product::{Product, ProductIdentity};
use crate::secrets::SecretStore;
use crate::store::{MetadataStore, OperationOutcome, OperationRecord};

pub const SIGNING_KEY_ID: Uuid = Uuid::from_u128(0x7e_20_1c_a0_00_00_40_00_80_00_00_00_00_00_00_01);

pub struct Session {
    adapter: Box<dyn Adapter>,
    connection: ConnectionDefinition,
    identity: Option<ProductIdentity>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("connection", &self.connection.display_name)
            .field("product", &self.connection.product_hint)
            .finish_non_exhaustive()
    }
}

impl Session {
    pub async fn open(connection: &ConnectionDefinition, secrets: &SecretStore) -> Result<Self> {
        let adapter: Box<dyn Adapter> = match connection.product_hint {
            Product::Sqlite => Box::new(sqlite::SqliteAdapter::open(connection)?),
            Product::Postgresql => {
                Box::new(postgres::PostgresAdapter::connect(connection, secrets).await?)
            }
            Product::Mysql | Product::MariaDb => {
                Box::new(mysql::MysqlAdapter::connect(connection, secrets).await?)
            }
            Product::Redis | Product::Valkey => {
                Box::new(redis::RedisAdapter::connect(connection, secrets).await?)
            }
            product => anyhow::bail!(
                "Veronica has no {} adapter yet. It reads PostgreSQL, MySQL, MariaDB, \
                 SQLite, Redis and Valkey.",
                product.title()
            ),
        };
        Ok(Self {
            adapter,
            connection: connection.clone(),
            identity: None,
        })
    }

    pub fn connection(&self) -> &ConnectionDefinition {
        &self.connection
    }

    pub async fn identify(&mut self) -> Result<ProductIdentity> {
        if let Some(identity) = &self.identity {
            return Ok(identity.clone());
        }
        let identity = self.adapter.identify().await?;
        self.identity = Some(identity.clone());
        Ok(identity)
    }

    pub async fn capabilities(&mut self) -> Result<Report> {
        Ok(Report::for_identity(&self.identify().await?))
    }

    pub async fn objects(
        &mut self,
        parent: Option<&ObjectIdentifier>,
    ) -> Result<Vec<ObjectIdentifier>> {
        self.adapter.objects(parent).await
    }

    pub async fn read(&mut self, object: &ObjectIdentifier, request: &PageRequest) -> Result<Page> {
        self.adapter.read(object, request).await
    }

    pub async fn query(&mut self, statement: &str, request: &PageRequest) -> Result<Page> {
        self.adapter.query(statement, request).await
    }

    pub async fn count(&mut self, object: &ObjectIdentifier) -> Result<Option<u64>> {
        self.adapter.count(object).await
    }

    pub async fn preview(
        &mut self,
        plan: &Plan,
        store: &mut MetadataStore,
        secrets: &SecretStore,
        lifetime_seconds: i64,
    ) -> Result<Preview> {
        let mut measured = plan.clone();
        if measured.impact.estimated_records.is_none() {
            if let Some(object) = &plan.target.object {
                if let Ok(Some(count)) = self.adapter.count(object).await {
                    measured.impact = crate::mutation::Impact {
                        estimated_records: Some(count),
                        is_exact: measured.scope == crate::mutation::Scope::EntireObject,
                    };
                }
            }
        }

        let key = secrets
            .signing_key(SIGNING_KEY_ID, SecretPurpose::ConfirmationSigningKey)
            .await?;
        let mut guard = Guard::new(key, store).context("cannot open the confirmation guard")?;
        Ok(guard.preview(
            &measured,
            &self.connection,
            chrono::Utc::now(),
            lifetime_seconds,
        )?)
    }

    pub async fn apply(
        &mut self,
        plan: &Plan,
        token: &str,
        confirmation_text: &str,
        store: &mut MetadataStore,
        secrets: &SecretStore,
    ) -> Result<Applied> {
        let key = secrets
            .signing_key(SIGNING_KEY_ID, SecretPurpose::ConfirmationSigningKey)
            .await?;
        let mut measured = plan.clone();
        if measured.impact.estimated_records.is_none() {
            if let Some(object) = &plan.target.object {
                if let Ok(Some(count)) = self.adapter.count(object).await {
                    measured.impact = crate::mutation::Impact {
                        estimated_records: Some(count),
                        is_exact: measured.scope == crate::mutation::Scope::EntireObject,
                    };
                }
            }
        }

        let effect = {
            let mut guard = Guard::new(key, &mut *store)?;
            guard.authorize(
                token,
                &measured,
                &self.connection,
                confirmation_text,
                chrono::Utc::now(),
            )?
        };

        let started = std::time::Instant::now();
        let outcome = self.adapter.execute(&measured.payload).await;
        let elapsed = crate::elapsed_millis(started);

        let record = OperationRecord {
            id: Uuid::new_v4(),
            connection_id: self.connection.id,
            connection_name: self.connection.display_name.clone(),
            kind: "mutation".to_string(),
            summary: format!(
                "{} · {}",
                measured.action.key(),
                measured.payload.preview().command
            ),
            outcome: if outcome.is_ok() {
                OperationOutcome::Succeeded
            } else {
                OperationOutcome::Failed
            },
            affected_records: outcome.as_ref().ok().copied(),
            elapsed_millis: elapsed,
            error: outcome.as_ref().err().map(|error| format!("{error:#}")),
            at: chrono::Utc::now(),
        };
        let _ = store.record_operation(&record);

        let affected = outcome?;
        Ok(Applied {
            effect: Box::new(effect),
            affected_records: affected,
            elapsed_millis: elapsed,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub effect: Box<crate::mutation::Effect>,
    pub affected_records: u64,
    pub elapsed_millis: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::*;
    use crate::identify::{ObjectKind, TargetIdentifier};
    use crate::mutation::*;
    use crate::value::Value;

    fn sqlite_connection(path: &str) -> ConnectionDefinition {
        let now = chrono::Utc::now();
        ConnectionDefinition {
            version: SCHEMA_VERSION,
            id: Uuid::new_v4(),
            display_name: "Test".into(),
            product_hint: Product::Sqlite,
            location: Location::Sqlite {
                sqlite: SqliteLocation {
                    path: path.into(),
                    access_mode: SqliteAccessMode::ReadWrite,
                },
            },
            username: None,
            namespaces: NamespaceDefaults::default(),
            deployment_mode: DeploymentMode::Embedded,
            authentication: Authentication::default(),
            tls: TlsConfiguration::default(),
            tunnel: None,
            limits: ConnectionLimits::default(),
            read_only_policy: ReadOnlyPolicy::Disabled,
            production_policy: ProductionPolicy::Standard,
            environment: EnvironmentMetadata::default(),
            group: None,
            tags: vec![],
            color: None,
            is_favorite: false,
            created_at: now,
            updated_at: now,
            last_tested_at: None,
            last_used_at: None,
        }
    }

    fn seed(path: &std::path::Path) {
        rusqlite::Connection::open(path)
            .unwrap()
            .execute_batch(
                "CREATE TABLE orders (id INTEGER PRIMARY KEY, note TEXT);
                 INSERT INTO orders VALUES (1, 'first'), (2, 'second'), (3, 'third');",
            )
            .unwrap();
    }

    fn delete_plan(connection: &ConnectionDefinition) -> Plan {
        Plan {
            payload: Payload::Relational {
                product: Product::Sqlite,
                statement: "DELETE FROM \"orders\" WHERE id = ?1".into(),
                parameters: vec![Parameter {
                    name: "id".into(),
                    value: Value::SignedInteger(2),
                }],
            },
            action: Action::Delete,
            scope: Scope::SingleRecord,
            impact: Impact::exact(1),
            transaction_behavior: TransactionBehavior::Transactional,
            rollback_availability: RollbackAvailability::Available,
            execution_mode: ExecutionMode::Synchronous,
            target: TargetIdentifier::object(
                connection.id,
                ObjectIdentifier::new(ObjectKind::Table, vec!["orders".into()]),
            ),
            context: MutationContext {
                kind: ContextKind::Database,
                value: "main".into(),
                catalog: None,
                schema: None,
            },
            selected_records: vec![],
            predicate: None,
        }
    }

    #[tokio::test]
    async fn a_change_is_previewed_confirmed_and_applied() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.db");
        seed(&path);

        let connection = sqlite_connection(path.to_str().unwrap());
        let secrets = SecretStore::file_only(directory.path().join("secrets.json"));
        let mut store = MetadataStore::in_memory().unwrap();
        let mut session = Session::open(&connection, &secrets).await.unwrap();
        let plan = delete_plan(&connection);

        let preview = session
            .preview(&plan, &mut store, &secrets, 120)
            .await
            .unwrap();
        assert_eq!(
            session
                .count(&ObjectIdentifier::new(
                    ObjectKind::Table,
                    vec!["orders".into()]
                ))
                .await
                .unwrap(),
            Some(3)
        );

        let applied = session
            .apply(
                &plan,
                &preview.token,
                &preview.required_confirmation.text,
                &mut store,
                &secrets,
            )
            .await
            .unwrap();
        assert_eq!(applied.affected_records, 1);
        assert_eq!(
            session
                .count(&ObjectIdentifier::new(
                    ObjectKind::Table,
                    vec!["orders".into()]
                ))
                .await
                .unwrap(),
            Some(2)
        );

        let history = store.operations(10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].outcome, OperationOutcome::Succeeded);
        assert_eq!(history[0].affected_records, Some(1));
        assert!(history[0].summary.contains("delete"));
    }

    #[tokio::test]
    async fn a_change_cannot_be_applied_without_a_preview() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.db");
        seed(&path);

        let connection = sqlite_connection(path.to_str().unwrap());
        let secrets = SecretStore::file_only(directory.path().join("secrets.json"));
        let mut store = MetadataStore::in_memory().unwrap();
        let mut session = Session::open(&connection, &secrets).await.unwrap();

        let error = session
            .apply(
                &delete_plan(&connection),
                "not.atoken",
                "confirm",
                &mut store,
                &secrets,
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("not a token"), "got {error}");
        assert_eq!(
            session
                .count(&ObjectIdentifier::new(
                    ObjectKind::Table,
                    vec!["orders".into()]
                ))
                .await
                .unwrap(),
            Some(3)
        );
    }

    #[tokio::test]
    async fn a_token_taken_for_one_row_will_not_delete_another() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.db");
        seed(&path);

        let connection = sqlite_connection(path.to_str().unwrap());
        let secrets = SecretStore::file_only(directory.path().join("secrets.json"));
        let mut store = MetadataStore::in_memory().unwrap();
        let mut session = Session::open(&connection, &secrets).await.unwrap();

        let preview = session
            .preview(&delete_plan(&connection), &mut store, &secrets, 120)
            .await
            .unwrap();

        let mut other = delete_plan(&connection);
        other.payload = Payload::Relational {
            product: Product::Sqlite,
            statement: "DELETE FROM \"orders\" WHERE id = ?1".into(),
            parameters: vec![Parameter {
                name: "id".into(),
                value: Value::SignedInteger(3),
            }],
        };
        assert!(session
            .apply(
                &other,
                &preview.token,
                &preview.required_confirmation.text,
                &mut store,
                &secrets
            )
            .await
            .is_err());
        assert_eq!(
            session
                .count(&ObjectIdentifier::new(
                    ObjectKind::Table,
                    vec!["orders".into()]
                ))
                .await
                .unwrap(),
            Some(3),
            "nothing was deleted"
        );
    }

    #[tokio::test]
    async fn a_preview_measures_the_impact_rather_than_trusting_the_caller() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.db");
        seed(&path);

        let connection = sqlite_connection(path.to_str().unwrap());
        let secrets = SecretStore::file_only(directory.path().join("secrets.json"));
        let mut store = MetadataStore::in_memory().unwrap();
        let mut session = Session::open(&connection, &secrets).await.unwrap();

        let mut plan = delete_plan(&connection);
        plan.scope = Scope::EntireObject;
        plan.action = Action::Truncate;
        plan.impact = Impact::unknown();

        let preview = session
            .preview(&plan, &mut store, &secrets, 120)
            .await
            .unwrap();
        assert_eq!(preview.effect.impact.estimated_records, Some(3));
        assert!(preview.effect.impact.is_exact);
        assert_eq!(
            preview.required_confirmation.strength,
            ConfirmationStrength::ConnectionAndTarget
        );
    }

    #[tokio::test]
    async fn a_failed_mutation_is_recorded_too() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.db");
        seed(&path);

        let connection = sqlite_connection(path.to_str().unwrap());
        let secrets = SecretStore::file_only(directory.path().join("secrets.json"));
        let mut store = MetadataStore::in_memory().unwrap();
        let mut session = Session::open(&connection, &secrets).await.unwrap();

        let mut plan = delete_plan(&connection);
        plan.payload = Payload::Relational {
            product: Product::Sqlite,
            statement: "DELETE FROM \"no_such_table\" WHERE id = ?1".into(),
            parameters: vec![Parameter {
                name: "id".into(),
                value: Value::SignedInteger(2),
            }],
        };

        let preview = session
            .preview(&plan, &mut store, &secrets, 120)
            .await
            .unwrap();
        assert!(session
            .apply(
                &plan,
                &preview.token,
                &preview.required_confirmation.text,
                &mut store,
                &secrets
            )
            .await
            .is_err());

        let history = store.operations(10).unwrap();
        assert_eq!(history[0].outcome, OperationOutcome::Failed);
        assert!(history[0].error.is_some());
    }

    #[tokio::test]
    async fn a_product_with_no_adapter_says_so_by_name() {
        let directory = tempfile::tempdir().unwrap();
        let mut connection = sqlite_connection("/tmp/unused.db");
        connection.product_hint = Product::Elasticsearch;
        let secrets = SecretStore::file_only(directory.path().join("secrets.json"));
        let error = Session::open(&connection, &secrets)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("Elasticsearch"), "got {error}");
        assert!(error.contains("PostgreSQL"), "and says what it does read");
    }

    #[tokio::test]
    async fn capabilities_come_from_what_the_server_turned_out_to_be() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.db");
        seed(&path);
        let connection = sqlite_connection(path.to_str().unwrap());
        let secrets = SecretStore::file_only(directory.path().join("secrets.json"));
        let mut session = Session::open(&connection, &secrets).await.unwrap();

        let report = session.capabilities().await.unwrap();
        assert_eq!(report.product, Product::Sqlite);
        assert!(report.supports(crate::capabilities::Capability::Transactions));
        assert!(!report.supports(crate::capabilities::Capability::ListSessions));
    }
}
