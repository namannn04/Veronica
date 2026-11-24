use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::{Context as _, Result};
use serde_json::{json, Map, Value};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::task::JoinSet;
use veronica_host::Layout;

use super::protocol::{
    self, Message, RpcError, INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND,
};
use super::tools::{self, CatalogCall, Invocation, CATALOG_TOOL};
use super::resources;

const SERVER_NAME: &str = "veronica";
const SETTLE: Duration = Duration::from_millis(150);
const CONCURRENT: [&str; 2] = ["tools/call", "resources/read"];

const INSTRUCTIONS: &str = "\
Veronica runs on this computer and exposes its `vr` commands as tools.

1. Discover: tools/list shows one tool per command, named veronica_<area>_<action>. \
Each description ends with the command's usage line.
2. Call: pass the words after the command in `arguments`, as on the command line, \
without --json or --yes. Every call answers with one JSON document.
3. Confirm: destructive tools change nothing unless `confirm` is true. Without it they \
return a preview; show it to the person and ask before calling again with confirm.
4. Install when needed: if no tool fits, call veronica_catalog with action search, then \
install with confirm. The new extension's tools appear and the server announces that the \
list changed.
5. Navigate: the resource veronica://guide lists every command as data, \
veronica://guide/<topic> holds one part of the manual and veronica://extensions/<id> \
describes one extension.";

pub(crate) struct Server {
    program: PathBuf,
    version: Mutex<Option<&'static str>>,
    outgoing: UnboundedSender<Value>,
}

impl Server {
    fn version(&self) -> Option<&'static str> {
        *self.version.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn send(&self, message: Value) {
        let _ = self.outgoing.send(message);
    }

    fn initialize(&self, params: &Value) -> Value {
        let version = protocol::negotiate(params["protocolVersion"].as_str());
        *self.version.lock().unwrap_or_else(PoisonError::into_inner) = Some(version);
        json!({
            "protocolVersion": version,
            "capabilities": {
                "tools": { "listChanged": true },
                "resources": { "listChanged": true },
            },
            "serverInfo": {
                "name": SERVER_NAME,
                "title": "Veronica",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "instructions": INSTRUCTIONS,
        })
    }

    fn list_tools() -> Value {
        let routes = tools::current_routes();
        let mut listed = vec![tools::catalog_tool()];
        listed.extend(tools::discover(&routes).iter().map(tools::Tool::describe));
        json!({ "tools": listed })
    }

    async fn call(&self, params: &Value) -> Result<Value, RpcError> {
        let version = self.version().unwrap_or(protocol::LATEST_VERSION);
        let name = params["name"]
            .as_str()
            .ok_or_else(|| RpcError::new(INVALID_PARAMS, "tools/call needs a tool name"))?;
        let input = match &params["arguments"] {
            Value::Null => Map::new(),
            Value::Object(input) => input.clone(),
            _ => return Err(RpcError::new(INVALID_PARAMS, "arguments must be an object")),
        };
        if name == CATALOG_TOOL {
            return match tools::catalog_call(&input)? {
                CatalogCall::Preview(preview) => Ok(tools::document_result(&preview, version)),
                CatalogCall::Run { invocation, status } => {
                    let mut completed = self.run(&invocation).await?;
                    if let Some(enabled) = status {
                        tools::filter_listing(&mut completed, enabled);
                    }
                    Ok(tools::result(&completed, version))
                }
            };
        }
        let routes = tools::current_routes();
        let tool = tools::discover(&routes)
            .into_iter()
            .find(|tool| tool.name == name)
            .ok_or_else(|| RpcError::new(INVALID_PARAMS, format!("no tool called {name}")))?;
        let invocation = tool.argv(&input)?;
        let completed = self.run(&invocation).await?;
        Ok(tools::result(&completed, version))
    }

    async fn run(&self, invocation: &Invocation) -> Result<tools::Completed, RpcError> {
        invocation
            .run(&self.program)
            .await
            .map_err(|error| RpcError::new(INTERNAL_ERROR, format!("cannot run vr: {error}")))
    }

    async fn respond(self: Arc<Self>, id: Value, method: String, params: Value) {
        let outcome = match method.as_str() {
            _ if self.version().is_none() => self.answer(&method, &params),
            "tools/call" => self.call(&params).await,
            "resources/read" => resources::read(&params, &self.program).await,
            _ => self.answer(&method, &params),
        };
        self.send(match outcome {
            Ok(result) => protocol::success(&id, result),
            Err(error) => protocol::failure(&id, &error),
        });
    }

    fn answer(&self, method: &str, params: &Value) -> Result<Value, RpcError> {
        match method {
            "initialize" => Ok(self.initialize(params)),
            "ping" => Ok(json!({})),
            _ if self.version().is_none() => Err(RpcError::new(
                INVALID_REQUEST,
                "the session is not initialized; send initialize first",
            )),
            "tools/list" => Ok(Self::list_tools()),
            "resources/list" => Ok(resources::list()),
            "resources/templates/list" => Ok(resources::templates()),
            other => Err(RpcError::new(
                METHOD_NOT_FOUND,
                format!("no method called {other}"),
            )),
        }
    }
}

pub(crate) async fn serve() -> Result<()> {
    let program = std::env::current_exe().context("cannot find the running vr")?;
    let (outgoing, mut queue) = unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(message) = queue.recv().await {
            let mut line = message.to_string();
            line.push('\n');
            if stdout.write_all(line.as_bytes()).await.is_err() || stdout.flush().await.is_err() {
                break;
            }
        }
    });
    let server = Arc::new(Server {
        program,
        version: Mutex::new(None),
        outgoing,
    });
    let watcher = watch(&server);
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut work = JoinSet::new();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        match protocol::parse(&line) {
            Message::Request { id, method, params } if CONCURRENT.contains(&method.as_str()) => {
                work.spawn(Arc::clone(&server).respond(id, method, params));
            }
            Message::Request { id, method, params } => {
                Arc::clone(&server).respond(id, method, params).await;
            }
            Message::Invalid { id, error } => server.send(protocol::failure(&id, &error)),
            Message::Notification { .. } | Message::Response => {}
        }
        while work.try_join_next().is_some() {}
    }
    while work.join_next().await.is_some() {}
    drop(watcher);
    drop(server);
    writer
        .await
        .context("the MCP writer stopped unexpectedly")?;
    Ok(())
}

fn watch(server: &Arc<Server>) -> Option<notify::RecommendedWatcher> {
    use notify::Watcher as _;

    let installed = match Layout::current() {
        Ok(layout) => layout.installed_file(),
        Err(error) => {
            eprintln!("vr: cannot watch installed extensions: {error:#}");
            return None;
        }
    };
    let directory = installed.parent()?.to_path_buf();
    let name = installed.file_name()?.to_os_string();
    let (changed, mut signals) = unbounded_channel::<()>();
    let started = std::fs::create_dir_all(&directory)
        .map_err(notify::Error::io)
        .and_then(|()| {
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let relevant = event.is_ok_and(|event| {
                    event
                        .paths
                        .iter()
                        .any(|path| path.file_name() == Some(name.as_os_str()))
                });
                if relevant {
                    let _ = changed.send(());
                }
            })
        })
        .and_then(|mut watcher| {
            watcher
                .watch(&directory, notify::RecursiveMode::NonRecursive)
                .map(|()| watcher)
        });
    let watcher = match started {
        Ok(watcher) => watcher,
        Err(error) => {
            eprintln!("vr: cannot watch {}: {error}", directory.display());
            return None;
        }
    };
    let server = Arc::clone(server);
    tokio::spawn(async move {
        while signals.recv().await.is_some() {
            tokio::time::sleep(SETTLE).await;
            while signals.try_recv().is_ok() {}
            if server.version().is_some() {
                server.send(protocol::notification("notifications/tools/list_changed"));
                server.send(protocol::notification(
                    "notifications/resources/list_changed",
                ));
            }
        }
    });
    Some(watcher)
}
