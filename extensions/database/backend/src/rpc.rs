use serde::Deserialize;
use serde_json::Value;
use veronica_core::AppDirectories;
use veronica_database::connection::ConnectionDefinition;
use veronica_database::identify::ObjectIdentifier;
use veronica_database::paging::{Page, PageRequest, PageSize};
use veronica_database::secrets::SecretStore;
use veronica_database::session::Session;
use veronica_database::store::MetadataStore;
use veronica_sdk::protocol::rpc::ErrorObject;
use veronica_sdk::rpc::{failure, reply};

pub(crate) const METHODS: [&str; 7] = [
    "connections",
    "test",
    "browse",
    "query",
    "read",
    "capabilities",
    "operations",
];

const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

#[derive(Deserialize)]
struct Target {
    connection: String,
}

#[derive(Deserialize)]
struct Browse {
    connection: String,
    #[serde(default)]
    path: Vec<String>,
}

#[derive(Deserialize)]
struct Query {
    connection: String,
    statement: String,
    limit: u32,
    offset: u64,
}

#[derive(Deserialize)]
struct Read {
    connection: String,
    path: Vec<String>,
    limit: u32,
    offset: u64,
}

#[derive(Deserialize)]
struct Operations {
    limit: usize,
}

fn store() -> anyhow::Result<MetadataStore> {
    MetadataStore::open(AppDirectories::current()?.database_store())
}

async fn session(connection: &str) -> anyhow::Result<(ConnectionDefinition, Session)> {
    let definition = store()?.resolve(connection)?;
    let secrets = SecretStore::new(AppDirectories::current()?.database_secrets_fallback());
    let session = Session::open(&definition, &secrets).await?;
    Ok((definition, session))
}

fn page_request(limit: u32, offset: u64) -> PageRequest {
    PageRequest {
        page_size: PageSize::clamped(limit),
        offset,
        ..Default::default()
    }
}

fn page_for_webview(page: &Page) -> anyhow::Result<Value> {
    let mut json = serde_json::to_value(page)?;
    exact_large_integers(&mut json);
    Ok(json)
}

fn exact_large_integers(json: &mut Value) {
    match json {
        Value::Object(map) => {
            let integer = matches!(
                map.get("kind").and_then(Value::as_str),
                Some("signedInteger" | "unsignedInteger")
            );
            if integer {
                if let Some(value) = map.get_mut("value") {
                    let unsafe_number = match value {
                        Value::Number(number) => number
                            .as_i64()
                            .map(|n| n.unsigned_abs() > MAX_SAFE_INTEGER)
                            .or_else(|| number.as_u64().map(|n| n > MAX_SAFE_INTEGER))
                            .unwrap_or(false),
                        _ => false,
                    };
                    if unsafe_number {
                        *value = Value::String(value.to_string());
                    }
                }
            }
            map.values_mut().for_each(exact_large_integers);
        }
        Value::Array(items) => items.iter_mut().for_each(exact_large_integers),
        _ => {}
    }
}

async fn dispatch(method: &str, arguments: Value) -> anyhow::Result<Value> {
    Ok(match method {
        "connections" => serde_json::to_value(store()?.connections()?)?,
        "test" => {
            let Target { connection } = serde_json::from_value(arguments)?;
            let (_, mut session) = session(&connection).await?;
            serde_json::to_value(session.identify().await?)?
        }
        "browse" => {
            let Browse { connection, path } = serde_json::from_value(arguments)?;
            let (definition, mut session) = session(&connection).await?;
            let parent = (!path.is_empty())
                .then(|| ObjectIdentifier::for_path(definition.product_hint, path));
            serde_json::to_value(session.objects(parent.as_ref()).await?)?
        }
        "query" => {
            let request: Query = serde_json::from_value(arguments)?;
            let (_, mut session) = session(&request.connection).await?;
            let page = session
                .query(
                    &request.statement,
                    &page_request(request.limit, request.offset),
                )
                .await?;
            page_for_webview(&page)?
        }
        "read" => {
            let request: Read = serde_json::from_value(arguments)?;
            let (definition, mut session) = session(&request.connection).await?;
            let page = session
                .read(
                    &ObjectIdentifier::for_path(definition.product_hint, request.path),
                    &page_request(request.limit, request.offset),
                )
                .await?;
            page_for_webview(&page)?
        }
        "capabilities" => {
            let Target { connection } = serde_json::from_value(arguments)?;
            let (_, mut session) = session(&connection).await?;
            serde_json::to_value(session.capabilities().await?)?
        }
        "operations" => {
            let Operations { limit } = serde_json::from_value(arguments)?;
            serde_json::to_value(store()?.operations(limit)?)?
        }
        other => anyhow::bail!("unknown method {other}"),
    })
}

pub(crate) async fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    if !METHODS.contains(&method) {
        return Err(ErrorObject::method_not_found(method));
    }
    let value = dispatch(method, arguments)
        .await
        .map_err(|error| failure(&error))?;
    reply(&value)
}

#[cfg(test)]
mod tests {
    use veronica_database::value::Value;

    #[test]
    fn integers_past_javascripts_range_reach_the_page_exactly() {
        let mut json = serde_json::json!([
            serde_json::to_value(Value::SignedInteger(1_234_567_890_123_456_789)).unwrap(),
            serde_json::to_value(Value::SignedInteger(-9_007_199_254_740_993)).unwrap(),
            serde_json::to_value(Value::UnsignedInteger(u64::MAX)).unwrap(),
            serde_json::to_value(Value::SignedInteger(42)).unwrap(),
            serde_json::to_value(Value::Array(vec![Value::UnsignedInteger(1 << 60)])).unwrap(),
        ]);
        super::exact_large_integers(&mut json);
        assert_eq!(json[0]["value"], "1234567890123456789");
        assert_eq!(json[1]["value"], "-9007199254740993");
        assert_eq!(json[2]["value"], "18446744073709551615");
        assert_eq!(json[3]["value"], 42, "safe integers stay numbers");
        assert_eq!(json[4]["value"][0]["value"], "1152921504606846976");
    }
}
