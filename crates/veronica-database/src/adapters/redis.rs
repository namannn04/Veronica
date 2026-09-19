//! Redis and Valkey.
//!
//! The awkward one, and the reason `Family` exists. A key-value store has no
//! schema, no tables and no rows, so "browse" means scanning the keyspace and
//! "read" means fetching one key's value in whatever shape it happens to have.
//!
//! Two decisions worth stating:
//!
//! - **`SCAN`, never `KEYS`.** `KEYS *` blocks the server for the length of the
//!   scan, which on a production instance is an outage. `SCAN` is incremental
//!   and is what a tool pointed at somebody's live cache must use.
//! - **Only the commands Veronica understands.** An arbitrary command string is
//!   not accepted; a mutation names a command from a known list, and the guard
//!   has already signed which one.

use anyhow::{Context, Result};
use redis::AsyncCommands;

use crate::connection::{ConnectionDefinition, ReadOnlyPolicy, SecretPurpose};
use crate::identify::{ObjectIdentifier, ObjectKind};
use crate::mutation::Payload;
use crate::paging::{ColumnDescriptor, Page, PageRequest};
use crate::product::{Product, ProductIdentity, Topology, TopologyKind, Version};
use crate::secrets::SecretStore;
use crate::value::{ObjectField, Value};

use super::Adapter;

/// The commands a mutation may name. Anything else is refused before it is
/// sent, so a payload cannot smuggle `FLUSHALL` past the guard by spelling it
/// into a command string.
pub const ALLOWED_COMMANDS: &[&str] = &[
    "SET", "SETEX", "GETSET", "DEL", "UNLINK", "EXPIRE", "PERSIST", "RENAME", "INCR", "DECR",
    "INCRBY", "DECRBY", "APPEND", "HSET", "HDEL", "LPUSH", "RPUSH", "LPOP", "RPOP", "SADD", "SREM",
    "ZADD", "ZREM",
];

pub struct RedisAdapter {
    connection: redis::aio::MultiplexedConnection,
    read_only: bool,
    product: Product,
}

impl RedisAdapter {
    pub async fn connect(definition: &ConnectionDefinition, secrets: &SecretStore) -> Result<Self> {
        let endpoint = definition
            .location
            .primary_endpoint()
            .context("this connection names no endpoint")?;

        // The URL is how this client's public API takes credentials; its
        // struct fields are private. The password is built into the string at
        // connect time and dropped with it — it is never logged, and the URL
        // never leaves this function.
        let mut authority = String::new();
        if let Some(username) = &definition.username {
            authority.push_str(&percent_encode(username));
        }
        if let Some(reference) = definition.authentication.reference(SecretPurpose::Password) {
            if let Some(password) = secrets
                .load(reference.identifier, reference.purpose)
                .await?
            {
                authority.push(':');
                authority.push_str(&percent_encode(&String::from_utf8_lossy(&password)));
            }
        }
        if !authority.is_empty() {
            authority.push('@');
        }
        let database: u32 = definition
            .namespaces
            .logical_database
            .as_deref()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let url = format!(
            "redis://{authority}{}:{}/{database}",
            endpoint.host,
            endpoint.port.get()
        );

        let client = redis::Client::open(url).context("that is not a Redis address")?;
        let connection = client
            .get_multiplexed_async_connection()
            .await
            .with_context(|| format!("cannot reach {}:{}", endpoint.host, endpoint.port.get()))?;

        Ok(Self {
            connection,
            read_only: definition.read_only_policy == ReadOnlyPolicy::Required,
            product: definition.product_hint,
        })
    }

    /// One page of keys, by `SCAN` rather than `KEYS`.
    ///
    /// `SCAN` gives no total and no stable offset, so paging over it is done by
    /// scanning from the start each time and skipping. That is honest about
    /// what the server can do; pretending to have an offset would produce a
    /// page that silently repeats or skips keys.
    async fn scan_keys(
        &mut self,
        pattern: &str,
        skip: u64,
        take: usize,
    ) -> Result<(Vec<String>, bool)> {
        let mut cursor: u64 = 0;
        let mut seen: u64 = 0;
        let mut keys = Vec::new();
        loop {
            let (next, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(500)
                .query_async(&mut self.connection)
                .await?;
            for key in batch {
                if seen >= skip {
                    keys.push(key);
                    if keys.len() > take {
                        return Ok((keys, true));
                    }
                }
                seen += 1;
            }
            cursor = next;
            if cursor == 0 {
                return Ok((keys, false));
            }
        }
    }
}

/// Percent-encode a credential for a URL.
///
/// A password containing `@` or `/` would otherwise be read as part of the
/// host, connecting to somewhere unintended with a truncated credential.
fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Read one key, whatever type it is.
async fn read_key(
    connection: &mut redis::aio::MultiplexedConnection,
    key: &str,
) -> Result<(String, Value)> {
    let kind: String = redis::cmd("TYPE").arg(key).query_async(connection).await?;
    let value = match kind.as_str() {
        "string" => {
            let value: Option<String> = connection.get(key).await?;
            value.map(Value::String).unwrap_or(Value::Null)
        }
        "list" => {
            let items: Vec<String> = connection.lrange(key, 0, 99).await?;
            Value::Array(items.into_iter().map(Value::String).collect())
        }
        "set" => {
            let items: Vec<String> = connection.smembers(key).await?;
            Value::Array(items.into_iter().map(Value::String).collect())
        }
        "zset" => {
            let items: Vec<String> = connection.zrange(key, 0, 99).await?;
            Value::Array(items.into_iter().map(Value::String).collect())
        }
        "hash" => {
            let map: std::collections::HashMap<String, String> = connection.hgetall(key).await?;
            let mut fields: Vec<ObjectField> = map
                .into_iter()
                .map(|(name, value)| ObjectField {
                    name,
                    value: Value::String(value),
                })
                .collect();
            // A stable order, so a grid does not reshuffle between refreshes.
            fields.sort_by(|left, right| left.name.cmp(&right.name));
            Value::Object(fields)
        }
        // A key that expired between the scan and the read is gone, not an
        // error: that is ordinary in a cache.
        "none" => Value::Missing,
        other => Value::ProductSpecific(crate::value::ProductValue {
            type_name: other.to_string(),
            rendered: format!("<{other}>"),
        }),
    };
    Ok((kind, value))
}

/// Parse the `INFO` reply, which is `field:value` lines in `# Section` blocks.
pub fn parse_info(text: &str) -> std::collections::HashMap<String, String> {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect()
}

#[async_trait::async_trait]
impl Adapter for RedisAdapter {
    async fn identify(&mut self) -> Result<ProductIdentity> {
        let text: String = redis::cmd("INFO")
            .query_async(&mut self.connection)
            .await
            .context("the server refused INFO")?;
        let info = parse_info(&text);

        // Valkey reports its own version field; without it this is Redis.
        let (product, version) = match info.get("valkey_version") {
            Some(version) => (Product::Valkey, version.clone()),
            None => (
                self.product,
                info.get("redis_version").cloned().unwrap_or_default(),
            ),
        };
        let role = info.get("role").cloned().unwrap_or_default();
        let cluster_enabled = info.get("cluster_enabled").map(String::as_str) == Some("1");

        Ok(ProductIdentity {
            version: Some(Version::parse(&version)),
            distribution: info.get("redis_mode").cloned(),
            topology: Topology {
                kind: if cluster_enabled {
                    TopologyKind::Cluster
                } else if role == "slave" || role == "replica" {
                    TopologyKind::PrimaryReplica
                } else {
                    TopologyKind::Standalone
                },
                local_role: (!role.is_empty()).then_some(role),
                replica_count: info
                    .get("connected_slaves")
                    .and_then(|count| count.parse().ok()),
                ..Topology::default()
            },
            server_identifier: info.get("run_id").cloned(),
            ..ProductIdentity::new(product)
        })
    }

    async fn objects(
        &mut self,
        parent: Option<&ObjectIdentifier>,
    ) -> Result<Vec<ObjectIdentifier>> {
        // A key-value store's only hierarchy is the keyspace, so the top level
        // is a sample of keys and there is nothing to descend into.
        let pattern = parent
            .map(|object| format!("{}*", object.name()))
            .unwrap_or_else(|| "*".to_string());
        let (keys, _) = self.scan_keys(&pattern, 0, 200).await?;
        Ok(keys
            .into_iter()
            .map(|key| ObjectIdentifier::new(ObjectKind::Key, vec![key]))
            .collect())
    }

    async fn read(&mut self, object: &ObjectIdentifier, request: &PageRequest) -> Result<Page> {
        let started = std::time::Instant::now();
        let (kind, value) = read_key(&mut self.connection, object.name()).await?;
        let ttl: i64 = redis::cmd("TTL")
            .arg(object.name())
            .query_async(&mut self.connection)
            .await
            .unwrap_or(-1);
        Ok(Page::from_rows(
            vec![
                ColumnDescriptor {
                    name: "key".into(),
                    type_name: None,
                },
                ColumnDescriptor {
                    name: "type".into(),
                    type_name: None,
                },
                ColumnDescriptor {
                    name: "value".into(),
                    type_name: None,
                },
                ColumnDescriptor {
                    name: "ttl".into(),
                    type_name: Some("seconds".into()),
                },
            ],
            vec![vec![
                Value::String(object.name().to_string()),
                Value::String(kind),
                value,
                // -1 is "no expiry" and -2 is "gone"; both are facts, not
                // negative numbers of seconds.
                match ttl {
                    -1 => Value::Null,
                    -2 => Value::Missing,
                    seconds => Value::SignedInteger(seconds),
                },
            ]],
            request,
            started.elapsed().as_millis() as u64,
        ))
    }

    /// A "query" against Redis is a key pattern, which is the only read that
    /// makes sense here. `SCAN`, so it never blocks the server.
    async fn query(&mut self, statement: &str, request: &PageRequest) -> Result<Page> {
        let started = std::time::Instant::now();
        let pattern = statement.trim();
        if pattern.is_empty() {
            anyhow::bail!("give a key pattern, such as `session:*`");
        }
        let (keys, _) = self
            .scan_keys(pattern, request.offset, request.page_size.get() as usize)
            .await?;

        let mut rows = Vec::new();
        for key in keys {
            let (kind, value) = read_key(&mut self.connection, &key).await?;
            rows.push(vec![Value::String(key), Value::String(kind), value]);
        }
        Ok(Page::from_rows(
            vec![
                ColumnDescriptor {
                    name: "key".into(),
                    type_name: None,
                },
                ColumnDescriptor {
                    name: "type".into(),
                    type_name: None,
                },
                ColumnDescriptor {
                    name: "value".into(),
                    type_name: None,
                },
            ],
            rows,
            request,
            started.elapsed().as_millis() as u64,
        ))
    }

    /// Counting the keys a pattern matches means scanning the whole keyspace,
    /// which is exactly what this adapter avoids. The capability report already
    /// says so; this returns the honest answer rather than a slow one.
    async fn count(&mut self, _object: &ObjectIdentifier) -> Result<Option<u64>> {
        Ok(None)
    }

    async fn execute(&mut self, payload: &Payload) -> Result<u64> {
        let Payload::Keyspace {
            command, arguments, ..
        } = payload
        else {
            anyhow::bail!("Redis takes a keyspace payload, not a {:?}", payload.kind());
        };
        if self.read_only {
            anyhow::bail!("this connection is read-only");
        }
        let upper = command.trim().to_ascii_uppercase();
        if !ALLOWED_COMMANDS.contains(&upper.as_str()) {
            anyhow::bail!(
                "Veronica does not send `{upper}`. It sends one of: {}",
                ALLOWED_COMMANDS.join(", ")
            );
        }

        let mut request = redis::cmd(&upper);
        for argument in arguments {
            request.arg(argument.value.render(usize::MAX));
        }
        // The reply shape differs per command — an integer, `OK`, a string —
        // so it is read loosely and turned into a count of what changed.
        let reply: redis::Value = request.query_async(&mut self.connection).await?;
        Ok(match reply {
            redis::Value::Int(count) => count.max(0) as u64,
            redis::Value::Nil => 0,
            // `OK` and anything else means the one key was acted on.
            _ => 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_list_holds_no_way_to_wipe_the_server() {
        // A payload must not be able to smuggle one past the guard.
        for forbidden in [
            "FLUSHALL",
            "FLUSHDB",
            "SHUTDOWN",
            "CONFIG",
            "SCRIPT",
            "EVAL",
            "DEBUG",
            "MIGRATE",
            "REPLICAOF",
            "SLAVEOF",
            "CLUSTER",
            "ACL",
        ] {
            assert!(
                !ALLOWED_COMMANDS.contains(&forbidden),
                "{forbidden} is in the allowed list"
            );
        }
    }

    #[test]
    fn the_command_list_is_upper_case_so_matching_is_exact() {
        for command in ALLOWED_COMMANDS {
            assert_eq!(*command, command.to_ascii_uppercase());
        }
    }

    #[test]
    fn the_info_reply_parses_into_its_fields() {
        let text = "# Server\r\nredis_version:7.2.4\r\nrun_id:abc123\r\n\r\n# Replication\r\nrole:master\r\nconnected_slaves:2\r\n";
        let info = parse_info(text);
        assert_eq!(info.get("redis_version").map(String::as_str), Some("7.2.4"));
        assert_eq!(info.get("role").map(String::as_str), Some("master"));
        assert_eq!(info.get("connected_slaves").map(String::as_str), Some("2"));
        // The section headers are not fields.
        assert!(!info.contains_key("# Server"));
    }

    #[test]
    fn a_field_whose_value_contains_a_colon_survives() {
        // `executable:/usr/bin/redis-server` is one field, not a truncated one.
        let info = parse_info("executable:/usr/bin/redis-server\n");
        assert_eq!(
            info.get("executable").map(String::as_str),
            Some("/usr/bin/redis-server")
        );
    }

    #[test]
    fn an_empty_info_reply_yields_no_fields_rather_than_panicking() {
        assert!(parse_info("").is_empty());
        assert!(parse_info("# Only a header\n").is_empty());
    }

    #[test]
    fn a_credential_with_url_syntax_in_it_is_encoded() {
        // An unencoded `@` would make the rest of the password look like a host.
        assert_eq!(percent_encode("p@ss/word"), "p%40ss%2Fword");
        assert_eq!(percent_encode("plain-123_x.y~z"), "plain-123_x.y~z");
        assert_eq!(percent_encode(":"), "%3A");
        assert_eq!(percent_encode(""), "");
    }
}
