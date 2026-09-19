//! PostgreSQL.
//!
//! `tokio-postgres` is async throughout, so unlike the SQLite adapter nothing
//! here needs a blocking pool. The connection task is spawned and held: dropping
//! it closes the socket, which is why the join handle is kept alive alongside
//! the client rather than discarded.

use anyhow::{Context, Result};
use tokio_postgres::types::Type;
use tokio_postgres::{Client, NoTls};

use crate::connection::{ConnectionDefinition, ReadOnlyPolicy, SecretPurpose};
use crate::identify::{ObjectIdentifier, ObjectKind};
use crate::mutation::Payload;
use crate::paging::{ColumnDescriptor, Page, PageRequest};
use crate::product::{Product, ProductIdentity, Topology, TopologyKind, Version};
use crate::secrets::SecretStore;
use crate::value::{Decimal, Value};

use super::Adapter;

pub struct PostgresAdapter {
    client: Client,
    /// Holds the connection task open. Dropping it closes the socket.
    _task: tokio::task::JoinHandle<()>,
    read_only: bool,
}

impl PostgresAdapter {
    pub async fn connect(definition: &ConnectionDefinition, secrets: &SecretStore) -> Result<Self> {
        let endpoint = definition
            .location
            .primary_endpoint()
            .context("this connection names no endpoint")?;

        let mut config = tokio_postgres::Config::new();
        config
            .host(&endpoint.host)
            .port(endpoint.port.get())
            .connect_timeout(std::time::Duration::from_millis(
                definition.limits.connection_timeout.get(),
            ));
        if let Some(username) = &definition.username {
            config.user(username);
        }
        if let Some(database) = &definition.namespaces.database {
            config.dbname(database);
        }
        // The password is fetched at connect time and never stored on the
        // adapter, so it lives as briefly as it can.
        if let Some(reference) = definition.authentication.reference(SecretPurpose::Password) {
            if let Some(password) = secrets
                .load(reference.identifier, reference.purpose)
                .await?
            {
                config.password(password);
            }
        }
        // A read-only connection tells the server so, as well as Veronica
        // refusing to send a write. Two locks are better than one.
        let read_only = definition.read_only_policy == ReadOnlyPolicy::Required;
        if read_only {
            config.options("-c default_transaction_read_only=on");
        }

        let (client, connection) = config
            .connect(NoTls)
            .await
            .with_context(|| format!("cannot reach {}:{}", endpoint.host, endpoint.port.get()))?;
        let task = tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::debug!("the PostgreSQL connection ended: {error}");
            }
        });

        Ok(Self {
            client,
            _task: task,
            read_only,
        })
    }
}

/// Quote an identifier the way PostgreSQL does.
pub fn quote(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// A schema-qualified name, each part quoted separately so a dot in a name is
/// part of the name rather than a separator.
fn qualified(object: &ObjectIdentifier) -> String {
    object
        .path
        .iter()
        .map(|part| quote(part))
        .collect::<Vec<_>>()
        .join(".")
}

/// Read one column, mapping PostgreSQL's types onto Veronica's.
///
/// Everything unknown falls through to text, which every type has a
/// representation in. Guessing at a binary layout would be worse than showing
/// what the server itself would print.
fn read_value(row: &tokio_postgres::Row, index: usize) -> Value {
    let column = &row.columns()[index];
    match *column.type_() {
        Type::BOOL => row
            .try_get::<_, Option<bool>>(index)
            .ok()
            .flatten()
            .map(Value::Boolean),
        Type::INT2 => row
            .try_get::<_, Option<i16>>(index)
            .ok()
            .flatten()
            .map(|value| Value::SignedInteger(value.into())),
        Type::INT4 => row
            .try_get::<_, Option<i32>>(index)
            .ok()
            .flatten()
            .map(|value| Value::SignedInteger(value.into())),
        Type::INT8 => row
            .try_get::<_, Option<i64>>(index)
            .ok()
            .flatten()
            .map(Value::SignedInteger),
        Type::FLOAT4 => row
            .try_get::<_, Option<f32>>(index)
            .ok()
            .flatten()
            .map(|value| Value::FloatingPoint(value.into())),
        Type::FLOAT8 => row
            .try_get::<_, Option<f64>>(index)
            .ok()
            .flatten()
            .map(Value::FloatingPoint),
        Type::UUID => row
            .try_get::<_, Option<uuid::Uuid>>(index)
            .ok()
            .flatten()
            .map(Value::Uuid),
        Type::BYTEA => row
            .try_get::<_, Option<Vec<u8>>>(index)
            .ok()
            .flatten()
            .map(|bytes| {
                Value::Binary(crate::value::Binary::Complete {
                    data: base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        &bytes,
                    ),
                    media_type: None,
                    digest: None,
                })
            }),
        Type::JSON | Type::JSONB => row
            .try_get::<_, Option<serde_json::Value>>(index)
            .ok()
            .flatten()
            .map(|json| Value::from_json(&json)),
        // NUMERIC is exact, and reading it through a float would lose that, so
        // it comes back as text and stays exact.
        _ => None,
    }
    .unwrap_or_else(|| match row.try_get::<_, Option<String>>(index) {
        Ok(Some(text)) => {
            if *column.type_() == Type::NUMERIC {
                Value::Decimal(Decimal(text))
            } else {
                Value::String(text)
            }
        }
        Ok(None) => Value::Null,
        // A type with no text representation this client can read. Saying so
        // beats showing an empty cell that looks like NULL.
        Err(_) => Value::ProductSpecific(crate::value::ProductValue {
            type_name: column.type_().name().to_string(),
            rendered: format!("<{}>", column.type_().name()),
        }),
    })
}

fn to_page(rows: Vec<tokio_postgres::Row>, request: &PageRequest, elapsed: u64) -> Page {
    let columns: Vec<ColumnDescriptor> = rows
        .first()
        .map(|row| {
            row.columns()
                .iter()
                .map(|column| ColumnDescriptor {
                    name: column.name().to_string(),
                    type_name: Some(column.type_().name().to_string()),
                })
                .collect()
        })
        .unwrap_or_default();
    let shaped: Vec<Vec<Value>> = rows
        .iter()
        .map(|row| {
            (0..row.columns().len())
                .map(|i| read_value(row, i))
                .collect()
        })
        .collect();
    Page::from_rows(columns, shaped, request, elapsed)
}

fn bounded(sql: &str, request: &PageRequest) -> String {
    format!(
        "SELECT * FROM ({sql}) AS veronica_page LIMIT {} OFFSET {}",
        request.page_size.get() as u64 + 1,
        request.offset
    )
}

#[async_trait::async_trait]
impl Adapter for PostgresAdapter {
    async fn identify(&mut self) -> Result<ProductIdentity> {
        let row = self
            .client
            .query_one(
                "SELECT version(), current_setting('server_version'), \
                 pg_is_in_recovery(), current_database()",
                &[],
            )
            .await?;
        let banner: String = row.get(0);
        let version: String = row.get(1);
        let in_recovery: bool = row.get(2);
        let database: String = row.get(3);

        Ok(ProductIdentity {
            version: Some(Version::parse(&version)),
            distribution: Some(banner),
            topology: Topology {
                // A server in recovery is a replica, and knowing that before
                // trying to write to it saves an obscure error later.
                kind: if in_recovery {
                    TopologyKind::PrimaryReplica
                } else {
                    TopologyKind::Standalone
                },
                local_role: Some(if in_recovery { "replica" } else { "primary" }.into()),
                ..Topology::default()
            },
            server_identifier: Some(database),
            ..ProductIdentity::new(Product::Postgresql)
        })
    }

    async fn objects(
        &mut self,
        parent: Option<&ObjectIdentifier>,
    ) -> Result<Vec<ObjectIdentifier>> {
        match parent {
            // The top level is the schemas a user would care about.
            None => {
                let rows = self
                    .client
                    .query(
                        "SELECT nspname FROM pg_namespace
                         WHERE nspname NOT LIKE 'pg_%' AND nspname <> 'information_schema'
                         ORDER BY nspname",
                        &[],
                    )
                    .await?;
                Ok(rows
                    .iter()
                    .map(|row| {
                        ObjectIdentifier::new(ObjectKind::Schema, vec![row.get::<_, String>(0)])
                    })
                    .collect())
            }
            Some(object) if object.kind == ObjectKind::Schema => {
                let rows = self
                    .client
                    .query(
                        "SELECT table_name, table_type FROM information_schema.tables
                         WHERE table_schema = $1 ORDER BY table_name",
                        &[&object.name()],
                    )
                    .await?;
                Ok(rows
                    .iter()
                    .map(|row| {
                        let name: String = row.get(0);
                        let kind: String = row.get(1);
                        ObjectIdentifier::new(
                            if kind == "VIEW" {
                                ObjectKind::View
                            } else {
                                ObjectKind::Table
                            },
                            vec![object.name().to_string(), name],
                        )
                    })
                    .collect())
            }
            Some(object) => {
                let schema = object.path.first().cloned().unwrap_or_default();
                let rows = self
                    .client
                    .query(
                        "SELECT column_name, data_type FROM information_schema.columns
                         WHERE table_schema = $1 AND table_name = $2 ORDER BY ordinal_position",
                        &[&schema, &object.name()],
                    )
                    .await?;
                Ok(rows
                    .iter()
                    .map(|row| ObjectIdentifier {
                        kind: ObjectKind::Column,
                        path: vec![schema.clone(), object.name().to_string(), row.get(0)],
                        native_identifier: Some(row.get(1)),
                    })
                    .collect())
            }
        }
    }

    async fn read(&mut self, object: &ObjectIdentifier, request: &PageRequest) -> Result<Page> {
        let started = std::time::Instant::now();
        let sql = bounded(&format!("SELECT * FROM {}", qualified(object)), request);
        let rows = self.client.query(&sql, &[]).await?;
        Ok(to_page(rows, request, started.elapsed().as_millis() as u64))
    }

    async fn query(&mut self, statement: &str, request: &PageRequest) -> Result<Page> {
        if !super::is_read_only_sql(statement) {
            anyhow::bail!(
                "`query` only runs statements that read. A change goes through \
                 `vr database mutations`, which previews it first."
            );
        }
        let started = std::time::Instant::now();
        let sql = bounded(statement.trim().trim_end_matches(';'), request);
        let rows = self.client.query(&sql, &[]).await?;
        Ok(to_page(rows, request, started.elapsed().as_millis() as u64))
    }

    async fn count(&mut self, object: &ObjectIdentifier) -> Result<Option<u64>> {
        let row = self
            .client
            .query_one(&format!("SELECT COUNT(*) FROM {}", qualified(object)), &[])
            .await?;
        Ok(Some(row.get::<_, i64>(0) as u64))
    }

    async fn execute(&mut self, payload: &Payload) -> Result<u64> {
        let Payload::Relational {
            statement,
            parameters,
            ..
        } = payload
        else {
            anyhow::bail!(
                "PostgreSQL takes a relational payload, not a {:?}",
                payload.kind()
            );
        };
        if self.read_only {
            anyhow::bail!("this connection is read-only");
        }
        // Values are bound as text and cast by the server, which is the one
        // mapping that works for every column type without this client having
        // to know the schema. The statement is still exactly what was signed.
        let bound: Vec<Option<String>> = parameters
            .iter()
            .map(|parameter| bind_text(&parameter.value))
            .collect();
        let references: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = bound
            .iter()
            .map(|value| value as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();
        Ok(self.client.execute(statement, &references).await?)
    }
}

/// A value as the text PostgreSQL will cast.
fn bind_text(value: &Value) -> Option<String> {
    match value {
        Value::Null | Value::Missing => None,
        Value::Boolean(value) => Some(value.to_string()),
        Value::SignedInteger(value) => Some(value.to_string()),
        Value::UnsignedInteger(value) => Some(value.to_string()),
        Value::FloatingPoint(value) => Some(value.to_string()),
        Value::Decimal(Decimal(text)) => Some(text.clone()),
        Value::String(value) => Some(value.clone()),
        Value::Uuid(value) => Some(value.to_string()),
        Value::Date(value) | Value::Time(value) | Value::Timestamp(value) => Some(value.clone()),
        Value::Binary(binary) => Some(match binary {
            crate::value::Binary::Complete { data, .. }
            | crate::value::Binary::Preview { bytes: data, .. } => {
                // PostgreSQL's hex bytea form, which it casts without help.
                match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data) {
                    Ok(bytes) => format!(
                        "\\x{}",
                        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
                    ),
                    Err(_) => String::new(),
                }
            }
        }),
        Value::Array(_) | Value::Object(_) => {
            Some(serde_json::to_string(value).unwrap_or_else(|_| "null".into()))
        }
        Value::ProductSpecific(product) => Some(product.rendered.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identifier_is_quoted_and_a_quote_inside_it_doubled() {
        assert_eq!(quote("orders"), "\"orders\"");
        assert_eq!(quote(r#"a"b"#), r#""a""b""#);
    }

    #[test]
    fn each_part_of_a_path_is_quoted_separately() {
        // So a schema called `a.b` is one schema, not two path components.
        let object =
            ObjectIdentifier::new(ObjectKind::Table, vec!["my.schema".into(), "orders".into()]);
        assert_eq!(qualified(&object), r#""my.schema"."orders""#);
    }

    #[test]
    fn a_page_asks_for_one_row_more_than_it_shows() {
        let request = PageRequest {
            page_size: crate::paging::PageSize::new(50).unwrap(),
            offset: 100,
            ..PageRequest::default()
        };
        let sql = bounded("SELECT * FROM t", &request);
        assert!(sql.contains("LIMIT 51"), "got {sql}");
        assert!(sql.contains("OFFSET 100"), "got {sql}");
    }

    #[test]
    fn a_decimal_binds_as_its_exact_text() {
        assert_eq!(
            bind_text(&Value::Decimal(Decimal("19.99".into()))),
            Some("19.99".to_string())
        );
    }

    #[test]
    fn null_and_missing_bind_as_null_rather_than_an_empty_string() {
        // Writing '' into a nullable column is not the same as writing NULL.
        assert_eq!(bind_text(&Value::Null), None);
        assert_eq!(bind_text(&Value::Missing), None);
        assert_eq!(
            bind_text(&Value::String(String::new())),
            Some(String::new())
        );
    }

    #[test]
    fn a_blob_binds_in_the_hex_form_postgresql_casts() {
        let binary = Value::Binary(crate::value::Binary::Complete {
            data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0xde, 0xad]),
            media_type: None,
            digest: None,
        });
        assert_eq!(bind_text(&binary), Some("\\xdead".to_string()));
    }

    #[test]
    fn a_structure_binds_as_json() {
        let value = Value::Array(vec![Value::SignedInteger(1)]);
        assert!(bind_text(&value).unwrap().contains("signedInteger"));
    }
}
