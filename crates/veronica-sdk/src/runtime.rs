use std::collections::HashMap;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures_util::FutureExt;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use tokio::task::AbortHandle;
use tracing_subscriber::EnvFilter;
use veronica_protocol::manifest::Permissions;
use veronica_protocol::rpc::{
    error_code, Cancel, CliRun, CliRunParams, CliRunResult, ContractRequest, ContractRequestParams,
    ErrorObject, HealthPing, HostTopic, Initialize, InitializeParams, InitializeResult,
    Initialized, JsonRpcVersion, Message, Notification, Outcome, Request, RequestId, RequestMethod,
    Response, RpcCall, RpcCallParams, RunMode, ShortcutFired, ShortcutFiredParams, Shutdown,
    SurfaceAction, SurfaceActionParams, TopicParams, MAX_MESSAGE_BYTES, PROTOCOL_VERSION,
};
use veronica_protocol::ExitCode;

use crate::dirs::Dirs;
use crate::gate::{confirmation_required, Gate};
use crate::host_client::{HostClient, Outgoing};

const READ_BUFFER_BYTES: usize = 64 * 1024;
const RUNTIME_SHUTDOWN_GRACE: Duration = Duration::from_secs(1);
const LOG_FILTER_ENV: &str = "VERONICA_LOG";

#[derive(Clone)]
pub struct Context {
    host: HostClient,
    dirs: Dirs,
    initialize: Arc<InitializeParams>,
    gate: Arc<Gate>,
}

impl Context {
    pub fn host(&self) -> &HostClient {
        &self.host
    }

    pub fn dirs(&self) -> &Dirs {
        &self.dirs
    }

    pub fn initialize(&self) -> &InitializeParams {
        &self.initialize
    }

    pub fn mode(&self) -> RunMode {
        self.initialize.mode
    }

    pub fn granted(&self) -> &Permissions {
        &self.initialize.granted
    }
}

pub trait Extension: Send + Sync + 'static {
    fn methods(&self) -> Vec<String> {
        Vec::new()
    }

    fn topics(&self) -> Vec<String> {
        Vec::new()
    }

    fn initialize(
        &self,
        _context: &Context,
    ) -> impl Future<Output = Result<(), ErrorObject>> + Send {
        async { Ok(()) }
    }

    fn cli_run(
        &self,
        _context: Context,
        _params: CliRunParams,
    ) -> impl Future<Output = Result<CliRunResult, ErrorObject>> + Send {
        async { Err(ErrorObject::method_not_found(CliRun::NAME)) }
    }

    fn cli_preview(
        &self,
        _context: Context,
        params: CliRunParams,
    ) -> impl Future<Output = Result<CliRunResult, ErrorObject>> + Send {
        async move { Ok(confirmation_required(&params)) }
    }

    fn rpc_call(
        &self,
        _context: Context,
        params: RpcCallParams,
    ) -> impl Future<Output = Result<Value, ErrorObject>> + Send {
        async move { Err(ErrorObject::method_not_found(&params.method)) }
    }

    fn contract_request(
        &self,
        _context: Context,
        params: ContractRequestParams,
    ) -> impl Future<Output = Result<Value, ErrorObject>> + Send {
        async move { Err(ErrorObject::method_not_found(&params.contract.to_string())) }
    }

    fn surface_action(
        &self,
        _context: Context,
        _params: SurfaceActionParams,
    ) -> impl Future<Output = Result<(), ErrorObject>> + Send {
        async { Err(ErrorObject::method_not_found(SurfaceAction::NAME)) }
    }

    fn shortcut_fired(
        &self,
        _context: Context,
        _params: ShortcutFiredParams,
    ) -> impl Future<Output = Result<(), ErrorObject>> + Send {
        async { Err(ErrorObject::method_not_found(ShortcutFired::NAME)) }
    }

    fn host_topic(
        &self,
        _context: Context,
        _params: TopicParams,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }

    fn shutdown(&self) -> impl Future<Output = ()> + Send {
        async {}
    }
}

pub fn run<E: Extension>(extension: E) -> std::process::ExitCode {
    let filter = EnvFilter::try_from_env(LOG_FILTER_ENV).unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(filter)
        .try_init();
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(%error, "could not start the async runtime");
            return ExitCode::BackendFailed.code().into();
        }
    };
    let outcome = runtime.block_on(serve(extension, tokio::io::stdin(), tokio::io::stdout()));
    runtime.shutdown_timeout(RUNTIME_SHUTDOWN_GRACE);
    match outcome {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "the backend stopped");
            ExitCode::BackendFailed.code().into()
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("reading from the host failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("the host speaks protocol {0}, this backend speaks {PROTOCOL_VERSION}")]
    ProtocolMismatch(u32),
    #[error("initialize failed: {0}")]
    Initialize(ErrorObject),
}

pub async fn serve<E, R, W>(extension: E, reader: R, writer: W) -> Result<(), ServeError>
where
    E: Extension,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (outgoing, receiver) = mpsc::unbounded_channel();
    let writer_task = tokio::spawn(write_loop(writer, receiver));
    let mut session = Session {
        extension: Arc::new(extension),
        host: HostClient::new(outgoing.clone()),
        context: None,
        inflight: Arc::default(),
    };
    let outcome = session
        .read_loop(BufReader::with_capacity(READ_BUFFER_BYTES, reader))
        .await;
    session.stop();
    let (flushed, done) = oneshot::channel();
    if outgoing.send(Outgoing::Flush(flushed)).is_ok() {
        let _ = done.await;
    }
    writer_task.abort();
    outcome
}

type Inflight = Arc<Mutex<HashMap<RequestId, AbortHandle>>>;

fn lock(inflight: &Inflight) -> MutexGuard<'_, HashMap<RequestId, AbortHandle>> {
    inflight.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Session<E> {
    extension: Arc<E>,
    host: HostClient,
    context: Option<Context>,
    inflight: Inflight,
}

enum Flow {
    Continue,
    Stop,
}

impl<E: Extension> Session<E> {
    async fn read_loop<R: AsyncBufRead + Unpin>(
        &mut self,
        mut reader: R,
    ) -> Result<(), ServeError> {
        let mut buffer = Vec::new();
        loop {
            let Some(line) = read_line(&mut reader, &mut buffer).await? else {
                tracing::debug!("the host closed stdin");
                self.extension.shutdown().await;
                return Ok(());
            };
            let message = match line.and_then(|text| Message::parse(&text)) {
                Ok(message) => message,
                Err(error) => {
                    tracing::warn!(%error, "rejected a malformed message");
                    self.reply(None, Err(error));
                    continue;
                }
            };
            match message {
                Message::Request(request) => {
                    if let Flow::Stop = self.handle_request(request).await? {
                        return Ok(());
                    }
                }
                Message::Notification(notification) => self.handle_notification(&notification),
                Message::Response(response) => self.host.resolve(response),
            }
        }
    }

    fn reply(&self, id: Option<RequestId>, outcome: Result<Value, ErrorObject>) {
        self.host.send(response(id, outcome));
    }

    async fn handle_request(&mut self, request: Request) -> Result<Flow, ServeError> {
        if request.is::<Initialize>() {
            return self.initialize(&request).await;
        }
        let Some(context) = self.context.clone() else {
            self.reply(
                Some(request.id),
                Err(ErrorObject::new(
                    error_code::INVALID_REQUEST,
                    "the backend is not initialized",
                )),
            );
            return Ok(Flow::Continue);
        };
        if request.is::<Shutdown>() {
            self.extension.shutdown().await;
            self.reply(Some(request.id), Ok(Value::Null));
            return Ok(Flow::Stop);
        }
        self.spawn(context, request);
        Ok(Flow::Continue)
    }

    async fn initialize(&mut self, request: &Request) -> Result<Flow, ServeError> {
        let id = Some(request.id.clone());
        if self.context.is_some() {
            self.reply(
                id,
                Err(ErrorObject::new(
                    error_code::INVALID_REQUEST,
                    "the backend is already initialized",
                )),
            );
            return Ok(Flow::Continue);
        }
        let params = match request.params::<Initialize>() {
            Ok(params) => params,
            Err(error) => {
                self.reply(id, Err(error));
                return Ok(Flow::Continue);
            }
        };
        if params.protocol != PROTOCOL_VERSION {
            self.reply(
                id,
                Err(ErrorObject::new(
                    error_code::VERSION_MISMATCH,
                    format!("this backend speaks protocol {PROTOCOL_VERSION}"),
                )),
            );
            return Err(ServeError::ProtocolMismatch(params.protocol));
        }
        let context = Context {
            host: self.host.clone(),
            dirs: Dirs::new(params.dirs.clone()),
            gate: Arc::new(Gate::load(&params.dirs.install)),
            initialize: Arc::new(params),
        };
        if let Err(error) = self.extension.initialize(&context).await {
            self.reply(id, Err(error.clone()));
            return Err(ServeError::Initialize(error));
        }
        let result = InitializeResult {
            protocol: PROTOCOL_VERSION,
            methods: self.extension.methods(),
            topics: self.extension.topics(),
        };
        self.reply(id, encode(&result));
        self.context = Some(context);
        Ok(Flow::Continue)
    }

    fn spawn(&self, context: Context, request: Request) {
        let id = request.id.clone();
        let mut inflight = lock(&self.inflight);
        if inflight.contains_key(&id) {
            drop(inflight);
            self.reply(
                Some(id),
                Err(ErrorObject::new(
                    error_code::INVALID_REQUEST,
                    "a request with this id is already running",
                )),
            );
            return;
        }
        let extension = Arc::clone(&self.extension);
        let host = self.host.clone();
        let registry = Arc::clone(&self.inflight);
        let task_id = id.clone();
        let task = tokio::spawn(async move {
            let outcome = AssertUnwindSafe(dispatch(&*extension, context, &request))
                .catch_unwind()
                .await
                .unwrap_or_else(|_| {
                    Err(ErrorObject::new(
                        error_code::INTERNAL_ERROR,
                        format!("the {} handler panicked", request.method),
                    ))
                });
            if lock(&registry).remove(&task_id).is_some() {
                host.send(response(Some(task_id), outcome));
            }
        });
        inflight.insert(id, task.abort_handle());
    }

    fn handle_notification(&self, notification: &Notification) {
        if notification.is::<Cancel>() {
            match notification.params::<Cancel>() {
                Ok(params) => self.cancel(params.id),
                Err(error) => tracing::warn!(%error, "ignored a malformed cancel"),
            }
        } else if notification.is::<HostTopic>() {
            let Some(context) = self.context.clone() else {
                return;
            };
            match notification.params::<HostTopic>() {
                Ok(params) => {
                    let extension = Arc::clone(&self.extension);
                    tokio::spawn(async move { extension.host_topic(context, params).await });
                }
                Err(error) => tracing::warn!(%error, "ignored a malformed host topic"),
            }
        } else if notification.is::<Initialized>() {
            tracing::debug!("the host finished the handshake");
        } else {
            tracing::debug!(method = %notification.method, "ignored an unknown notification");
        }
    }

    fn cancel(&self, id: RequestId) {
        let Some(handle) = lock(&self.inflight).remove(&id) else {
            return;
        };
        handle.abort();
        self.reply(
            Some(id),
            Err(ErrorObject::new(
                error_code::REQUEST_CANCELLED,
                "the request was cancelled",
            )),
        );
    }

    fn stop(&self) {
        for (_, handle) in lock(&self.inflight).drain() {
            handle.abort();
        }
        self.host.close();
    }
}

async fn dispatch<E: Extension>(
    extension: &E,
    context: Context,
    request: &Request,
) -> Result<Value, ErrorObject> {
    if request.is::<CliRun>() {
        encode(&cli(extension, context, request.params::<CliRun>()?).await?)
    } else if request.is::<RpcCall>() {
        extension
            .rpc_call(context, request.params::<RpcCall>()?)
            .await
    } else if request.is::<ContractRequest>() {
        extension
            .contract_request(context, request.params::<ContractRequest>()?)
            .await
    } else if request.is::<SurfaceAction>() {
        extension
            .surface_action(context, request.params::<SurfaceAction>()?)
            .await
            .map(|()| Value::Null)
    } else if request.is::<ShortcutFired>() {
        extension
            .shortcut_fired(context, request.params::<ShortcutFired>()?)
            .await
            .map(|()| Value::Null)
    } else if request.is::<HealthPing>() {
        Ok(Value::Null)
    } else {
        Err(ErrorObject::method_not_found(&request.method))
    }
}

async fn cli<E: Extension>(
    extension: &E,
    context: Context,
    params: CliRunParams,
) -> Result<CliRunResult, ErrorObject> {
    if params.confirm || !context.gate.needs_confirmation(&params.argv) {
        return extension.cli_run(context, params).await;
    }
    let mut result = extension.cli_preview(context, params).await?;
    if result.exit_code.is_success() {
        result.exit_code = ExitCode::ConfirmationRequired;
    }
    Ok(result)
}

fn encode<T: Serialize>(value: &T) -> Result<Value, ErrorObject> {
    serde_json::to_value(value)
        .map_err(|error| ErrorObject::new(error_code::INTERNAL_ERROR, error.to_string()))
}

fn response(id: Option<RequestId>, outcome: Result<Value, ErrorObject>) -> Message {
    Message::Response(match outcome {
        Ok(value) => Response {
            jsonrpc: JsonRpcVersion,
            id,
            outcome: Outcome::Result(value),
        },
        Err(error) => Response::failure(id, error),
    })
}

async fn read_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
) -> std::io::Result<Option<Result<String, ErrorObject>>> {
    loop {
        buffer.clear();
        let mut oversize = false;
        let mut seen_any = false;
        loop {
            let available = reader.fill_buf().await?;
            if available.is_empty() {
                if !seen_any {
                    return Ok(None);
                }
                break;
            }
            seen_any = true;
            let (chunk, consumed, complete) = match available.iter().position(|byte| *byte == b'\n')
            {
                Some(end) => (&available[..end], end + 1, true),
                None => (available, available.len(), false),
            };
            if !oversize {
                if buffer.len() + chunk.len() > MAX_MESSAGE_BYTES {
                    oversize = true;
                    buffer.clear();
                } else {
                    buffer.extend_from_slice(chunk);
                }
            }
            reader.consume(consumed);
            if complete {
                break;
            }
        }
        if oversize {
            return Ok(Some(Err(ErrorObject::new(
                error_code::INVALID_REQUEST,
                format!("message exceeds {MAX_MESSAGE_BYTES} bytes"),
            ))));
        }
        if buffer.last() == Some(&b'\r') {
            buffer.pop();
        }
        if buffer.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        return Ok(Some(String::from_utf8(std::mem::take(buffer)).map_err(
            |error| ErrorObject::new(error_code::PARSE_ERROR, error.to_string()),
        )));
    }
}

async fn write_loop<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut receiver: mpsc::UnboundedReceiver<Outgoing>,
) {
    while let Some(item) = receiver.recv().await {
        match item {
            Outgoing::Message(message) => {
                let Some(line) = encode_line(message) else {
                    continue;
                };
                if writer.write_all(line.as_bytes()).await.is_err() || writer.flush().await.is_err()
                {
                    tracing::error!("the host stopped reading stdout");
                    return;
                }
            }
            Outgoing::Flush(done) => {
                let _ = writer.flush().await;
                let _ = done.send(());
            }
        }
    }
}

fn encode_line(message: Message) -> Option<String> {
    let line = match message.to_line() {
        Ok(line) => line,
        Err(error) => {
            tracing::error!(%error, "could not encode a message");
            return None;
        }
    };
    if line.len() <= MAX_MESSAGE_BYTES {
        return Some(line);
    }
    let Message::Response(response) = message else {
        tracing::error!("dropped a message over {MAX_MESSAGE_BYTES} bytes");
        return None;
    };
    let error = ErrorObject::new(
        error_code::INTERNAL_ERROR,
        format!("the result exceeds {MAX_MESSAGE_BYTES} bytes; pass large data by file path"),
    );
    Message::Response(Response::failure(response.id, error))
        .to_line()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn lines(input: &[u8]) -> Vec<Result<String, i64>> {
        let mut reader = BufReader::with_capacity(4, input);
        let mut buffer = Vec::new();
        let mut out = Vec::new();
        while let Some(line) = read_line(&mut reader, &mut buffer).await.unwrap() {
            out.push(line.map_err(|error| error.code));
        }
        out
    }

    #[tokio::test]
    async fn lines_split_on_newlines_and_skip_blank_ones() {
        assert_eq!(
            lines(b"one\r\n\n  \ntwo\nthree").await,
            vec![Ok("one".into()), Ok("two".into()), Ok("three".into())]
        );
    }

    #[tokio::test]
    async fn invalid_utf8_is_a_parse_error() {
        assert_eq!(
            lines(b"\xff\xfe\nok\n").await,
            vec![Err(error_code::PARSE_ERROR), Ok("ok".into())]
        );
    }

    #[tokio::test]
    async fn an_oversize_line_is_rejected_and_reading_continues() {
        let mut input = vec![b'x'; MAX_MESSAGE_BYTES + 1];
        input.extend_from_slice(b"\nnext\n");
        assert_eq!(
            lines(&input).await,
            vec![Err(error_code::INVALID_REQUEST), Ok("next".into())]
        );
    }

    #[test]
    fn an_oversize_result_becomes_an_error_response() {
        let big = Value::String("x".repeat(MAX_MESSAGE_BYTES));
        let line = encode_line(response(Some(RequestId::Number(4)), Ok(big))).unwrap();
        let parsed = Message::parse(line.trim_end()).unwrap();
        let Message::Response(response) = parsed else {
            panic!("expected a response");
        };
        assert_eq!(response.id, Some(RequestId::Number(4)));
        assert!(
            matches!(response.outcome, Outcome::Error(error) if error.code == error_code::INTERNAL_ERROR)
        );
    }
}
