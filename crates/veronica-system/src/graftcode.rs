//! Explicit, bounded calls to a user-owned local Graftcode Gateway. No listeners.
use anyhow::{bail, ensure, Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Url,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
const MAX_RESPONSE: usize = 256 * 1024;
static REQUESTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Connection {
    pub endpoint: String,
    #[serde(default)]
    pub token: String,
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Discovery {
    pub server: Value,
    pub tools: Vec<Value>,
    pub install_commands: Value,
}
fn endpoint(value: &str) -> Result<Url> {
    ensure!(value.len() <= 512, "Endpoint is too long");
    let mut url = Url::parse(value).context("Use http://127.0.0.1:PORT/mcp")?;
    ensure!(
        url.scheme() == "http"
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/mcp",
        "Use an HTTP loopback endpoint ending in /mcp, without credentials or query parameters"
    );
    let local = matches!(url.host(),Some(url::Host::Ipv4(ip)) if ip.is_loopback())
        || matches!(url.host(),Some(url::Host::Ipv6(ip)) if ip.is_loopback())
        || url.host_str() == Some("localhost");
    ensure!(local, "Only local loopback Gateways are supported");
    if url.host_str() == Some("localhost") {
        url.set_host(Some("127.0.0.1"))?;
    }
    ensure!(
        url.port_or_known_default() != Some(0),
        "Port must be non-zero"
    );
    Ok(url)
}
struct Session {
    client: Client,
    url: Url,
    headers: HeaderMap,
    id: u64,
}
impl Session {
    fn new(connection: &Connection) -> Result<Self> {
        let url = endpoint(&connection.endpoint)?;
        ensure!(
            connection.token.len() <= 2048
                && connection.token.bytes().all(|b| b.is_ascii_graphic()),
            "Bearer token must contain at most 2048 visible ASCII characters"
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            "accept",
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        if !connection.token.is_empty() {
            headers.insert(
                "authorization",
                HeaderValue::from_str(&format!("Bearer {}", connection.token))?,
            );
        }
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(8))
            .connect_timeout(Duration::from_secs(2))
            .build()?;
        Ok(Self {
            client,
            url,
            headers,
            id: 0,
        })
    }
    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.id += 1;
        let mut response = self
            .client
            .post(self.url.clone())
            .headers(self.headers.clone())
            .json(&json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params}))
            .send()
            .await
            .context("Cannot reach local Gateway")?;
        ensure!(
            response.status().is_success(),
            "Gateway returned HTTP {}",
            response.status()
        );
        if let Some(session) = response.headers().get("mcp-session-id") {
            ensure!(session.as_bytes().len() <= 256, "Invalid session header");
            self.headers.insert("mcp-session-id", session.clone());
        }
        let event_stream = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|s| s.starts_with("text/event-stream"));
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                bytes.len() + chunk.len() <= MAX_RESPONSE,
                "Gateway response exceeds 256 KiB"
            );
            bytes.extend_from_slice(&chunk);
            if event_stream {
                let text = std::str::from_utf8(&bytes)
                    .unwrap_or_default()
                    .replace("\r\n", "\n");
                for event in text.split("\n\n").take(text.matches("\n\n").count()) {
                    let data = event
                        .lines()
                        .filter_map(|l| l.strip_prefix("data:").map(str::trim_start))
                        .collect::<Vec<_>>()
                        .join("\n");
                    if let Ok(value) = serde_json::from_str::<Value>(&data) {
                        if value.get("id") == Some(&json!(self.id)) {
                            return rpc_result(value, self.id);
                        }
                    }
                }
            }
        }
        ensure!(
            !event_stream,
            "Gateway event stream ended without a matching response"
        );
        rpc_result(
            serde_json::from_slice(&bytes).context("Gateway returned invalid JSON")?,
            self.id,
        )
    }
    async fn initialize(&mut self) -> Result<Value> {
        let response=self.request("initialize",json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"Veronica","version":env!("CARGO_PKG_VERSION")}})).await?;
        let version = response
            .get("protocolVersion")
            .and_then(Value::as_str)
            .context("Gateway did not negotiate an MCP version")?;
        ensure!(
            ["2024-11-05", "2025-03-26", "2025-06-18"].contains(&version),
            "Unsupported Gateway MCP version"
        );
        self.headers
            .insert("mcp-protocol-version", HeaderValue::from_str(version)?);
        let notification = self
            .client
            .post(self.url.clone())
            .headers(self.headers.clone())
            .json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .send()
            .await?;
        ensure!(
            notification.status().is_success(),
            "Gateway initialization notification failed"
        );
        Ok(response.get("serverInfo").cloned().unwrap_or(Value::Null))
    }
    async fn tools(&mut self) -> Result<Vec<Value>> {
        let result = self.request("tools/list", json!({})).await?;
        ensure!(
            result.get("nextCursor").is_none_or(Value::is_null),
            "Gateway has multiple tool pages; narrow its exposed methods first"
        );
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .context("Gateway did not return tools")?;
        ensure!(
            tools.len() <= 64,
            "Gateway exposes more than 64 tools; narrow its methods first"
        );
        let mut names = std::collections::HashSet::new();
        for tool in tools {
            let name = tool
                .get("name")
                .and_then(Value::as_str)
                .context("Tool has no name")?;
            ensure!(
                name.len() <= 256 && !name.is_empty() && names.insert(name),
                "Invalid or duplicate tool name"
            );
        }
        Ok(tools.clone())
    }
    async fn close(&self) {
        if self.headers.contains_key("mcp-session-id") {
            let _ = self
                .client
                .delete(self.url.clone())
                .headers(self.headers.clone())
                .timeout(Duration::from_secs(1))
                .send()
                .await;
        }
    }
    async fn installation(&self) -> Value {
        // Display only. Never execute commands or fetch advertised registry URLs.
        let mut url = self.url.clone();
        url.set_path("/discover");
        let Ok(mut response) = self
            .client
            .get(url)
            .headers(self.headers.clone())
            .send()
            .await
        else {
            return Value::Null;
        };
        if !response.status().is_success() {
            return Value::Null;
        }
        let mut bytes = Vec::new();
        while let Ok(Some(chunk)) = response.chunk().await {
            if bytes.len() + chunk.len() > MAX_RESPONSE {
                return Value::Null;
            }
            bytes.extend_from_slice(&chunk);
        }
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            return Value::Null;
        };
        let installations = value
            .get("modules")
            .and_then(Value::as_array)
            .map(|modules| {
                modules
                    .iter()
                    .take(16)
                    .filter_map(|m| m.get("installation").cloned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        json!(installations)
    }
}
fn rpc_result(value: Value, id: u64) -> Result<Value> {
    ensure!(
        value.get("jsonrpc") == Some(&json!("2.0")) && value.get("id") == Some(&json!(id)),
        "Gateway returned an unrelated response"
    );
    if value.get("error").is_some() {
        bail!(
            "Gateway rejected the request: {}",
            value["error"]["message"]
                .as_str()
                .unwrap_or("RPC error")
                .chars()
                .take(512)
                .collect::<String>()
        );
    }
    value
        .get("result")
        .cloned()
        .context("Gateway returned no result")
}
pub async fn discover(connection: Connection) -> Result<Discovery> {
    let _permit = REQUESTS
        .try_acquire()
        .context("Two Gateway requests are already running")?;
    let mut session = Session::new(&connection)?;
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        let server = session.initialize().await?;
        let tools = session.tools().await?;
        let install_commands = session.installation().await;
        Ok(Discovery {
            server,
            tools,
            install_commands,
        })
    })
    .await
    .unwrap_or_else(|_| Err(anyhow::anyhow!("Gateway discovery timed out")));
    session.close().await;
    result
}
pub async fn call(
    connection: Connection,
    name: String,
    mut arguments: Value,
    confirmed: bool,
) -> Result<Value> {
    ensure!(confirmed, "Confirm this method call before running it");
    ensure!(
        arguments.is_object() && serde_json::to_vec(&arguments)?.len() <= 16 * 1024,
        "Arguments must be a JSON object of at most 16 KiB"
    );
    ensure!(!name.is_empty() && name.len() <= 256, "Invalid tool name");
    let _permit = REQUESTS
        .try_acquire()
        .context("Two Gateway requests are already running")?;
    let mut session = Session::new(&connection)?;
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        session.initialize().await?;
        let tools = session.tools().await?;
        ensure!(
            tools.iter().any(|t| t["name"].as_str() == Some(&name)),
            "This tool is no longer advertised by the Gateway"
        );
        if !connection.token.is_empty()
            && tools
                .iter()
                .find(|t| t["name"].as_str() == Some(&name))
                .and_then(|t| t["inputSchema"]["properties"].as_object())
                .is_some_and(|p| p.contains_key("sessionToken"))
        {
            if let Some(args) = arguments.as_object_mut() {
                args.entry("sessionToken")
                    .or_insert_with(|| json!(connection.token));
            }
        }
        ensure!(serde_json::to_vec(&arguments)?.len()<=16*1024,"Arguments including the access token exceed 16 KiB");
        session
            .request("tools/call", json!({"name":name,"arguments":arguments}))
            .await
    })
    .await
    .unwrap_or_else(|_| Err(anyhow::anyhow!("Gateway method timed out; it may still be running in the Gateway. Verify before retrying.")));
    session.close().await;
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loopback_only_without_redirects_credentials_or_fragments() {
        for value in [
            "http://example.com:81/mcp",
            "http://127.0.0.1:81/admin",
            "http://user:secret@127.0.0.1/mcp",
            "http://127.0.0.1/mcp?x=1",
            "http://127.0.0.1/mcp#x",
            "file:///etc/passwd",
            "http://169.254.169.254/mcp",
        ] {
            assert!(endpoint(value).is_err(), "{value}");
        }
        for value in [
            "http://localhost:18880/mcp",
            "http://127.0.0.1:81/mcp",
            "http://[::1]:81/mcp",
        ] {
            assert!(endpoint(value).is_ok(), "{value}");
        }
    }
    #[tokio::test]
    async fn refuses_unconfirmed_and_oversized_calls_before_network() {
        let c = Connection {
            endpoint: "http://127.0.0.1:1/mcp".into(),
            token: String::new(),
        };
        assert!(call(c.clone(), "x".into(), json!({}), false)
            .await
            .unwrap_err()
            .to_string()
            .contains("Confirm"));
        assert!(call(c, "x".into(), json!({"x":"a".repeat(20000)}), true)
            .await
            .unwrap_err()
            .to_string()
            .contains("16 KiB"));
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    async fn fixture(mode: &'static str) -> (Connection, tokio::task::JoinHandle<Vec<String>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let connection = Connection {
            endpoint: format!("http://{}/mcp", listener.local_addr().unwrap()),
            token: "a".repeat(64),
        };
        let task = tokio::spawn(async move {
            let mut seen = Vec::new();
            loop {
                let Ok(Ok((mut stream, _))) =
                    tokio::time::timeout(Duration::from_secs(3), listener.accept()).await
                else {
                    break;
                };
                let mut bytes = Vec::new();
                let mut chunk = [0; 4096];
                loop {
                    let n = stream.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|v| v.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let request = String::from_utf8(bytes).unwrap();
                let end = request.find("\r\n\r\n").unwrap();
                let body =
                    serde_json::from_str::<Value>(&request[end + 4..]).unwrap_or(Value::Null);
                let method = body["method"].as_str().unwrap_or("");
                if request.starts_with("DELETE") {
                    seen.push("closed".into());
                    let _ = stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        )
                        .await;
                    break;
                }
                if mode == "redirect" {
                    let _=stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                    break;
                }
                seen.push(method.to_string());
                let id = body["id"].clone();
                let result = match method {
                    "initialize" => {
                        json!({"protocolVersion":"2025-06-18","serverInfo":{"name":"graftcode-gateway"}})
                    }
                    "tools/list" => {
                        json!({"tools":[{"name":"inspect","inputSchema":{"type":"object","properties":{"sessionToken":{"type":"string"}}}}]})
                    }
                    "tools/call" => {
                        assert_eq!(body["params"]["arguments"]["sessionToken"], "a".repeat(64));
                        json!({"content":[{"type":"text","text":"checked"}]})
                    }
                    _ => json!({"modules":[]}),
                };
                let payload = if mode == "oversized" {
                    "x".repeat(MAX_RESPONSE + 1)
                } else {
                    serde_json::to_string(&json!({"jsonrpc":"2.0","id":id,"result":result}))
                        .unwrap()
                };
                let response = if method == "notifications/initialized" {
                    "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
                } else {
                    format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",payload.len(),payload)
                };
                let _ = stream.write_all(response.as_bytes()).await;
                if mode == "oversized" {
                    break;
                }
            }
            seen
        });
        (connection, task)
    }
    #[tokio::test]
    async fn real_http_negotiates_calls_with_token_and_closes() {
        let (connection, task) = fixture("normal").await;
        let response = call(connection, "inspect".into(), json!({}), true)
            .await
            .unwrap();
        assert_eq!(response["content"][0]["text"], "checked");
        assert_eq!(
            task.await.unwrap(),
            vec![
                "initialize",
                "notifications/initialized",
                "tools/list",
                "tools/call",
                "closed"
            ]
        );
    }
    #[tokio::test]
    async fn refuses_http_redirect_and_oversized_body() {
        for mode in ["redirect", "oversized"] {
            let (connection, task) = fixture(mode).await;
            let error = discover(connection).await.unwrap_err().to_string();
            assert!(
                error.contains(if mode == "redirect" {
                    "HTTP 302"
                } else {
                    "256 KiB"
                }),
                "{error}"
            );
            task.await.unwrap();
        }
    }
}
