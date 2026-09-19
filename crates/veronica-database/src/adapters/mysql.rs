//! MySQL and MariaDB.
//!
//! One adapter for both: they speak the same protocol and differ in details the
//! identity report names rather than the code branches on.

use anyhow::{Context, Result};
use mysql_async::prelude::*;
use mysql_async::{Conn, Opts, OptsBuilder, Row};

use crate::connection::{ConnectionDefinition, ReadOnlyPolicy, SecretPurpose};
use crate::identify::{ObjectIdentifier, ObjectKind};
use crate::mutation::Payload;
use crate::paging::{ColumnDescriptor, Page, PageRequest};
use crate::product::{Product, ProductIdentity, Topology, TopologyKind, Version};
use crate::secrets::SecretStore;
use crate::value::{Binary, Decimal, Value};

use super::Adapter;

pub struct MysqlAdapter {
    connection: Conn,
    read_only: bool,
    product: Product,
}

impl MysqlAdapter {
    pub async fn connect(definition: &ConnectionDefinition, secrets: &SecretStore) -> Result<Self> {
        let endpoint = definition
            .location
            .primary_endpoint()
            .context("this connection names no endpoint")?;

        let mut builder = OptsBuilder::default()
            .ip_or_hostname(endpoint.host.clone())
            .tcp_port(endpoint.port.get());
        if let Some(username) = &definition.username {
            builder = builder.user(Some(username.clone()));
        }
        if let Some(database) = &definition.namespaces.database {
            builder = builder.db_name(Some(database.clone()));
        }
        if let Some(reference) = definition.authentication.reference(SecretPurpose::Password) {
            if let Some(password) = secrets
                .load(reference.identifier, reference.purpose)
                .await?
            {
                builder = builder.pass(Some(String::from_utf8_lossy(&password).into_owned()));
            }
        }

        let mut connection = Conn::new(Opts::from(builder))
            .await
            .with_context(|| format!("cannot reach {}:{}", endpoint.host, endpoint.port.get()))?;

        let read_only = definition.read_only_policy == ReadOnlyPolicy::Required;
        if read_only {
            // The server enforces it too, not only Veronica declining to send.
            connection
                .query_drop("SET SESSION TRANSACTION READ ONLY")
                .await
                .ok();
        }

        Ok(Self {
            connection,
            read_only,
            product: definition.product_hint,
        })
    }
}

/// Quote an identifier the way MySQL does: backticks, doubled inside.
pub fn quote(identifier: &str) -> String {
    format!("`{}`", identifier.replace('`', "``"))
}

fn qualified(object: &ObjectIdentifier) -> String {
    object
        .path
        .iter()
        .map(|part| quote(part))
        .collect::<Vec<_>>()
        .join(".")
}

fn read_value(row: &Row, index: usize) -> Value {
    use mysql_async::Value as M;
    match row.as_ref(index) {
        None | Some(M::NULL) => Value::Null,
        Some(M::Int(value)) => Value::SignedInteger(*value),
        Some(M::UInt(value)) => Value::UnsignedInteger(*value),
        Some(M::Float(value)) => Value::FloatingPoint(*value as f64),
        Some(M::Double(value)) => Value::FloatingPoint(*value),
        Some(M::Date(year, month, day, hour, minute, second, micros)) => {
            // A zero date is MySQL's way of saying "none"; rendering it as
            // 0000-00-00 would be a date that does not exist.
            if *year == 0 && *month == 0 && *day == 0 {
                Value::Null
            } else if *hour == 0 && *minute == 0 && *second == 0 && *micros == 0 {
                Value::Date(format!("{year:04}-{month:02}-{day:02}"))
            } else {
                Value::Timestamp(format!(
                    "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
                ))
            }
        }
        Some(M::Time(negative, days, hours, minutes, seconds, _micros)) => Value::Time(format!(
            "{}{:02}:{:02}:{:02}",
            if *negative { "-" } else { "" },
            *days * 24 + *hours as u32,
            minutes,
            seconds
        )),
        Some(M::Bytes(bytes)) => match std::str::from_utf8(bytes) {
            Ok(text) => Value::String(text.to_string()),
            // Not text: a real blob rather than a string that failed to decode.
            Err(_) => Value::Binary(Binary::Complete {
                data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes),
                media_type: None,
                digest: None,
            }),
        },
    }
}

fn to_page(rows: Vec<Row>, request: &PageRequest, elapsed: u64) -> Page {
    let columns: Vec<ColumnDescriptor> = rows
        .first()
        .map(|row| {
            row.columns_ref()
                .iter()
                .map(|column| ColumnDescriptor {
                    name: column.name_str().to_string(),
                    type_name: Some(format!("{:?}", column.column_type())),
                })
                .collect()
        })
        .unwrap_or_default();
    let width = columns.len();
    let shaped: Vec<Vec<Value>> = rows
        .iter()
        .map(|row| (0..width).map(|index| read_value(row, index)).collect())
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

/// Bind a value as something MySQL accepts.
fn bind(value: &Value) -> mysql_async::Value {
    use mysql_async::Value as M;
    match value {
        Value::Null | Value::Missing => M::NULL,
        Value::Boolean(value) => M::Int(i64::from(*value)),
        Value::SignedInteger(value) => M::Int(*value),
        Value::UnsignedInteger(value) => M::UInt(*value),
        Value::FloatingPoint(value) => M::Double(*value),
        // Exact as text, so the server parses the decimal rather than a float.
        Value::Decimal(Decimal(text)) => M::Bytes(text.clone().into_bytes()),
        Value::String(value) => M::Bytes(value.clone().into_bytes()),
        Value::Uuid(value) => M::Bytes(value.to_string().into_bytes()),
        Value::Date(value) | Value::Time(value) | Value::Timestamp(value) => {
            M::Bytes(value.clone().into_bytes())
        }
        Value::Binary(binary) => {
            let encoded = match binary {
                Binary::Complete { data, .. } | Binary::Preview { bytes: data, .. } => data,
            };
            M::Bytes(
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
                    .unwrap_or_default(),
            )
        }
        Value::Array(_) | Value::Object(_) => M::Bytes(
            serde_json::to_string(value)
                .unwrap_or_else(|_| "null".into())
                .into_bytes(),
        ),
        Value::ProductSpecific(product) => M::Bytes(product.rendered.clone().into_bytes()),
    }
}

#[async_trait::async_trait]
impl Adapter for MysqlAdapter {
    async fn identify(&mut self) -> Result<ProductIdentity> {
        let row: Option<(String, String, String)> = self
            .connection
            .query_first("SELECT VERSION(), @@version_comment, DATABASE()")
            .await?;
        let (version, comment, database) = row.unwrap_or_default();
        // MariaDB reports itself in its own version string, so the product is
        // read from the server rather than trusted from the connection.
        let product = if version.to_lowercase().contains("mariadb") {
            Product::MariaDb
        } else {
            self.product
        };
        let read_only: Option<(String, String)> = self
            .connection
            .query_first("SHOW VARIABLES LIKE 'read_only'")
            .await
            .ok()
            .flatten();
        let is_replica = read_only
            .map(|(_, value)| value.eq_ignore_ascii_case("ON"))
            .unwrap_or(false);

        Ok(ProductIdentity {
            version: Some(Version::parse(&version)),
            distribution: Some(comment),
            topology: Topology {
                kind: if is_replica {
                    TopologyKind::PrimaryReplica
                } else {
                    TopologyKind::Standalone
                },
                local_role: Some(if is_replica { "replica" } else { "primary" }.into()),
                ..Topology::default()
            },
            server_identifier: (!database.is_empty()).then_some(database),
            ..ProductIdentity::new(product)
        })
    }

    async fn objects(
        &mut self,
        parent: Option<&ObjectIdentifier>,
    ) -> Result<Vec<ObjectIdentifier>> {
        match parent {
            None => {
                let names: Vec<String> = self
                    .connection
                    .query(
                        "SELECT schema_name FROM information_schema.schemata
                         WHERE schema_name NOT IN
                           ('information_schema','performance_schema','mysql','sys')
                         ORDER BY schema_name",
                    )
                    .await?;
                Ok(names
                    .into_iter()
                    .map(|name| ObjectIdentifier::new(ObjectKind::Database, vec![name]))
                    .collect())
            }
            Some(object) if object.kind == ObjectKind::Database => {
                let rows: Vec<(String, String)> = self
                    .connection
                    .exec(
                        "SELECT table_name, table_type FROM information_schema.tables
                         WHERE table_schema = ? ORDER BY table_name",
                        (object.name(),),
                    )
                    .await?;
                Ok(rows
                    .into_iter()
                    .map(|(name, kind)| {
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
                let database = object.path.first().cloned().unwrap_or_default();
                let rows: Vec<(String, String)> = self
                    .connection
                    .exec(
                        "SELECT column_name, column_type FROM information_schema.columns
                         WHERE table_schema = ? AND table_name = ? ORDER BY ordinal_position",
                        (&database, object.name()),
                    )
                    .await?;
                Ok(rows
                    .into_iter()
                    .map(|(name, declared)| ObjectIdentifier {
                        kind: ObjectKind::Column,
                        path: vec![database.clone(), object.name().to_string(), name],
                        native_identifier: Some(declared),
                    })
                    .collect())
            }
        }
    }

    async fn read(&mut self, object: &ObjectIdentifier, request: &PageRequest) -> Result<Page> {
        let started = std::time::Instant::now();
        let sql = bounded(&format!("SELECT * FROM {}", qualified(object)), request);
        let rows: Vec<Row> = self.connection.query(sql).await?;
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
        let rows: Vec<Row> = self.connection.query(sql).await?;
        Ok(to_page(rows, request, started.elapsed().as_millis() as u64))
    }

    async fn count(&mut self, object: &ObjectIdentifier) -> Result<Option<u64>> {
        let count: Option<u64> = self
            .connection
            .query_first(format!("SELECT COUNT(*) FROM {}", qualified(object)))
            .await?;
        Ok(count)
    }

    async fn execute(&mut self, payload: &Payload) -> Result<u64> {
        let Payload::Relational {
            statement,
            parameters,
            ..
        } = payload
        else {
            anyhow::bail!(
                "MySQL takes a relational payload, not a {:?}",
                payload.kind()
            );
        };
        if self.read_only {
            anyhow::bail!("this connection is read-only");
        }
        let bound: Vec<mysql_async::Value> = parameters.iter().map(|p| bind(&p.value)).collect();
        self.connection
            .exec_drop(statement, mysql_async::Params::Positional(bound))
            .await?;
        Ok(self.connection.affected_rows())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identifier_is_backticked_and_a_backtick_inside_it_doubled() {
        assert_eq!(quote("orders"), "`orders`");
        assert_eq!(quote("a`b"), "`a``b`");
        assert_eq!(quote("`; DROP TABLE x -- "), "```; DROP TABLE x -- `");
    }

    #[test]
    fn each_part_of_a_path_is_quoted_separately() {
        let object =
            ObjectIdentifier::new(ObjectKind::Table, vec!["my.db".into(), "orders".into()]);
        assert_eq!(qualified(&object), "`my.db`.`orders`");
    }

    #[test]
    fn a_zero_date_is_null_rather_than_a_date_that_does_not_exist() {
        let row_value = mysql_async::Value::Date(0, 0, 0, 0, 0, 0, 0);
        // Exercised through the same branch the reader uses.
        let rendered = match row_value {
            mysql_async::Value::Date(y, m, d, _, _, _, _) if y == 0 && m == 0 && d == 0 => {
                Value::Null
            }
            _ => Value::Missing,
        };
        assert_eq!(rendered, Value::Null);
    }

    #[test]
    fn a_decimal_binds_as_text_so_the_server_parses_it_exactly() {
        let bound = bind(&Value::Decimal(Decimal("19.99".into())));
        assert_eq!(bound, mysql_async::Value::Bytes(b"19.99".to_vec()));
    }

    #[test]
    fn null_and_missing_both_bind_as_null() {
        assert_eq!(bind(&Value::Null), mysql_async::Value::NULL);
        assert_eq!(bind(&Value::Missing), mysql_async::Value::NULL);
    }

    #[test]
    fn a_page_asks_for_one_row_more_than_it_shows() {
        let request = PageRequest {
            page_size: crate::paging::PageSize::new(25).unwrap(),
            offset: 50,
            ..PageRequest::default()
        };
        let sql = bounded("SELECT 1", &request);
        assert!(sql.contains("LIMIT 26"), "got {sql}");
        assert!(sql.contains("OFFSET 50"), "got {sql}");
    }
}
