//! Where Veronica keeps what it knows about your databases.
//!
//! Edith stores this in SQLite, and so does Veronica — which is the answer to
//! "does Edith use a database itself": yes, one, for this feature's own
//! bookkeeping. Saved connections, saved queries, the operation history and the
//! confirmation receipts all live in one file under the state directory.
//!
//! **No secret is in this file.** Definitions carry references; the values are
//! in the keyring. The file is still written 0600, because knowing which hosts
//! and databases somebody connects to is worth something on its own.
//!
//! The schema is versioned and migrated forward on open. A file from a newer
//! build is refused rather than read with the wrong assumptions.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection as Sqlite, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connection::ConnectionDefinition;
use crate::guard::{Receipt, ReceiptStore};

/// Bumped whenever the schema changes. A file from a newer build is refused.
pub const SCHEMA_VERSION: i64 = 1;

/// Nothing unbounded, so one runaway page cannot fill the disk with history.
pub const MAX_OPERATIONS: usize = 5_000;
pub const MAX_SAVED_QUERIES: usize = 2_000;
pub const MAX_QUERY_BYTES: usize = 256 * 1024;

/// A query somebody kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    pub id: Uuid,
    pub connection_id: Option<Uuid>,
    pub name: String,
    pub text: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// What happened, so a mutation is not a thing that occurs invisibly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationOutcome {
    Succeeded,
    Failed,
    /// Previewed and never applied. Worth recording: it says somebody looked at
    /// dropping a table and thought better of it.
    Abandoned,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecord {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub connection_name: String,
    /// `query`, `browse`, `mutation` — what kind of thing this was.
    pub kind: String,
    /// The redacted summary. Never a parameter value.
    pub summary: String,
    pub outcome: OperationOutcome,
    pub affected_records: Option<u64>,
    pub elapsed_millis: u64,
    pub error: Option<String>,
    pub at: chrono::DateTime<chrono::Utc>,
}

/// The metadata store.
pub struct MetadataStore {
    connection: Sqlite,
    path: PathBuf,
}

impl std::fmt::Debug for MetadataStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetadataStore")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl MetadataStore {
    /// Open, creating and migrating as needed.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        let connection =
            Sqlite::open(&path).with_context(|| format!("cannot open {}", path.display()))?;

        // Write-ahead logging so a reader — the CLI — does not block the app
        // mid-write, and foreign keys so a saved query cannot outlive its
        // connection.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;

        let mut store = Self { connection, path };
        store.migrate()?;
        store.restrict_permissions()?;
        Ok(store)
    }

    /// In memory, for tests and for a dry run.
    pub fn in_memory() -> Result<Self> {
        let connection = Sqlite::open_in_memory()?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let mut store = Self {
            connection,
            path: PathBuf::from(":memory:"),
        };
        store.migrate()?;
        Ok(store)
    }

    fn restrict_permissions(&self) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        // Which hosts and databases somebody connects to is worth something on
        // its own, even with no password in the file.
        for suffix in ["", "-wal", "-shm"] {
            let path = PathBuf::from(format!("{}{suffix}", self.path.display()));
            if path.exists() {
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
        }
        Ok(())
    }

    fn migrate(&mut self) -> Result<()> {
        let version: i64 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            anyhow::bail!(
                "this database metadata file is version {version}; this build reads \
                 version {SCHEMA_VERSION}. A newer Veronica wrote it."
            );
        }
        if version == SCHEMA_VERSION {
            return Ok(());
        }

        self.connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS connections (
                id           TEXT PRIMARY KEY NOT NULL,
                display_name TEXT NOT NULL,
                product      TEXT NOT NULL,
                environment  TEXT NOT NULL,
                definition   TEXT NOT NULL,
                updated_at   TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS connections_name ON connections(display_name);

            CREATE TABLE IF NOT EXISTS saved_queries (
                id            TEXT PRIMARY KEY NOT NULL,
                connection_id TEXT REFERENCES connections(id) ON DELETE CASCADE,
                name          TEXT NOT NULL,
                text          TEXT NOT NULL,
                created_at    TEXT NOT NULL,
                updated_at    TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS saved_queries_connection
                ON saved_queries(connection_id);

            CREATE TABLE IF NOT EXISTS operations (
                id                TEXT PRIMARY KEY NOT NULL,
                connection_id     TEXT NOT NULL,
                connection_name   TEXT NOT NULL,
                kind              TEXT NOT NULL,
                summary           TEXT NOT NULL,
                outcome           TEXT NOT NULL,
                affected_records  INTEGER,
                elapsed_millis    INTEGER NOT NULL,
                error             TEXT,
                at                TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS operations_at ON operations(at DESC);

            -- One row per outstanding preview. Deleting the row is what makes a
            -- token single-use, so this table is the enforcement point.
            CREATE TABLE IF NOT EXISTS confirmations (
                id                TEXT PRIMARY KEY NOT NULL,
                effect_digest     TEXT NOT NULL,
                expires_at_millis INTEGER NOT NULL
            );
            "#,
        )?;
        self.connection
            .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    // ---------------------------------------------------------- connections

    pub fn save_connection(&self, connection: &ConnectionDefinition) -> Result<()> {
        self.connection.execute(
            "INSERT INTO connections (id, display_name, product, environment, definition, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                display_name = excluded.display_name,
                product      = excluded.product,
                environment  = excluded.environment,
                definition   = excluded.definition,
                updated_at   = excluded.updated_at",
            params![
                connection.id.to_string(),
                connection.display_name,
                connection.product_hint.key(),
                connection.environment.kind.key(),
                serde_json::to_string(connection)?,
                connection.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn connections(&self) -> Result<Vec<ConnectionDefinition>> {
        let mut statement = self
            .connection
            .prepare("SELECT definition FROM connections ORDER BY display_name COLLATE NOCASE")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut connections = Vec::new();
        for row in rows {
            // One unreadable row — written by a newer build, or edited by hand
            // — must not lose the others.
            match serde_json::from_str(&row?) {
                Ok(definition) => connections.push(definition),
                Err(error) => tracing::warn!("skipping an unreadable connection: {error}"),
            }
        }
        Ok(connections)
    }

    pub fn connection(&self, id: Uuid) -> Result<Option<ConnectionDefinition>> {
        let row: Option<String> = self
            .connection
            .query_row(
                "SELECT definition FROM connections WHERE id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(row.map(|json| serde_json::from_str(&json)).transpose()?)
    }

    /// Find one by id, or by a unique prefix of its id, or by its name.
    ///
    /// Typing a full UUID is not something anybody does willingly. An ambiguous
    /// prefix is an error rather than a guess: picking one of two production
    /// databases at random is not a thing to do.
    pub fn resolve(&self, needle: &str) -> Result<ConnectionDefinition> {
        let needle = needle.trim();
        if let Ok(id) = Uuid::parse_str(needle) {
            if let Some(found) = self.connection(id)? {
                return Ok(found);
            }
        }
        let connections = self.connections()?;
        let matches: Vec<&ConnectionDefinition> = connections
            .iter()
            .filter(|candidate| {
                candidate.display_name.eq_ignore_ascii_case(needle)
                    || candidate.id.to_string().starts_with(&needle.to_lowercase())
            })
            .collect();
        match matches.len() {
            1 => Ok(matches[0].clone()),
            0 => anyhow::bail!(
                "no connection called {needle:?}; run `vr database connections` for the list"
            ),
            _ => anyhow::bail!(
                "{needle:?} matches {} connections: {}. Use the full id.",
                matches.len(),
                matches
                    .iter()
                    .map(|found| found.display_name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    pub fn remove_connection(&self, id: Uuid) -> Result<bool> {
        let removed = self.connection.execute(
            "DELETE FROM connections WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(removed > 0)
    }

    // -------------------------------------------------------- saved queries

    pub fn save_query(&self, query: &SavedQuery) -> Result<()> {
        if query.text.len() > MAX_QUERY_BYTES {
            anyhow::bail!(
                "a saved query is limited to {MAX_QUERY_BYTES} bytes; that one is {}",
                query.text.len()
            );
        }
        let count: usize =
            self.connection
                .query_row("SELECT COUNT(*) FROM saved_queries", [], |row| row.get(0))?;
        if count >= MAX_SAVED_QUERIES && self.saved_query(query.id)?.is_none() {
            anyhow::bail!("there are already {MAX_SAVED_QUERIES} saved queries");
        }
        self.connection.execute(
            "INSERT INTO saved_queries (id, connection_id, name, text, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                connection_id = excluded.connection_id,
                name          = excluded.name,
                text          = excluded.text,
                updated_at    = excluded.updated_at",
            params![
                query.id.to_string(),
                query.connection_id.map(|id| id.to_string()),
                query.name,
                query.text,
                query.created_at.to_rfc3339(),
                query.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    fn row_to_query(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedQuery> {
        Ok(SavedQuery {
            id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or(Uuid::nil()),
            connection_id: row
                .get::<_, Option<String>>(1)?
                .and_then(|id| Uuid::parse_str(&id).ok()),
            name: row.get(2)?,
            text: row.get(3)?,
            created_at: parse_time(&row.get::<_, String>(4)?),
            updated_at: parse_time(&row.get::<_, String>(5)?),
        })
    }

    pub fn saved_queries(&self, connection_id: Option<Uuid>) -> Result<Vec<SavedQuery>> {
        let mut statement = self.connection.prepare(
            "SELECT id, connection_id, name, text, created_at, updated_at
             FROM saved_queries
             WHERE ?1 IS NULL OR connection_id = ?1
             ORDER BY name COLLATE NOCASE",
        )?;
        let rows = statement.query_map(
            params![connection_id.map(|id| id.to_string())],
            Self::row_to_query,
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn saved_query(&self, id: Uuid) -> Result<Option<SavedQuery>> {
        Ok(self
            .connection
            .query_row(
                "SELECT id, connection_id, name, text, created_at, updated_at
                 FROM saved_queries WHERE id = ?1",
                params![id.to_string()],
                Self::row_to_query,
            )
            .optional()?)
    }

    pub fn remove_query(&self, id: Uuid) -> Result<bool> {
        Ok(self.connection.execute(
            "DELETE FROM saved_queries WHERE id = ?1",
            params![id.to_string()],
        )? > 0)
    }

    // ------------------------------------------------------------ operations

    pub fn record_operation(&self, record: &OperationRecord) -> Result<()> {
        self.connection.execute(
            "INSERT INTO operations
             (id, connection_id, connection_name, kind, summary, outcome,
              affected_records, elapsed_millis, error, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record.id.to_string(),
                record.connection_id.to_string(),
                record.connection_name,
                record.kind,
                record.summary,
                serde_json::to_string(&record.outcome)?.trim_matches('"'),
                record.affected_records.map(|count| count as i64),
                record.elapsed_millis as i64,
                record.error,
                record.at.to_rfc3339(),
            ],
        )?;
        // Bounded, so history cannot fill the disk. Oldest first out.
        self.connection.execute(
            "DELETE FROM operations WHERE id NOT IN
                (SELECT id FROM operations ORDER BY at DESC LIMIT ?1)",
            params![MAX_OPERATIONS as i64],
        )?;
        Ok(())
    }

    pub fn operations(&self, limit: usize) -> Result<Vec<OperationRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, connection_id, connection_name, kind, summary, outcome,
                    affected_records, elapsed_millis, error, at
             FROM operations ORDER BY at DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit as i64], |row| {
            Ok(OperationRecord {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or(Uuid::nil()),
                connection_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or(Uuid::nil()),
                connection_name: row.get(2)?,
                kind: row.get(3)?,
                summary: row.get(4)?,
                outcome: serde_json::from_str(&format!("\"{}\"", row.get::<_, String>(5)?))
                    .unwrap_or(OperationOutcome::Failed),
                affected_records: row.get::<_, Option<i64>>(6)?.map(|count| count as u64),
                elapsed_millis: row.get::<_, i64>(7)? as u64,
                error: row.get(8)?,
                at: parse_time(&row.get::<_, String>(9)?),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

fn parse_time(raw: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|time| time.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::DateTime::UNIX_EPOCH)
}

/// The store is where single-use is enforced: deleting the row is what spends
/// the token, and SQLite's own atomicity is what makes it exactly once even
/// with the app and the CLI both running.
impl ReceiptStore for MetadataStore {
    fn register(&mut self, receipt: Receipt) -> Result<()> {
        let inserted = self.connection.execute(
            "INSERT OR IGNORE INTO confirmations (id, effect_digest, expires_at_millis)
             VALUES (?1, ?2, ?3)",
            params![
                receipt.identifier.to_string(),
                receipt.effect_digest,
                receipt.expires_at_millis
            ],
        )?;
        if inserted == 0 {
            anyhow::bail!("a confirmation with that identifier already exists");
        }
        Ok(())
    }

    fn consume(&mut self, identifier: Uuid) -> Result<Option<Receipt>> {
        // `DELETE ... RETURNING` so the read and the removal are one statement
        // and two processes cannot both spend the same token.
        let receipt = self
            .connection
            .query_row(
                "DELETE FROM confirmations WHERE id = ?1
                 RETURNING id, effect_digest, expires_at_millis",
                params![identifier.to_string()],
                |row| {
                    Ok(Receipt {
                        identifier: Uuid::parse_str(&row.get::<_, String>(0)?)
                            .unwrap_or(Uuid::nil()),
                        effect_digest: row.get(1)?,
                        expires_at_millis: row.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(receipt)
    }

    fn purge_expired(&mut self, now_millis: i64) -> Result<usize> {
        Ok(self.connection.execute(
            "DELETE FROM confirmations WHERE expires_at_millis <= ?1",
            params![now_millis],
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::*;
    use crate::product::Product;

    fn store() -> MetadataStore {
        MetadataStore::in_memory().unwrap()
    }

    fn definition(name: &str, product: Product) -> ConnectionDefinition {
        let now = chrono::Utc::now();
        ConnectionDefinition {
            version: crate::connection::SCHEMA_VERSION,
            id: Uuid::new_v4(),
            display_name: name.into(),
            product_hint: product,
            location: Location::Network {
                endpoints: vec![NetworkEndpoint {
                    host: "db".into(),
                    port: Port::new(5432).unwrap(),
                    role: EndpointRole::Primary,
                }],
            },
            username: Some("app".into()),
            namespaces: NamespaceDefaults::default(),
            deployment_mode: DeploymentMode::Automatic,
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

    #[test]
    fn a_connection_round_trips_whole() {
        let store = store();
        let original = definition("App", Product::Postgresql);
        store.save_connection(&original).unwrap();
        assert_eq!(store.connection(original.id).unwrap(), Some(original));
    }

    #[test]
    fn saving_the_same_connection_twice_updates_rather_than_duplicates() {
        let store = store();
        let mut connection = definition("App", Product::Postgresql);
        store.save_connection(&connection).unwrap();
        connection.display_name = "Renamed".into();
        store.save_connection(&connection).unwrap();
        let all = store.connections().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].display_name, "Renamed");
    }

    #[test]
    fn connections_come_back_sorted_by_name_regardless_of_case() {
        let store = store();
        for name in ["zebra", "Apple", "mango"] {
            store
                .save_connection(&definition(name, Product::Postgresql))
                .unwrap();
        }
        let names: Vec<String> = store
            .connections()
            .unwrap()
            .into_iter()
            .map(|connection| connection.display_name)
            .collect();
        assert_eq!(names, ["Apple", "mango", "zebra"]);
    }

    #[test]
    fn a_connection_resolves_by_id_prefix_or_name() {
        let store = store();
        let connection = definition("Production", Product::Postgresql);
        store.save_connection(&connection).unwrap();

        assert_eq!(
            store.resolve(&connection.id.to_string()).unwrap().id,
            connection.id
        );
        assert_eq!(store.resolve("Production").unwrap().id, connection.id);
        assert_eq!(store.resolve("production").unwrap().id, connection.id);
        let prefix = &connection.id.to_string()[..8];
        assert_eq!(store.resolve(prefix).unwrap().id, connection.id);
    }

    #[test]
    fn an_ambiguous_name_is_an_error_rather_than_a_guess() {
        // Picking one of two production databases at random is not a thing to do.
        let store = store();
        let mut first = definition("App", Product::Postgresql);
        first.id = Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000001").unwrap();
        let mut second = definition("App", Product::Mysql);
        second.id = Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000002").unwrap();
        store.save_connection(&first).unwrap();
        store.save_connection(&second).unwrap();

        let error = store.resolve("App").unwrap_err().to_string();
        assert!(error.contains("matches 2"), "got {error}");
        let error = store.resolve("aaaaaaaa").unwrap_err().to_string();
        assert!(error.contains("Use the full id"), "got {error}");
    }

    #[test]
    fn an_unknown_connection_says_where_to_look() {
        let error = store().resolve("nothing").unwrap_err().to_string();
        assert!(error.contains("vr database connections"), "got {error}");
    }

    #[test]
    fn removing_a_connection_takes_its_saved_queries_with_it() {
        let store = store();
        let connection = definition("App", Product::Postgresql);
        store.save_connection(&connection).unwrap();
        let now = chrono::Utc::now();
        store
            .save_query(&SavedQuery {
                id: Uuid::new_v4(),
                connection_id: Some(connection.id),
                name: "recent orders".into(),
                text: "SELECT 1".into(),
                created_at: now,
                updated_at: now,
            })
            .unwrap();
        assert_eq!(store.saved_queries(None).unwrap().len(), 1);

        assert!(store.remove_connection(connection.id).unwrap());
        assert!(
            store.saved_queries(None).unwrap().is_empty(),
            "an orphaned query would point at nothing"
        );
        assert!(!store.remove_connection(connection.id).unwrap());
    }

    #[test]
    fn a_saved_query_larger_than_the_limit_is_refused() {
        let store = store();
        let now = chrono::Utc::now();
        let error = store
            .save_query(&SavedQuery {
                id: Uuid::new_v4(),
                connection_id: None,
                name: "huge".into(),
                text: "x".repeat(MAX_QUERY_BYTES + 1),
                created_at: now,
                updated_at: now,
            })
            .unwrap_err()
            .to_string();
        assert!(error.contains("limited to"), "got {error}");
    }

    #[test]
    fn the_operation_history_is_bounded_and_newest_first() {
        let store = store();
        let connection = definition("App", Product::Postgresql);
        for index in 0..5 {
            store
                .record_operation(&OperationRecord {
                    id: Uuid::new_v4(),
                    connection_id: connection.id,
                    connection_name: connection.display_name.clone(),
                    kind: "query".into(),
                    summary: format!("statement {index}"),
                    outcome: OperationOutcome::Succeeded,
                    affected_records: Some(index),
                    elapsed_millis: 10,
                    error: None,
                    at: chrono::DateTime::from_timestamp(1_800_000_000 + index as i64, 0).unwrap(),
                })
                .unwrap();
        }
        let history = store.operations(10).unwrap();
        assert_eq!(history.len(), 5);
        assert_eq!(history[0].summary, "statement 4", "newest first");
        assert_eq!(history[0].outcome, OperationOutcome::Succeeded);
        assert_eq!(store.operations(2).unwrap().len(), 2);
    }

    #[test]
    fn an_abandoned_preview_is_recorded_too() {
        // It says somebody looked at dropping a table and thought better of it.
        let store = store();
        store
            .record_operation(&OperationRecord {
                id: Uuid::new_v4(),
                connection_id: Uuid::nil(),
                connection_name: "App".into(),
                kind: "mutation".into(),
                summary: "TRUNCATE orders".into(),
                outcome: OperationOutcome::Abandoned,
                affected_records: None,
                elapsed_millis: 0,
                error: None,
                at: chrono::Utc::now(),
            })
            .unwrap();
        assert_eq!(
            store.operations(1).unwrap()[0].outcome,
            OperationOutcome::Abandoned
        );
    }

    #[test]
    fn a_receipt_can_be_spent_exactly_once() {
        let mut store = store();
        let identifier = Uuid::new_v4();
        store
            .register(Receipt {
                identifier,
                effect_digest: "abc".into(),
                expires_at_millis: 9_999_999_999_999,
            })
            .unwrap();
        assert_eq!(
            store.consume(identifier).unwrap().map(|r| r.effect_digest),
            Some("abc".to_string())
        );
        assert!(store.consume(identifier).unwrap().is_none());
    }

    #[test]
    fn the_same_receipt_cannot_be_registered_twice() {
        let mut store = store();
        let receipt = Receipt {
            identifier: Uuid::new_v4(),
            effect_digest: "abc".into(),
            expires_at_millis: 1_000,
        };
        assert!(store.register(receipt.clone()).is_ok());
        assert!(store.register(receipt).is_err());
    }

    #[test]
    fn expired_receipts_are_purged() {
        let mut store = store();
        store
            .register(Receipt {
                identifier: Uuid::new_v4(),
                effect_digest: "old".into(),
                expires_at_millis: 1_000,
            })
            .unwrap();
        let live = Uuid::new_v4();
        store
            .register(Receipt {
                identifier: live,
                effect_digest: "new".into(),
                expires_at_millis: 9_999_999_999_999,
            })
            .unwrap();
        assert_eq!(store.purge_expired(2_000).unwrap(), 1);
        assert!(store.consume(live).unwrap().is_some());
    }

    #[test]
    fn a_file_from_a_newer_build_is_refused_rather_than_misread() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("db.sqlite");
        {
            let store = MetadataStore::open(&path).unwrap();
            store
                .connection
                .pragma_update(None, "user_version", super::SCHEMA_VERSION + 1)
                .unwrap();
        }
        let error = MetadataStore::open(&path).unwrap_err().to_string();
        assert!(error.contains("newer Veronica"), "got {error}");
    }

    #[test]
    fn a_store_survives_being_closed_and_reopened() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("db.sqlite");
        let connection = definition("App", Product::Postgresql);
        {
            MetadataStore::open(&path)
                .unwrap()
                .save_connection(&connection)
                .unwrap();
        }
        let reopened = MetadataStore::open(&path).unwrap();
        assert_eq!(reopened.connections().unwrap().len(), 1);
    }

    #[test]
    fn the_file_is_not_readable_by_anyone_else() {
        use std::os::unix::fs::PermissionsExt;
        // Which hosts somebody connects to is worth something on its own.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("db.sqlite");
        let store = MetadataStore::open(&path).unwrap();
        store
            .save_connection(&definition("App", Product::Postgresql))
            .unwrap();
        store.restrict_permissions().unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
