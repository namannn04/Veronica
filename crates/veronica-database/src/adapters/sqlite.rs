//! SQLite.
//!
//! The simplest adapter and the one worth reading first: it is a file, so
//! everything here can be tested against a real database with no server, and
//! the other adapters follow the same shape.
//!
//! `rusqlite` is synchronous, so every call goes through `spawn_blocking`. A
//! query against a large table on a slow disk would otherwise stall the runtime
//! and freeze the interface it was called from.

use anyhow::{Context, Result};
use rusqlite::{Connection as Sqlite, OpenFlags};

use crate::connection::{ConnectionDefinition, Location, SqliteAccessMode};
use crate::identify::{ObjectIdentifier, ObjectKind};
use crate::mutation::Payload;
use crate::paging::{ColumnDescriptor, Page, PageRequest};
use crate::product::{Product, ProductIdentity, Topology, TopologyKind, Version};
use crate::value::{Binary, Decimal, Value};

use super::Adapter;

pub struct SqliteAdapter {
    path: String,
    read_only: bool,
}

impl SqliteAdapter {
    /// Open from a stored definition.
    pub fn open(connection: &ConnectionDefinition) -> Result<Self> {
        let (path, access) = match &connection.location {
            Location::Sqlite { sqlite } => (sqlite.path.clone(), sqlite.access_mode),
            Location::Memory { name } => (
                // A shared cache URI, so two handles see the same database
                // rather than each getting a private empty one.
                match name {
                    Some(name) => format!("file:{name}?mode=memory&cache=shared"),
                    None => ":memory:".to_string(),
                },
                SqliteAccessMode::ReadWrite,
            ),
            Location::Network { .. } => {
                anyhow::bail!("SQLite is a file; this connection names a network endpoint")
            }
        };
        Ok(Self {
            read_only: access == SqliteAccessMode::ReadOnly
                || connection.read_only_policy == crate::connection::ReadOnlyPolicy::Required,
            path,
        })
    }

    fn connect(&self) -> Result<Sqlite> {
        let mut flags = OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        // Read-only is enforced by the file handle, not only by Veronica
        // refusing to send a write. Two locks are better than one.
        flags |= if self.read_only {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
        };
        Sqlite::open_with_flags(&self.path, flags)
            .with_context(|| format!("cannot open {}", self.path))
    }

    /// Run something on the blocking pool.
    async fn blocking<T, F>(&self, work: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(Sqlite) -> Result<T> + Send + 'static,
    {
        let path = self.path.clone();
        let read_only = self.read_only;
        tokio::task::spawn_blocking(move || {
            let adapter = SqliteAdapter { path, read_only };
            work(adapter.connect()?)
        })
        .await
        .context("the SQLite task did not finish")?
    }
}

/// Quote an identifier the way SQLite does: double quotes, doubled inside.
///
/// This is what makes a table called `"; DROP TABLE --` a table with a silly
/// name rather than an incident.
pub fn quote(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// Turn one column of one row into a value.
fn read_value(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Value> {
    use rusqlite::types::ValueRef;
    Ok(match row.get_ref(index)? {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => Value::SignedInteger(value),
        // SQLite has no decimal type, so a float is a float and is reported as
        // one rather than being dressed up as exact.
        ValueRef::Real(value) => Value::FloatingPoint(value),
        ValueRef::Text(bytes) => Value::String(String::from_utf8_lossy(bytes).into_owned()),
        ValueRef::Blob(bytes) => Value::Binary(if bytes.len() <= MAX_INLINE_BLOB {
            Binary::Complete {
                data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes),
                media_type: None,
                digest: None,
            }
        } else {
            // A grid must be able to describe a large blob without carrying it.
            Binary::Preview {
                byte_count: bytes.len() as u64,
                bytes: base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    &bytes[..MAX_INLINE_BLOB],
                ),
                media_type: None,
                digest: None,
            }
        }),
    })
}

/// How much of a blob a page carries inline.
const MAX_INLINE_BLOB: usize = 4 * 1024;

/// Run a statement and shape the result into a page.
fn page(connection: &Sqlite, sql: &str, request: &PageRequest) -> Result<Page> {
    let started = std::time::Instant::now();
    let mut statement = connection
        .prepare(sql)
        .with_context(|| format!("SQLite refused the statement: {sql}"))?;
    let columns: Vec<ColumnDescriptor> = statement
        .column_names()
        .into_iter()
        .map(|name| ColumnDescriptor {
            name: name.to_string(),
            type_name: None,
        })
        .collect();
    let width = columns.len();

    let mut rows = Vec::new();
    let mut cursor = statement.query([])?;
    while let Some(row) = cursor.next()? {
        let mut values = Vec::with_capacity(width);
        for index in 0..width {
            values.push(read_value(row, index)?);
        }
        rows.push(values);
    }
    Ok(Page::from_rows(
        columns,
        rows,
        request,
        started.elapsed().as_millis() as u64,
    ))
}

/// `LIMIT ?/OFFSET ?` with one extra row, so `has_more` is known.
fn bounded(sql: &str, request: &PageRequest) -> String {
    format!(
        "SELECT * FROM ({sql}) LIMIT {} OFFSET {}",
        request.page_size.get() as u64 + 1,
        request.offset
    )
}

#[async_trait::async_trait]
impl Adapter for SqliteAdapter {
    async fn identify(&mut self) -> Result<ProductIdentity> {
        let path = self.path.clone();
        self.blocking(move |connection| {
            let version: String =
                connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
            Ok(ProductIdentity {
                version: Some(Version::parse(&version)),
                topology: Topology {
                    // A file is not a server, and calling it standalone would
                    // imply there is something to fail over to.
                    kind: TopologyKind::Embedded,
                    ..Topology::default()
                },
                server_identifier: Some(path),
                ..ProductIdentity::new(Product::Sqlite)
            })
        })
        .await
    }

    async fn objects(
        &mut self,
        parent: Option<&ObjectIdentifier>,
    ) -> Result<Vec<ObjectIdentifier>> {
        match parent {
            // The top level is the tables and views in the file.
            None => {
                self.blocking(|connection| {
                    let mut statement = connection.prepare(
                        "SELECT name, type FROM sqlite_master
                         WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%'
                         ORDER BY name",
                    )?;
                    let rows = statement.query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?;
                    let mut objects = Vec::new();
                    for row in rows {
                        let (name, kind) = row?;
                        objects.push(ObjectIdentifier::new(
                            if kind == "view" {
                                ObjectKind::View
                            } else {
                                ObjectKind::Table
                            },
                            vec![name],
                        ));
                    }
                    Ok(objects)
                })
                .await
            }
            // Inside a table: its columns.
            Some(object) => {
                let table = object.name().to_string();
                self.blocking(move |connection| {
                    let mut statement =
                        connection.prepare(&format!("PRAGMA table_info({})", quote(&table)))?;
                    let rows = statement.query_map([], |row| {
                        Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?))
                    })?;
                    let mut columns = Vec::new();
                    for row in rows {
                        let (name, declared) = row?;
                        columns.push(ObjectIdentifier {
                            kind: ObjectKind::Column,
                            path: vec![table.clone(), name],
                            native_identifier: Some(declared),
                        });
                    }
                    Ok(columns)
                })
                .await
            }
        }
    }

    async fn read(&mut self, object: &ObjectIdentifier, request: &PageRequest) -> Result<Page> {
        let sql = bounded(&format!("SELECT * FROM {}", quote(object.name())), request);
        let request = request.clone();
        self.blocking(move |connection| page(&connection, &sql, &request))
            .await
    }

    async fn query(&mut self, statement: &str, request: &PageRequest) -> Result<Page> {
        if !super::is_read_only_sql(statement) {
            anyhow::bail!(
                "`query` only runs statements that read. A change goes through \
                 `vr database mutations`, which previews it first."
            );
        }
        let sql = bounded(statement.trim().trim_end_matches(';'), request);
        let request = request.clone();
        self.blocking(move |connection| page(&connection, &sql, &request))
            .await
    }

    async fn count(&mut self, object: &ObjectIdentifier) -> Result<Option<u64>> {
        let sql = format!("SELECT COUNT(*) FROM {}", quote(object.name()));
        self.blocking(move |connection| {
            let count: i64 = connection.query_row(&sql, [], |row| row.get(0))?;
            Ok(Some(count as u64))
        })
        .await
    }

    async fn execute(&mut self, payload: &Payload) -> Result<u64> {
        let Payload::Relational {
            statement,
            parameters,
            ..
        } = payload
        else {
            anyhow::bail!(
                "SQLite takes a relational payload, not a {:?}",
                payload.kind()
            );
        };
        if self.read_only {
            anyhow::bail!("this connection is read-only");
        }
        let statement = statement.clone();
        let parameters = parameters.clone();
        self.blocking(move |connection| {
            let mut prepared = connection.prepare(&statement)?;
            // Values are bound, never interpolated. The statement the guard
            // signed is the statement that runs, character for character.
            let bound: Vec<rusqlite::types::Value> =
                parameters.iter().map(|p| to_sqlite(&p.value)).collect();
            let affected = prepared.execute(rusqlite::params_from_iter(bound))?;
            Ok(affected as u64)
        })
        .await
    }
}

/// Map a value onto what SQLite can bind.
fn to_sqlite(value: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as S;
    match value {
        Value::Null | Value::Missing => S::Null,
        Value::Boolean(value) => S::Integer(i64::from(*value)),
        Value::SignedInteger(value) => S::Integer(*value),
        Value::UnsignedInteger(value) => S::Integer(*value as i64),
        Value::FloatingPoint(value) => S::Real(*value),
        // A decimal keeps its exact text: SQLite has no decimal type, and
        // rounding it through a float here is how money goes missing.
        Value::Decimal(Decimal(text)) => S::Text(text.clone()),
        Value::String(value) => S::Text(value.clone()),
        Value::Uuid(value) => S::Text(value.to_string()),
        Value::Date(value) | Value::Time(value) | Value::Timestamp(value) => S::Text(value.clone()),
        Value::Binary(binary) => {
            let encoded = match binary {
                Binary::Complete { data, .. } | Binary::Preview { bytes: data, .. } => data,
            };
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
                .map(S::Blob)
                .unwrap_or(S::Null)
        }
        // A structure has no SQLite column type; JSON text is what SQLite's own
        // json1 functions read.
        Value::Array(_) | Value::Object(_) => {
            S::Text(serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()))
        }
        Value::ProductSpecific(product) => S::Text(product.rendered.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::*;
    use crate::mutation::Parameter;
    use crate::paging::PageSize;
    use uuid::Uuid;

    fn definition(path: &str, access: SqliteAccessMode) -> ConnectionDefinition {
        let now = chrono::Utc::now();
        ConnectionDefinition {
            version: crate::connection::SCHEMA_VERSION,
            id: Uuid::new_v4(),
            display_name: "Test".into(),
            product_hint: Product::Sqlite,
            location: Location::Sqlite {
                sqlite: SqliteLocation {
                    path: path.into(),
                    access_mode: access,
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

    /// A real database on disk, seeded.
    async fn seeded() -> (tempfile::TempDir, SqliteAdapter) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.db");
        {
            let connection = Sqlite::open(&path).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE orders (id INTEGER PRIMARY KEY, total REAL, note TEXT, blob BLOB);
                     CREATE VIEW recent AS SELECT * FROM orders;
                     INSERT INTO orders (id, total, note) VALUES (1, 9.99, 'first');
                     INSERT INTO orders (id, total, note) VALUES (2, 19.99, NULL);
                     INSERT INTO orders (id, total, note) VALUES (3, 29.99, 'third');",
                )
                .unwrap();
        }
        let adapter = SqliteAdapter::open(&definition(
            path.to_str().unwrap(),
            SqliteAccessMode::ReadWrite,
        ))
        .unwrap();
        (directory, adapter)
    }

    #[test]
    fn an_identifier_with_a_quote_in_it_is_escaped_not_interpolated() {
        // This is what makes a silly table name a silly name and not an incident.
        assert_eq!(quote("orders"), "\"orders\"");
        assert_eq!(quote(r#"a"b"#), r#""a""b""#);
        // The leading quote is doubled, so the whole thing stays one
        // identifier and the `DROP` is part of its name.
        assert_eq!(quote(r#""; DROP TABLE x --"#), r#""""; DROP TABLE x --""#);
    }

    #[tokio::test]
    async fn it_identifies_a_real_file_as_an_embedded_database() {
        let (_directory, mut adapter) = seeded().await;
        let identity = adapter.identify().await.unwrap();
        assert_eq!(identity.product, Product::Sqlite);
        assert_eq!(identity.topology.kind, TopologyKind::Embedded);
        assert!(identity.version.unwrap().major.unwrap() >= 3);
    }

    #[tokio::test]
    async fn browsing_lists_the_tables_and_views_but_not_sqlites_own() {
        let (_directory, mut adapter) = seeded().await;
        let objects = adapter.objects(None).await.unwrap();
        let names: Vec<&str> = objects.iter().map(|object| object.name()).collect();
        assert_eq!(names, ["orders", "recent"]);
        assert_eq!(objects[0].kind, ObjectKind::Table);
        assert_eq!(objects[1].kind, ObjectKind::View);
        assert!(
            !names.iter().any(|name| name.starts_with("sqlite_")),
            "internal tables are not the user's"
        );
    }

    #[tokio::test]
    async fn browsing_into_a_table_lists_its_columns_with_their_types() {
        let (_directory, mut adapter) = seeded().await;
        let table = ObjectIdentifier::new(ObjectKind::Table, vec!["orders".into()]);
        let columns = adapter.objects(Some(&table)).await.unwrap();
        let names: Vec<&str> = columns.iter().map(|column| column.name()).collect();
        assert_eq!(names, ["id", "total", "note", "blob"]);
        assert_eq!(columns[0].kind, ObjectKind::Column);
        assert_eq!(columns[1].native_identifier.as_deref(), Some("REAL"));
    }

    #[tokio::test]
    async fn reading_a_table_returns_typed_values() {
        let (_directory, mut adapter) = seeded().await;
        let table = ObjectIdentifier::new(ObjectKind::Table, vec!["orders".into()]);
        let page = adapter.read(&table, &PageRequest::default()).await.unwrap();
        assert_eq!(page.rows.len(), 3);
        assert_eq!(
            page.columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["id", "total", "note", "blob"]
        );
        assert_eq!(page.rows[0][0], Value::SignedInteger(1));
        assert_eq!(page.rows[0][1], Value::FloatingPoint(9.99));
        assert_eq!(page.rows[0][2], Value::String("first".into()));
        // A NULL column is NULL, not an empty string.
        assert_eq!(page.rows[1][2], Value::Null);
        assert!(!page.has_more);
    }

    #[tokio::test]
    async fn a_page_stops_at_its_size_and_says_there_is_more() {
        let (_directory, mut adapter) = seeded().await;
        let table = ObjectIdentifier::new(ObjectKind::Table, vec!["orders".into()]);
        let request = PageRequest {
            page_size: PageSize::new(2).unwrap(),
            ..PageRequest::default()
        };
        let page = adapter.read(&table, &request).await.unwrap();
        assert_eq!(page.rows.len(), 2);
        assert!(page.has_more);
        assert_eq!(page.next_offset, Some(2));

        let second = adapter
            .read(
                &table,
                &PageRequest {
                    offset: 2,
                    ..request
                },
            )
            .await
            .unwrap();
        assert_eq!(second.rows.len(), 1);
        assert!(!second.has_more);
    }

    #[tokio::test]
    async fn a_query_that_reads_runs_and_one_that_writes_does_not() {
        let (_directory, mut adapter) = seeded().await;
        let page = adapter
            .query(
                "SELECT note FROM orders WHERE id = 1",
                &PageRequest::default(),
            )
            .await
            .unwrap();
        assert_eq!(page.rows[0][0], Value::String("first".into()));

        let error = adapter
            .query("DELETE FROM orders", &PageRequest::default())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("only runs statements that read"),
            "got {error}"
        );
        // And nothing was deleted.
        assert_eq!(
            adapter
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
    async fn counting_says_how_many_there_are() {
        let (_directory, mut adapter) = seeded().await;
        let table = ObjectIdentifier::new(ObjectKind::Table, vec!["orders".into()]);
        assert_eq!(adapter.count(&table).await.unwrap(), Some(3));
    }

    #[tokio::test]
    async fn a_mutation_binds_its_values_rather_than_interpolating_them() {
        let (_directory, mut adapter) = seeded().await;
        // A value that would be catastrophic if it reached the statement.
        let affected = adapter
            .execute(&Payload::Relational {
                product: Product::Sqlite,
                statement: "UPDATE orders SET note = ?1 WHERE id = ?2".into(),
                parameters: vec![
                    Parameter {
                        name: "note".into(),
                        value: Value::String("'; DROP TABLE orders; --".into()),
                    },
                    Parameter {
                        name: "id".into(),
                        value: Value::SignedInteger(1),
                    },
                ],
            })
            .await
            .unwrap();
        assert_eq!(affected, 1);

        // The table is still there, and the value went in verbatim.
        let page = adapter
            .query(
                "SELECT note FROM orders WHERE id = 1",
                &PageRequest::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            page.rows[0][0],
            Value::String("'; DROP TABLE orders; --".into())
        );
    }

    #[tokio::test]
    async fn a_read_only_connection_refuses_a_write_at_the_file_handle() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.db");
        Sqlite::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE t (a INT); INSERT INTO t VALUES (1);")
            .unwrap();

        let mut adapter = SqliteAdapter::open(&definition(
            path.to_str().unwrap(),
            SqliteAccessMode::ReadOnly,
        ))
        .unwrap();
        // Reading is fine.
        assert_eq!(
            adapter
                .count(&ObjectIdentifier::new(ObjectKind::Table, vec!["t".into()]))
                .await
                .unwrap(),
            Some(1)
        );
        // Writing is refused before it reaches SQLite at all.
        let error = adapter
            .execute(&Payload::Relational {
                product: Product::Sqlite,
                statement: "DELETE FROM t".into(),
                parameters: vec![],
            })
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("read-only"), "got {error}");
    }

    #[tokio::test]
    async fn a_payload_for_another_family_is_refused() {
        let (_directory, mut adapter) = seeded().await;
        let error = adapter
            .execute(&Payload::Keyspace {
                product: Product::Redis,
                command: "DEL".into(),
                arguments: vec![],
            })
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("relational payload"), "got {error}");
    }

    #[tokio::test]
    async fn a_large_blob_comes_back_as_a_preview_rather_than_whole() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("blobs.db");
        {
            let connection = Sqlite::open(&path).unwrap();
            connection
                .execute_batch("CREATE TABLE files (data BLOB);")
                .unwrap();
            connection
                .execute("INSERT INTO files VALUES (zeroblob(100000))", [])
                .unwrap();
        }
        let mut adapter = SqliteAdapter::open(&definition(
            path.to_str().unwrap(),
            SqliteAccessMode::ReadWrite,
        ))
        .unwrap();
        let page = adapter
            .read(
                &ObjectIdentifier::new(ObjectKind::Table, vec!["files".into()]),
                &PageRequest::default(),
            )
            .await
            .unwrap();
        let Value::Binary(binary) = &page.rows[0][0] else {
            panic!("expected a blob, got {:?}", page.rows[0][0]);
        };
        assert!(!binary.is_complete(), "100 KB should not be carried whole");
        assert_eq!(binary.byte_count(), 100_000, "but its size is reported");
    }

    #[tokio::test]
    async fn a_table_with_an_awkward_name_can_still_be_read() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("odd.db");
        {
            let connection = Sqlite::open(&path).unwrap();
            connection
                .execute_batch(r#"CREATE TABLE "a""b" (x INT); INSERT INTO "a""b" VALUES (1);"#)
                .unwrap();
        }
        let mut adapter = SqliteAdapter::open(&definition(
            path.to_str().unwrap(),
            SqliteAccessMode::ReadWrite,
        ))
        .unwrap();
        let objects = adapter.objects(None).await.unwrap();
        assert_eq!(objects[0].name(), r#"a"b"#);
        let page = adapter
            .read(&objects[0], &PageRequest::default())
            .await
            .unwrap();
        assert_eq!(page.rows.len(), 1);
    }

    #[test]
    fn a_decimal_is_bound_as_its_exact_text() {
        // Rounding it through a float is how money goes missing.
        let bound = to_sqlite(&Value::Decimal(Decimal("19.99".into())));
        assert_eq!(bound, rusqlite::types::Value::Text("19.99".into()));
    }

    #[test]
    fn missing_and_null_both_bind_as_null() {
        assert_eq!(to_sqlite(&Value::Missing), rusqlite::types::Value::Null);
        assert_eq!(to_sqlite(&Value::Null), rusqlite::types::Value::Null);
    }
}
