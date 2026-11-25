use std::fmt::Write as _;

use anyhow::{Context, Result};
use serde_json::json;
use uuid::Uuid;
use veronica_core::AppDirectories;
use veronica_database::connection::{
    Authentication, AuthenticationKind, ConnectionDefinition, ConnectionLimits, DeploymentMode,
    EndpointRole, EnvironmentKind, EnvironmentMetadata, EnvironmentProtection, Location,
    NamespaceDefaults, NetworkEndpoint, Port, ProductionPolicy, ReadOnlyPolicy, SecretPurpose,
    SecretReference, SqliteAccessMode, SqliteLocation, TlsConfiguration, SCHEMA_VERSION,
};
use veronica_database::identify::{ObjectIdentifier, TargetIdentifier};
use veronica_database::mutation::{
    Action, ContextKind, ExecutionMode, Impact, MutationContext, Parameter, Payload, Plan,
    RollbackAvailability, Scope, TransactionBehavior,
};
use veronica_database::paging::{PageRequest, PageSize};
use veronica_database::product::Product;
use veronica_database::secrets::SecretStore;
use veronica_database::session::Session;
use veronica_database::store::{MetadataStore, SavedQuery};
use veronica_database::value::Value;

use veronica_sdk::format;
use veronica_sdk::Output;

#[derive(clap::Subcommand)]
pub(crate) enum DatabaseCommand {
    #[command(subcommand, alias = "conn", about = "Saved connections")]
    Connections(ConnectionCommand),
    #[command(about = "What this server can be asked to do")]
    Capabilities { connection: String },
    #[command(about = "Reach the server and report what it turned out to be. Reads only")]
    Test { connection: String },
    #[command(
        alias = "ls",
        about = "List what is inside a database: schemas, tables, columns, keys"
    )]
    Browse {
        connection: String,
        #[arg(help = "A dotted path, e.g. `public` or `public.orders`. Omit for the top")]
        path: Option<String>,
    },
    #[command(about = "Read a bounded page of one object's records")]
    Read {
        connection: String,
        #[arg(help = "A dotted path to a table, collection or key")]
        path: String,
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
    #[command(about = "Run one statement that reads. A write is refused")]
    Query {
        connection: String,
        #[arg(help = "The statement. For Redis, a key pattern")]
        statement: String,
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
    #[command(subcommand, name = "saved-queries", about = "Queries you kept")]
    SavedQueries(SavedQueryCommand),
    #[command(subcommand, about = "Change data, previewed first")]
    Mutations(MutationCommand),
    #[command(about = "What Veronica has done, newest first")]
    Operations {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

#[derive(clap::Subcommand)]
pub(crate) enum ConnectionCommand {
    #[command(
        alias = "ls",
        about = "Every saved connection. No credential is printed"
    )]
    List,
    #[command(about = "One connection in full")]
    Get { connection: String },
    #[command(
        about = "Save a connection",
        long_about = "Save a connection.\n\nThe password is read from stdin, never from an argument, so it does not land in your shell history or in the process list."
    )]
    Add {
        #[arg(help = "What to call it")]
        name: String,
        #[arg(help = "postgresql, mysql, mariadb, sqlite, redis, valkey")]
        product: String,
        #[arg(help = "`host:port`, or a path for SQLite")]
        target: String,
        #[arg(long)]
        username: Option<String>,
        #[arg(long, help = "The database, schema or logical database to default to")]
        database: Option<String>,
        #[arg(
            long,
            default_value = "local",
            help = "local, development, testing, staging, production, other"
        )]
        environment: String,
        #[arg(long, help = "Refuse every change on this connection")]
        read_only: bool,
        #[arg(long, help = "Read a password from stdin")]
        password_stdin: bool,
    },
    #[command(about = "Change a connection's safety policy")]
    Policy {
        connection: String,
        #[arg(long, help = "Refuse every change on this connection")]
        read_only: Option<bool>,
        #[arg(long, help = "local, development, testing, staging, production, other")]
        environment: Option<String>,
    },
    #[command(
        alias = "rm",
        about = "Forget a connection, its saved queries and its password"
    )]
    Remove { connection: String },
}

#[derive(clap::Subcommand)]
pub(crate) enum SavedQueryCommand {
    #[command(alias = "ls")]
    List,
    #[command(about = "Keep a query. The text is read from stdin when not given")]
    Save {
        name: String,
        text: Option<String>,
        #[arg(long)]
        connection: Option<String>,
    },
    #[command(about = "Print one saved query's text")]
    Get { id: String },
    #[command(alias = "rm")]
    Remove { id: String },
}

#[derive(clap::Subcommand)]
pub(crate) enum MutationCommand {
    #[command(
        about = "Describe a change without making it, and issue a token",
        long_about = "Describe a change without making it, and issue a token.\n\nPrints the plan, what it would affect, the warnings it earns, and the exact text you have to type back. Nothing is sent to the server."
    )]
    Preview {
        connection: String,
        #[arg(
            long,
            help = "The statement, with `?1`, `$1` or `?` placeholders for every value"
        )]
        sql: Option<String>,
        #[arg(long, help = "A Redis command from the allowed list")]
        command: Option<String>,
        #[arg(long = "param", help = "One value, as JSON. Repeat, in order")]
        params: Vec<String>,
        #[arg(
            long,
            default_value = "update",
            help = "insert, update, updateMany, delete, deleteMany, truncate, dropObject"
        )]
        action: String,
        #[arg(long, help = "The object this touches, e.g. `public.orders`")]
        target: String,
        #[arg(
            long,
            default_value = "singleRecord",
            help = "singleRecord, selectedRecords, predicate, entireObject"
        )]
        scope: String,
        #[arg(long, default_value_t = 120, help = "How long the token is good for")]
        lifetime: i64,
        #[arg(long, help = "Write the plan here so `apply` can read it back")]
        save_plan: Option<String>,
    },
    #[command(about = "Make a previewed change")]
    Apply {
        connection: String,
        #[arg(long, help = "The plan `preview --save-plan` wrote. `-` reads stdin")]
        plan: String,
        #[arg(long, help = "The token from the preview")]
        token: String,
        #[arg(long, help = "Exactly the text the preview asked for")]
        confirm: String,
    },
}

fn directories_store(directories: &AppDirectories) -> Result<MetadataStore> {
    MetadataStore::open(directories.database_store())
}

fn secret_store(directories: &AppDirectories) -> SecretStore {
    SecretStore::new(directories.database_secrets_fallback())
}

fn path_components(path: &str) -> Vec<String> {
    path.split('.')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn object_from_path(product: Product, path: &str) -> ObjectIdentifier {
    ObjectIdentifier::for_path(product, path_components(path))
}

fn render_page(page: &veronica_database::paging::Page) -> String {
    if page.rows.is_empty() {
        return "no rows".to_string();
    }
    let headers: Vec<&str> = page.columns.iter().map(|c| c.name.as_str()).collect();
    let rows: Vec<Vec<String>> = page
        .rows
        .iter()
        .map(|row| row.iter().map(|value| value.render(60)).collect())
        .collect();
    format!(
        "{}\n\n{} row{} in {} ms{}",
        format::table(&headers, &rows),
        page.rows.len(),
        if page.rows.len() == 1 { "" } else { "s" },
        page.elapsed_millis,
        if page.has_more {
            format!(", more from --offset {}", page.next_offset.unwrap_or(0))
        } else {
            String::new()
        }
    )
}

fn read_stdin(stdin: Option<&str>) -> Result<String> {
    let buffer = stdin.context("cannot read from stdin")?;
    Ok(buffer.trim_end_matches(['\n', '\r']).to_string())
}

pub(crate) async fn run(
    directories: &AppDirectories,
    command: &DatabaseCommand,
    stdin: Option<&str>,
    output: Output<'_>,
) -> Result<()> {
    match command {
        DatabaseCommand::Connections(command) => {
            connections(directories, command, stdin, output).await
        }
        DatabaseCommand::SavedQueries(command) => {
            saved_queries(directories, command, stdin, output)
        }
        DatabaseCommand::Mutations(command) => mutations(directories, command, stdin, output).await,

        DatabaseCommand::Capabilities { connection } => {
            let store = directories_store(directories)?;
            let definition = store.resolve(connection)?;
            let mut session = Session::open(&definition, &secret_store(directories)).await?;
            let report = session.capabilities().await?;
            output.emit(&report, || {
                let rows: Vec<Vec<String>> =
                    veronica_database::capabilities::Capability::ALL
                        .iter()
                        .map(|capability| {
                            let state = report.state(*capability);
                            vec![
                                capability.title().to_string(),
                                if state.is_available() { "yes" } else { "no" }.to_string(),
                                match state {
                                    veronica_database::capabilities::State::Available => {
                                        String::new()
                                    }
                                    veronica_database::capabilities::State::Unsupported {
                                        reason,
                                    }
                                    | veronica_database::capabilities::State::Unavailable {
                                        reason,
                                    } => reason.clone(),
                                },
                            ]
                        })
                        .collect();
                format!(
                    "{} · {}\n\n{}",
                    report.product.title(),
                    report.object_hierarchy.join(" › "),
                    format::table(&["capability", "", "why not"], &rows)
                )
            })
        }

        DatabaseCommand::Test { connection } => {
            let store = directories_store(directories)?;
            let definition = store.resolve(connection)?;
            let started = std::time::Instant::now();
            let mut session = Session::open(&definition, &secret_store(directories)).await?;
            let identity = session.identify().await?;
            let elapsed = started.elapsed().as_millis();
            output.emit(
                &json!({ "identity": identity, "connectedInMillis": elapsed }),
                || {
                    let mut out = format!("{} answered in {elapsed} ms\n", definition.display_name);
                    let _ = writeln!(out, "Product   {}", identity.product.title());
                    if let Some(version) = &identity.version {
                        let _ = writeln!(out, "Version   {}", version.string);
                    }
                    let _ = writeln!(out, "Topology  {:?}", identity.topology.kind);
                    if let Some(role) = &identity.topology.local_role {
                        let _ = writeln!(out, "Role      {role}");
                    }
                    if let Some(server) = &identity.server_identifier {
                        let _ = write!(out, "Server    {server}");
                    }
                    out.trim_end().to_string()
                },
            )
        }

        DatabaseCommand::Browse { connection, path } => {
            let store = directories_store(directories)?;
            let definition = store.resolve(connection)?;
            let mut session = Session::open(&definition, &secret_store(directories)).await?;
            let parent = path
                .as_deref()
                .map(|path| object_from_path(definition.product_hint, path));
            let objects = session.objects(parent.as_ref()).await?;
            output.emit(&objects, || {
                if objects.is_empty() {
                    return "nothing here".to_string();
                }
                let rows: Vec<Vec<String>> = objects
                    .iter()
                    .map(|object| {
                        vec![
                            object.kind.key().to_string(),
                            object.display_path(),
                            object.native_identifier.clone().unwrap_or_default(),
                        ]
                    })
                    .collect();
                format::table(&["kind", "path", "type"], &rows)
            })
        }

        DatabaseCommand::Read {
            connection,
            path,
            limit,
            offset,
        } => {
            let store = directories_store(directories)?;
            let definition = store.resolve(connection)?;
            let mut session = Session::open(&definition, &secret_store(directories)).await?;
            let object = object_from_path(definition.product_hint, path);
            let page = session
                .read(
                    &object,
                    &PageRequest {
                        page_size: PageSize::clamped(*limit),
                        offset: *offset,
                        ..PageRequest::default()
                    },
                )
                .await?;
            output.emit(&page, || render_page(&page))
        }

        DatabaseCommand::Query {
            connection,
            statement,
            limit,
            offset,
        } => {
            let store = directories_store(directories)?;
            let definition = store.resolve(connection)?;
            let mut session = Session::open(&definition, &secret_store(directories)).await?;
            let page = session
                .query(
                    statement,
                    &PageRequest {
                        page_size: PageSize::clamped(*limit),
                        offset: *offset,
                        ..PageRequest::default()
                    },
                )
                .await?;
            output.emit(&page, || render_page(&page))
        }

        DatabaseCommand::Operations { limit } => {
            let store = directories_store(directories)?;
            let history = store.operations(*limit)?;
            output.emit(&history, || {
                if history.is_empty() {
                    return "nothing yet".to_string();
                }
                let rows: Vec<Vec<String>> = history
                    .iter()
                    .map(|record| {
                        vec![
                            record.at.format("%Y-%m-%d %H:%M").to_string(),
                            record.connection_name.clone(),
                            record.kind.clone(),
                            format!("{:?}", record.outcome).to_lowercase(),
                            record
                                .affected_records
                                .map(|count| count.to_string())
                                .unwrap_or_default(),
                            record.summary.clone(),
                        ]
                    })
                    .collect();
                format::table(
                    &["when", "connection", "kind", "outcome", "rows", "what"],
                    &rows,
                )
            })
        }
    }
}

async fn connections(
    directories: &AppDirectories,
    command: &ConnectionCommand,
    stdin: Option<&str>,
    output: Output<'_>,
) -> Result<()> {
    let store = directories_store(directories)?;

    match command {
        ConnectionCommand::List => {
            let connections = store.connections()?;
            output.emit(&connections, || {
                if connections.is_empty() {
                    return "no saved connections; add one with `vr database connections add`"
                        .to_string();
                }
                let rows: Vec<Vec<String>> = connections
                    .iter()
                    .map(|connection| {
                        vec![
                            connection.display_name.clone(),
                            connection.product_hint.title().to_string(),
                            connection.environment.kind.key().to_string(),
                            connection.location.summary(),
                            match connection.mutation_prohibition() {
                                Some(_) => "read-only".to_string(),
                                None => String::new(),
                            },
                        ]
                    })
                    .collect();
                format::table(&["name", "product", "environment", "where", ""], &rows)
            })
        }

        ConnectionCommand::Get { connection } => {
            let definition = store.resolve(connection)?;
            output.emit(&definition, || {
                let mut out = format!("{}\n", definition.display_name);
                let _ = writeln!(out, "id           {}", definition.id);
                let _ = writeln!(out, "product      {}", definition.product_hint.title());
                let _ = writeln!(out, "where        {}", definition.location.summary());
                let _ = writeln!(
                    out,
                    "username     {}",
                    definition.username.as_deref().unwrap_or("-")
                );
                let _ = writeln!(
                    out,
                    "credential   {}",
                    if definition.authentication.is_configured() {
                        "configured, in the keyring"
                    } else {
                        "none"
                    }
                );
                let _ = writeln!(
                    out,
                    "environment  {} ({:?})",
                    definition.environment.label, definition.environment.protection
                );
                let _ = write!(
                    out,
                    "changes      {}",
                    match definition.mutation_prohibition() {
                        Some(prohibition) => format!("refused: {}", prohibition.reason()),
                        None => "allowed, previewed first".to_string(),
                    }
                );
                out
            })
        }

        ConnectionCommand::Add {
            name,
            product,
            target,
            username,
            database,
            environment,
            read_only,
            password_stdin,
        } => {
            let product = Product::parse(product).with_context(|| {
                let known: Vec<&str> = Product::ALL.iter().map(|p| p.key()).collect();
                format!(
                    "unknown product '{product}'; try one of {}",
                    known.join(", ")
                )
            })?;
            let environment_kind = EnvironmentKind::parse(environment).with_context(|| {
                format!(
                    "unknown environment '{environment}'; try one of {}",
                    EnvironmentKind::ALL
                        .iter()
                        .map(|kind| kind.key())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;

            let location = if product == Product::Sqlite {
                Location::Sqlite {
                    sqlite: SqliteLocation {
                        path: target.clone(),
                        access_mode: if *read_only {
                            SqliteAccessMode::ReadOnly
                        } else {
                            SqliteAccessMode::ReadWrite
                        },
                    },
                }
            } else {
                let (host, port) = match target.rsplit_once(':') {
                    Some((host, port)) => (
                        host.to_string(),
                        port.parse::<u16>()
                            .with_context(|| format!("not a port: {port}"))?,
                    ),
                    None => (
                        target.clone(),
                        product
                            .default_port()
                            .context("this product needs a port")?,
                    ),
                };
                Location::Network {
                    endpoints: vec![NetworkEndpoint {
                        host,
                        port: Port::new(port)?,
                        role: EndpointRole::Primary,
                    }],
                }
            };

            let mut authentication = Authentication::default();
            let secrets = secret_store(directories);
            let identifier = Uuid::new_v4();
            if *password_stdin {
                let password = read_stdin(stdin)?;
                if password.is_empty() {
                    anyhow::bail!("no password on stdin");
                }
                secrets
                    .store(identifier, SecretPurpose::Password, password.as_bytes())
                    .await?;
                authentication = Authentication {
                    kind: if username.is_some() {
                        AuthenticationKind::UsernameAndPassword
                    } else {
                        AuthenticationKind::Password
                    },
                    secret_references: vec![SecretReference {
                        identifier,
                        purpose: SecretPurpose::Password,
                    }],
                    source: Some("stdin".into()),
                };
            }

            let now = chrono::Utc::now();
            let definition = ConnectionDefinition {
                version: SCHEMA_VERSION,
                id: Uuid::new_v4(),
                display_name: name.clone(),
                product_hint: product,
                location,
                username: username.clone(),
                namespaces: NamespaceDefaults {
                    database: database.clone(),
                    logical_database: database.clone(),
                    ..NamespaceDefaults::default()
                },
                deployment_mode: DeploymentMode::Automatic,
                authentication,
                tls: TlsConfiguration::default(),
                tunnel: None,
                limits: ConnectionLimits::default(),
                read_only_policy: if *read_only {
                    ReadOnlyPolicy::Required
                } else {
                    ReadOnlyPolicy::Disabled
                },
                production_policy: if environment_kind == EnvironmentKind::Production {
                    ProductionPolicy::RequireMutationPreview
                } else {
                    ProductionPolicy::Standard
                },
                environment: EnvironmentMetadata {
                    kind: environment_kind,
                    label: environment_kind.key().to_string(),
                    protection: if environment_kind == EnvironmentKind::Production {
                        EnvironmentProtection::ConfirmationRequired
                    } else {
                        EnvironmentProtection::Standard
                    },
                },
                group: None,
                tags: vec![],
                color: None,
                is_favorite: false,
                created_at: now,
                updated_at: now,
                last_tested_at: None,
                last_used_at: None,
            };
            store.save_connection(&definition)?;

            let backing = secret_store(directories).backing().await;
            output.emit(&definition, || {
                format!(
                    "saved {} ({})\ncredentials go to {}",
                    definition.display_name,
                    definition.location.summary(),
                    backing.title()
                )
            })
        }

        ConnectionCommand::Policy {
            connection,
            read_only,
            environment,
        } => {
            let mut definition = store.resolve(connection)?;
            if let Some(read_only) = read_only {
                definition.read_only_policy = if *read_only {
                    ReadOnlyPolicy::Required
                } else {
                    ReadOnlyPolicy::Disabled
                };
            }
            if let Some(environment) = environment {
                let kind = EnvironmentKind::parse(environment)
                    .with_context(|| format!("unknown environment '{environment}'"))?;
                definition.environment = EnvironmentMetadata {
                    kind,
                    label: kind.key().to_string(),
                    protection: if kind == EnvironmentKind::Production {
                        EnvironmentProtection::ConfirmationRequired
                    } else {
                        EnvironmentProtection::Standard
                    },
                };
            }
            definition.updated_at = chrono::Utc::now();
            store.save_connection(&definition)?;
            output.emit(&definition, || {
                format!(
                    "{} · {} · changes {}",
                    definition.display_name,
                    definition.environment.label,
                    match definition.mutation_prohibition() {
                        Some(prohibition) => format!("refused ({})", prohibition.reason()),
                        None => "allowed".to_string(),
                    }
                )
            })
        }

        ConnectionCommand::Remove { connection } => {
            let definition = store.resolve(connection)?;
            let secrets = secret_store(directories);
            for reference in &definition.authentication.secret_references {
                let _ = secrets
                    .forget(reference.identifier, reference.purpose)
                    .await;
            }
            store.remove_connection(definition.id)?;
            output.emit(&json!({ "removed": definition.display_name }), || {
                format!("forgot {} and its credential", definition.display_name)
            })
        }
    }
}

fn saved_queries(
    directories: &AppDirectories,
    command: &SavedQueryCommand,
    stdin: Option<&str>,
    output: Output<'_>,
) -> Result<()> {
    let store = directories_store(directories)?;
    match command {
        SavedQueryCommand::List => {
            let queries = store.saved_queries(None)?;
            output.emit(&queries, || {
                if queries.is_empty() {
                    return "nothing saved".to_string();
                }
                let rows: Vec<Vec<String>> = queries
                    .iter()
                    .map(|query| {
                        vec![
                            query.id.to_string()[..8].to_string(),
                            query.name.clone(),
                            veronica_database::value::truncate(&query.text.replace('\n', " "), 60),
                        ]
                    })
                    .collect();
                format::table(&["id", "name", "query"], &rows)
            })
        }
        SavedQueryCommand::Save {
            name,
            text,
            connection,
        } => {
            let text = match text {
                Some(text) => text.clone(),
                None => read_stdin(stdin)?,
            };
            if text.trim().is_empty() {
                anyhow::bail!("no query text");
            }
            let connection_id = connection
                .as_deref()
                .map(|needle| store.resolve(needle).map(|found| found.id))
                .transpose()?;
            let now = chrono::Utc::now();
            let query = SavedQuery {
                id: Uuid::new_v4(),
                connection_id,
                name: name.clone(),
                text,
                created_at: now,
                updated_at: now,
            };
            store.save_query(&query)?;
            output.emit(&query, || format!("saved {} as {}", query.name, query.id))
        }
        SavedQueryCommand::Get { id } => {
            let id = Uuid::parse_str(id).context("that is not a saved query id")?;
            let query = store
                .saved_query(id)?
                .context("no saved query with that id")?;
            output.emit(&query, || query.text.clone())
        }
        SavedQueryCommand::Remove { id } => {
            let id = Uuid::parse_str(id).context("that is not a saved query id")?;
            if !store.remove_query(id)? {
                anyhow::bail!("no saved query with that id");
            }
            output.emit(&json!({ "removed": id }), || format!("removed {id}"))
        }
    }
}

fn build_plan(
    definition: &ConnectionDefinition,
    sql: Option<&String>,
    command: Option<&String>,
    params: &[String],
    action: &str,
    target: &str,
    scope: &str,
) -> Result<Plan> {
    let action: Action = serde_json::from_value(json!(action))
        .with_context(|| format!("unknown action '{action}'"))?;
    let scope: Scope =
        serde_json::from_value(json!(scope)).with_context(|| format!("unknown scope '{scope}'"))?;

    let parameters: Vec<Parameter> = params
        .iter()
        .enumerate()
        .map(|(index, raw)| {
            let json: serde_json::Value = serde_json::from_str(raw)
                .unwrap_or_else(|_| serde_json::Value::String(raw.clone()));
            Ok(Parameter {
                name: format!("{}", index + 1),
                value: Value::from_json(&json),
            })
        })
        .collect::<Result<_>>()?;

    let payload = match (sql, command) {
        (Some(statement), None) => Payload::Relational {
            product: definition.product_hint,
            statement: statement.clone(),
            parameters,
        },
        (None, Some(command)) => Payload::Keyspace {
            product: definition.product_hint,
            command: command.clone(),
            arguments: parameters,
        },
        _ => anyhow::bail!("give exactly one of --sql or --command"),
    };

    Ok(Plan {
        payload,
        action,
        scope,
        impact: Impact::unknown(),
        transaction_behavior: match definition.product_hint.family() {
            veronica_database::product::Family::Relational => TransactionBehavior::Transactional,
            _ => TransactionBehavior::Nontransactional,
        },
        rollback_availability: match definition.product_hint.family() {
            veronica_database::product::Family::Relational => RollbackAvailability::Available,
            _ => RollbackAvailability::Unavailable,
        },
        execution_mode: ExecutionMode::Synchronous,
        target: TargetIdentifier::object(
            definition.id,
            object_from_path(definition.product_hint, target),
        ),
        context: MutationContext {
            kind: ContextKind::Database,
            value: definition
                .namespaces
                .database
                .clone()
                .unwrap_or_else(|| definition.display_name.clone()),
            catalog: definition.namespaces.catalog.clone(),
            schema: definition.namespaces.schema.clone(),
        },
        selected_records: vec![],
        predicate: None,
    })
}

async fn mutations(
    directories: &AppDirectories,
    command: &MutationCommand,
    stdin: Option<&str>,
    output: Output<'_>,
) -> Result<()> {
    let mut store = directories_store(directories)?;
    let secrets = secret_store(directories);

    match command {
        MutationCommand::Preview {
            connection,
            sql,
            command,
            params,
            action,
            target,
            scope,
            lifetime,
            save_plan,
        } => {
            let definition = store.resolve(connection)?;
            let plan = build_plan(
                &definition,
                sql.as_ref(),
                command.as_ref(),
                params,
                action,
                target,
                scope,
            )?;
            let mut session = Session::open(&definition, &secrets).await?;
            let preview = session
                .preview(&plan, &mut store, &secrets, *lifetime)
                .await?;

            if let Some(path) = save_plan {
                std::fs::write(path, serde_json::to_vec_pretty(&plan)?)
                    .with_context(|| format!("cannot write {path}"))?;
            }

            output.emit(&json!({ "preview": preview, "plan": plan }), || {
                let mut out = format!(
                    "{} on {}\n\n",
                    preview.effect.action.key(),
                    definition.display_name
                );
                let _ = writeln!(out, "  {}", preview.request.command);
                if !preview.request.parameters.is_empty() {
                    let _ = writeln!(
                        out,
                        "  values: {}",
                        preview
                            .request
                            .parameters
                            .iter()
                            .map(|parameter| format!(
                                "{}=<{:?}>",
                                parameter.name, parameter.value_kind
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                let _ = writeln!(out, "\nAffects  {}", preview.effect.impact.describe());
                let _ = writeln!(out, "Scope    {:?}", preview.effect.scope);
                for warning in &preview.warnings {
                    let _ = writeln!(out, "\n  ! {}", warning.detail);
                }
                let _ = write!(
                    out,
                    "\nNothing has been changed. To go ahead:\n\n  \
                     vr database mutations apply {} \\\n    --plan {} \\\n    \
                     --token {} \\\n    --confirm '{}'\n\nThe token works once and \
                     expires {}.",
                    connection,
                    save_plan.as_deref().unwrap_or("<plan.json>"),
                    preview.token,
                    preview.required_confirmation.text,
                    preview.expires_at.format("at %H:%M:%S UTC")
                );
                out
            })
        }

        MutationCommand::Apply {
            connection,
            plan,
            token,
            confirm,
        } => {
            let definition = store.resolve(connection)?;
            let text = if plan == "-" {
                read_stdin(stdin)?
            } else {
                std::fs::read_to_string(plan).with_context(|| format!("cannot read {plan}"))?
            };
            let plan: Plan = serde_json::from_str(&text)
                .context("that file is not a plan `vr database mutations preview` wrote")?;

            let mut session = Session::open(&definition, &secrets).await?;
            let applied = session
                .apply(&plan, token, confirm, &mut store, &secrets)
                .await?;
            output.emit(&applied, || {
                format!(
                    "{} · {} record{} in {} ms",
                    applied.effect.action.key(),
                    applied.affected_records,
                    if applied.affected_records == 1 {
                        ""
                    } else {
                        "s"
                    },
                    applied.elapsed_millis
                )
            })
        }
    }
}
