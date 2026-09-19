//! A read-only MCP server over stdio.
//!
//! A port of Edith's `ed database mcp`: the same two tools, the same read-only
//! annotations, and the same projection rule. An agent gets to know *which*
//! databases you have saved and what each can do; it does not get the host, the
//! port, the username, the TLS material, the tunnel, or any reference to a
//! secret.
//!
//! That projection is the whole design. A model asked to help with a query does
//! not need a connection string, and handing it one puts a credential's
//! neighbourhood into a transcript that goes somewhere else. `Projection` below
//! is what may leave this process; adding a field to it is a decision about
//! what an agent may learn.
//!
//! stdout carries protocol traffic and nothing else — no banner, no log line —
//! because anything else on it corrupts the stream. Diagnostics go to stderr.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};

use crate::capabilities::{Capability, Report};
use crate::connection::ConnectionDefinition;
use crate::store::MetadataStore;

/// The protocol version this server speaks.
pub const PROTOCOL_VERSION: &str = "2024-11-05";
/// At most this many connections in one reply, as Edith bounds it.
pub const MAX_CONNECTIONS: usize = 100;

/// What an agent may know about a connection.
///
/// Deliberately small. No endpoint, no username, no authentication, no TLS, no
/// tunnel, no options, no secret reference, no authentication source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Projection {
    pub id: String,
    pub display_name: String,
    pub product: String,
    pub family: String,
    pub environment: String,
    /// Whether a change is refused outright. Useful to an agent, and says
    /// nothing about where the database is.
    pub read_only: bool,
}

impl Projection {
    pub fn of(connection: &ConnectionDefinition) -> Self {
        Self {
            id: connection.id.to_string(),
            display_name: connection.display_name.clone(),
            product: connection.product_hint.key().to_string(),
            family: format!("{:?}", connection.product_hint.family()).to_lowercase(),
            environment: connection.environment.kind.key().to_string(),
            read_only: connection.mutation_prohibition().is_some(),
        }
    }
}

/// The tools this server offers. Both read; neither can change anything.
fn tools() -> Json {
    json!([
        {
            "name": "database_connections",
            "description":
                "List the databases Veronica has saved, or get one by id. Returns names, \
                 products and environments only — never a host, username, or credential.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "A connection id. Omit to list them all."
                    }
                },
                "additionalProperties": false
            },
            "annotations": {
                "title": "List database connections",
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false
            }
        },
        {
            "name": "database_capabilities",
            "description":
                "What one saved database can be asked to do — whether it has transactions, \
                 a schema to describe, sessions to list. Derived from the product; it does \
                 not connect.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The connection id, from database_connections."
                    }
                },
                "required": ["id"],
                "additionalProperties": false
            },
            "annotations": {
                "title": "Describe database capabilities",
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false
            }
        }
    ])
}

fn capability_rows(report: &Report) -> Json {
    json!(Capability::ALL
        .iter()
        .map(|capability| {
            let state = report.state(*capability);
            json!({
                "capability": capability.title(),
                "available": state.is_available(),
                "reason": match state {
                    crate::capabilities::State::Available => Json::Null,
                    crate::capabilities::State::Unsupported { reason }
                    | crate::capabilities::State::Unavailable { reason } => json!(reason),
                }
            })
        })
        .collect::<Vec<_>>())
}

/// Handle one tool call.
///
/// Returns the MCP content payload. Every failure comes back as an `isError`
/// result rather than a protocol error, because a model reads the text and a
/// transport error just looks like the tool is broken.
pub fn call_tool(store: &MetadataStore, name: &str, arguments: &Json) -> Json {
    let text = |value: Json, error: bool| {
        json!({
            "content": [{ "type": "text", "text": value.to_string() }],
            "isError": error
        })
    };

    match name {
        "database_connections" => {
            let connections = match store.connections() {
                Ok(connections) => connections,
                Err(error) => {
                    return text(json!({ "error": error.to_string() }), true);
                }
            };
            match arguments.get("id").and_then(Json::as_str) {
                Some(id) => match connections
                    .iter()
                    .find(|connection| connection.id.to_string() == id)
                {
                    Some(connection) => text(json!(Projection::of(connection)), false),
                    None => text(
                        json!({ "error": format!("no connection with id {id}") }),
                        true,
                    ),
                },
                None => {
                    let total = connections.len();
                    let projections: Vec<Projection> = connections
                        .iter()
                        .take(MAX_CONNECTIONS)
                        .map(Projection::of)
                        .collect();
                    text(
                        json!({
                            "connections": projections,
                            "total": total,
                            // Says so rather than silently truncating, which
                            // would let a model conclude a database is absent.
                            "complete": total <= MAX_CONNECTIONS
                        }),
                        false,
                    )
                }
            }
        }

        "database_capabilities" => {
            let Some(id) = arguments.get("id").and_then(Json::as_str) else {
                return text(json!({ "error": "an id is required" }), true);
            };
            let connection = match store.connections() {
                Ok(connections) => connections
                    .into_iter()
                    .find(|connection| connection.id.to_string() == id),
                Err(error) => return text(json!({ "error": error.to_string() }), true),
            };
            match connection {
                Some(connection) => {
                    let report = Report::for_product(connection.product_hint);
                    text(
                        json!({
                            "connection": Projection::of(&connection),
                            "hierarchy": report.object_hierarchy,
                            "capabilities": capability_rows(&report)
                        }),
                        false,
                    )
                }
                None => text(
                    json!({ "error": format!("no connection with id {id}") }),
                    true,
                ),
            }
        }

        other => text(json!({ "error": format!("no tool called {other}") }), true),
    }
}

/// Handle one JSON-RPC request. `None` for a notification, which takes no reply.
pub fn handle(store: &MetadataStore, request: &Json) -> Option<Json> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Json::as_str).unwrap_or("");

    // A notification has no id and must not be answered; replying to one is a
    // protocol violation that some clients treat as fatal.
    id.as_ref()?;

    let result = match method {
        "initialize" => json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "veronica-database", "version": env!("CARGO_PKG_VERSION") }
        }),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
            let name = params
                .get("name")
                .and_then(Json::as_str)
                .unwrap_or_default();
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            call_tool(store, name, &arguments)
        }
        "ping" => json!({}),
        other => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("no method called {other}") }
            }));
        }
    };

    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// Serve until stdin closes.
///
/// One JSON document per line, which is what every MCP stdio client sends.
pub fn serve(store: &MetadataStore) -> Result<()> {
    use std::io::{BufRead, Write};

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Json = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                // A parse error has no id to answer against, so it is reported
                // with a null id, as the spec requires.
                let reply = json!({
                    "jsonrpc": "2.0",
                    "id": Json::Null,
                    "error": { "code": -32700, "message": error.to_string() }
                });
                writeln!(stdout, "{reply}")?;
                stdout.flush()?;
                continue;
            }
        };
        if let Some(reply) = handle(store, &request) {
            writeln!(stdout, "{reply}")?;
            // Flushed per message: a client waiting on a reply that is sitting
            // in a buffer looks like a hung server.
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::*;
    use crate::product::Product;
    use uuid::Uuid;

    fn store_with(connection: ConnectionDefinition) -> MetadataStore {
        let store = MetadataStore::in_memory().unwrap();
        store.save_connection(&connection).unwrap();
        store
    }

    fn definition() -> ConnectionDefinition {
        let now = chrono::Utc::now();
        ConnectionDefinition {
            version: crate::connection::SCHEMA_VERSION,
            id: Uuid::from_u128(42),
            display_name: "Production".into(),
            product_hint: Product::Postgresql,
            location: Location::Network {
                endpoints: vec![NetworkEndpoint {
                    host: "db.internal.example.com".into(),
                    port: Port::new(5432).unwrap(),
                    role: EndpointRole::Primary,
                }],
            },
            username: Some("app_writer".into()),
            namespaces: NamespaceDefaults::default(),
            deployment_mode: DeploymentMode::Automatic,
            authentication: Authentication {
                kind: AuthenticationKind::UsernameAndPassword,
                secret_references: vec![SecretReference {
                    identifier: Uuid::from_u128(7),
                    purpose: SecretPurpose::Password,
                }],
                source: Some("VERONICA_PGPASSWORD".into()),
            },
            tls: TlsConfiguration::default(),
            tunnel: Some(TunnelDefinition {
                machine_identifier: "bastion".into(),
                remote_endpoint: NetworkEndpoint {
                    host: "10.0.0.5".into(),
                    port: Port::new(5432).unwrap(),
                    role: EndpointRole::Primary,
                },
                local_bind_address: "127.0.0.1".into(),
                requested_local_port: None,
                manages_lifecycle: true,
            }),
            limits: ConnectionLimits::default(),
            read_only_policy: ReadOnlyPolicy::Disabled,
            production_policy: ProductionPolicy::Standard,
            environment: EnvironmentMetadata {
                kind: EnvironmentKind::Production,
                label: "production".into(),
                protection: EnvironmentProtection::ConfirmationRequired,
            },
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

    fn call(store: &MetadataStore, name: &str, arguments: Json) -> String {
        let reply = call_tool(store, name, &arguments);
        reply["content"][0]["text"].as_str().unwrap().to_string()
    }

    /// The rule the whole module exists for.
    #[test]
    fn a_projection_tells_an_agent_nothing_it_could_connect_with() {
        let store = store_with(definition());
        let text = call(&store, "database_connections", json!({}));
        for secret in [
            "db.internal.example.com",
            "5432",
            "app_writer",
            "VERONICA_PGPASSWORD",
            "bastion",
            "10.0.0.5",
            &Uuid::from_u128(7).to_string(),
        ] {
            assert!(
                !text.contains(secret),
                "the projection leaked {secret:?}: {text}"
            );
        }
        // What it does say is which database it is and what it is for.
        assert!(text.contains("Production"));
        assert!(text.contains("postgresql"));
        assert!(text.contains("production"));
    }

    #[test]
    fn one_connection_can_be_fetched_by_id_and_is_projected_the_same_way() {
        let store = store_with(definition());
        let text = call(
            &store,
            "database_connections",
            json!({ "id": Uuid::from_u128(42).to_string() }),
        );
        assert!(text.contains("Production"));
        assert!(!text.contains("app_writer"));
    }

    #[test]
    fn an_unknown_id_is_an_error_result_rather_than_a_transport_failure() {
        // A model reads the text; a transport error just looks broken.
        let store = store_with(definition());
        let reply = call_tool(
            &store,
            "database_connections",
            &json!({ "id": "00000000-0000-0000-0000-000000000000" }),
        );
        assert_eq!(reply["isError"], json!(true));
        assert!(reply["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("no connection"));
    }

    #[test]
    fn a_listing_says_whether_it_is_complete() {
        // Silently truncating would let a model conclude a database is absent.
        let store = MetadataStore::in_memory().unwrap();
        for index in 0..3 {
            let mut connection = definition();
            connection.id = Uuid::from_u128(index);
            connection.display_name = format!("db-{index}");
            store.save_connection(&connection).unwrap();
        }
        let text = call(&store, "database_connections", json!({}));
        let parsed: Json = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["total"], json!(3));
        assert_eq!(parsed["complete"], json!(true));
        assert_eq!(parsed["connections"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn capabilities_come_back_with_a_reason_for_each_no() {
        let mut connection = definition();
        connection.product_hint = Product::Redis;
        let store = store_with(connection);
        let text = call(
            &store,
            "database_capabilities",
            json!({ "id": Uuid::from_u128(42).to_string() }),
        );
        let parsed: Json = serde_json::from_str(&text).unwrap();
        let rows = parsed["capabilities"].as_array().unwrap();
        assert_eq!(rows.len(), Capability::ALL.len());
        for row in rows {
            if row["available"] == json!(false) {
                assert!(
                    row["reason"]
                        .as_str()
                        .is_some_and(|reason| !reason.is_empty()),
                    "{row} gives no reason"
                );
            }
        }
        // And it still leaks nothing.
        assert!(!text.contains("app_writer"));
    }

    #[test]
    fn capabilities_without_an_id_is_an_error_not_a_panic() {
        let store = store_with(definition());
        let reply = call_tool(&store, "database_capabilities", &json!({}));
        assert_eq!(reply["isError"], json!(true));
    }

    #[test]
    fn an_unknown_tool_is_refused_by_name() {
        let store = store_with(definition());
        let reply = call_tool(&store, "database_drop_everything", &json!({}));
        assert_eq!(reply["isError"], json!(true));
        assert!(reply["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("no tool called"));
    }

    #[test]
    fn both_tools_declare_themselves_read_only_and_closed_world() {
        let tools = tools();
        let list = tools.as_array().unwrap();
        assert_eq!(list.len(), 2, "only these two tools exist");
        for tool in list {
            let annotations = &tool["annotations"];
            assert_eq!(annotations["readOnlyHint"], json!(true), "{tool}");
            assert_eq!(annotations["destructiveHint"], json!(false), "{tool}");
            assert_eq!(annotations["idempotentHint"], json!(true), "{tool}");
            assert_eq!(annotations["openWorldHint"], json!(false), "{tool}");
        }
    }

    #[test]
    fn there_is_no_tool_that_can_change_anything() {
        // The server is read-only by construction, not by convention.
        let tools = tools();
        let names: Vec<&str> = tools
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["database_connections", "database_capabilities"]);
        for name in ["query", "read", "browse", "mutate", "apply", "execute"] {
            assert!(
                !names.iter().any(|tool| tool.contains(name)),
                "a {name} tool would let an agent reach a database"
            );
        }
    }

    #[test]
    fn initialize_reports_the_protocol_and_the_server() {
        let store = store_with(definition());
        let reply = handle(
            &store,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }),
        )
        .unwrap();
        assert_eq!(reply["result"]["protocolVersion"], json!(PROTOCOL_VERSION));
        assert_eq!(
            reply["result"]["serverInfo"]["name"],
            json!("veronica-database")
        );
        assert_eq!(reply["id"], json!(1));
    }

    #[test]
    fn a_notification_is_not_answered() {
        // Replying to one is a protocol violation some clients treat as fatal.
        let store = store_with(definition());
        assert!(handle(
            &store,
            &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
        )
        .is_none());
    }

    #[test]
    fn an_unknown_method_gets_a_json_rpc_error_with_the_right_code() {
        let store = store_with(definition());
        let reply = handle(
            &store,
            &json!({ "jsonrpc": "2.0", "id": 9, "method": "resources/list" }),
        )
        .unwrap();
        assert_eq!(reply["error"]["code"], json!(-32601));
        assert_eq!(reply["id"], json!(9));
    }

    #[test]
    fn a_string_id_comes_back_unchanged() {
        // Coercing it to a number would lose the client's correlation.
        let store = store_with(definition());
        let reply = handle(
            &store,
            &json!({ "jsonrpc": "2.0", "id": "abc", "method": "ping" }),
        )
        .unwrap();
        assert_eq!(reply["id"], json!("abc"));
    }
}
