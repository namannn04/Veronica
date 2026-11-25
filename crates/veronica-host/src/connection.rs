use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use rustix::process::{kill_process_group, Pid, Signal};
use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdout, Command};
use tokio::sync::{mpsc, oneshot, watch};
use veronica_core::host::is_sandboxed;
use veronica_core::session::DesktopSession;
use veronica_protocol::rpc::{
    error_code, Cancel, CancelParams, ErrorObject, ExtensionDirs, ExtensionInfo, HealthPing,
    HostInfo, Initialize, InitializeParams, InitializeResult, Initialized, JsonRpcVersion, Message,
    Notification, NotificationMethod, Outcome, Request, RequestId, RequestMethod, Response,
    RunMode, SessionInfo, Shutdown, MAX_MESSAGE_BYTES, PROTOCOL_VERSION,
};
use veronica_protocol::{ExitCode, Manifest};

use crate::host_api::{Grant, HostServices};
use crate::install::HOST_API;
use crate::layout::Layout;
use crate::registry::Registry;

pub const EXTENSION_ID_ENV: &str = "VERONICA_EXT_ID";
const EXITED_READER_GRACE: Duration = Duration::from_secs(2);
const ORPHAN_GRACE: Duration = Duration::from_secs(2);
const PASSED_ENV: [&str; 31] = [
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_CTYPE",
    "LC_MESSAGES",
    "TZ",
    "TERM",
    "NO_COLOR",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "XDG_CACHE_HOME",
    "XDG_RUNTIME_DIR",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "DBUS_SESSION_BUS_ADDRESS",
    "SSH_AUTH_SOCK",
    "FLATPAK_ID",
    "SNAP",
    "APPIMAGE",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "VERONICA_LOG",
];

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("{0} is not installed")]
    NotInstalled(String),
    #[error("{0} is disabled")]
    Disabled(String),
    #[error("{0} has no backend")]
    NoBackend(String),
    #[error("{0} keeps crashing and was stopped; restart it to try again")]
    CrashLooping(String),
    #[error("the backend could not start: {0}")]
    Launch(String),
    #[error("the backend exited ({0})")]
    Exited(String),
    #[error("the backend did not answer within {0:?}")]
    Timeout(Duration),
    #[error("the backend broke the protocol: {0}")]
    Protocol(String),
    #[error("{0}")]
    Rpc(ErrorObject),
}

impl BackendError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::NotInstalled(_)
            | Self::Disabled(_)
            | Self::NoBackend(_)
            | Self::CrashLooping(_) => ExitCode::Unavailable,
            Self::Timeout(_) => ExitCode::Timeout,
            Self::Rpc(error) => match error.code {
                error_code::PERMISSION_DENIED => ExitCode::PermissionDenied,
                error_code::UNAVAILABLE => ExitCode::Unavailable,
                error_code::TIMEOUT => ExitCode::Timeout,
                error_code::INVALID_PARAMS | error_code::METHOD_NOT_FOUND => ExitCode::Usage,
                error_code::REQUEST_CANCELLED => ExitCode::Interrupted,
                _ => ExitCode::Failed,
            },
            Self::Launch(_) | Self::Exited(_) | Self::Protocol(_) => ExitCode::BackendFailed,
        }
    }
}

pub fn describe_exit(status: Option<ExitStatus>) -> String {
    match status {
        Some(status) => match (status.code(), status.signal()) {
            (Some(code), _) => format!("exit code {code}"),
            (None, Some(signal)) => format!("signal {signal}"),
            (None, None) => "unknown status".to_string(),
        },
        None => "still running".to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct Launch {
    pub id: String,
    pub entry: PathBuf,
    pub enabled: bool,
    pub manifest: Manifest,
    pub log: Option<PathBuf>,
    pub initialize: InitializeParams,
}

fn extension_root(path: &Path) -> &Path {
    path.parent().unwrap_or(path)
}

fn session() -> SessionInfo {
    let env = |key: &str| std::env::var(key).unwrap_or_default();
    let kind = DesktopSession::classify(
        &env("XDG_SESSION_TYPE"),
        &env("WAYLAND_DISPLAY"),
        &env("DISPLAY"),
    );
    let desktop = DesktopSession::primary_desktop(&env("XDG_CURRENT_DESKTOP"));
    let packaging = if is_sandboxed() {
        "flatpak"
    } else if std::env::var_os("SNAP").is_some() {
        "snap"
    } else if std::env::var_os("APPIMAGE").is_some() {
        "appimage"
    } else {
        "native"
    };
    SessionInfo {
        kind: serde_json::to_value(kind)
            .ok()
            .and_then(|value| value.as_str().map(String::from))
            .unwrap_or_default(),
        desktop: (!desktop.is_empty()).then_some(desktop),
        packaging: packaging.to_string(),
    }
}

fn locale() -> String {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .ok()
        .and_then(|raw| {
            raw.split(['.', '@'])
                .next()
                .map(|tag| tag.replace('_', "-"))
        })
        .filter(|tag| !tag.is_empty() && tag != "C" && tag != "POSIX")
        .unwrap_or_else(|| "en".to_string())
}

impl Launch {
    pub fn prepare(layout: &Layout, id: &str, mode: RunMode) -> Result<Self, BackendError> {
        let registry = Registry::load(&layout.installed_file())
            .map_err(|error| BackendError::Launch(format!("{error:#}")))?;
        let entry = registry
            .get(id)
            .ok_or_else(|| BackendError::NotInstalled(id.to_string()))?;
        let install = fs::canonicalize(layout.current_link(id))
            .map_err(|_| BackendError::NotInstalled(id.to_string()))?;
        let raw = fs::read_to_string(install.join("manifest.json"))
            .map_err(|error| BackendError::Launch(error.to_string()))?;
        let manifest =
            Manifest::parse(&raw).map_err(|error| BackendError::Launch(error.to_string()))?;
        let backend = manifest
            .backend
            .as_ref()
            .ok_or_else(|| BackendError::NoBackend(id.to_string()))?;
        let scoped = |root: PathBuf| root.join("ext").join(id);
        let dirs = ExtensionDirs {
            install: install.clone(),
            data: layout.data_dir(id),
            cache: scoped(extension_root(&layout.downloads_dir()).to_path_buf()),
            state: scoped(extension_root(&layout.installed_file()).to_path_buf()),
            runtime: scoped(extension_root(&layout.lock_file()).join("extensions")),
        };
        for dir in [&dirs.data, &dirs.cache, &dirs.state, &dirs.runtime] {
            fs::create_dir_all(dir).map_err(|error| BackendError::Launch(error.to_string()))?;
        }
        let initialize = InitializeParams {
            protocol: PROTOCOL_VERSION,
            host: HostInfo {
                version: env!("CARGO_PKG_VERSION").to_string(),
                api_version: HOST_API,
            },
            extension: ExtensionInfo {
                id: id.to_string(),
                version: entry.version.to_string(),
            },
            mode,
            dirs,
            granted: manifest.permissions.clone(),
            session: session(),
            locale: locale(),
        };
        Ok(Self {
            id: id.to_string(),
            entry: install.join(&backend.entry),
            enabled: entry.enabled,
            log: (mode == RunMode::Service).then(|| layout.log_file(id)),
            manifest,
            initialize,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopStep {
    AlreadyExited,
    Shutdown,
    Terminated,
    Killed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stopped {
    pub step: StopStep,
    pub status: Option<ExitStatus>,
}

type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Outcome>>>>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Connection {
    id: String,
    pid: Option<Pid>,
    outgoing: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_id: AtomicI64,
    exit: watch::Receiver<Option<ExitStatus>>,
    last_activity: Mutex<Instant>,
    retired: AtomicBool,
    initialized: InitializeResult,
}

struct PendingGuard<'a> {
    connection: &'a Connection,
    id: i64,
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        let connection = self.connection;
        if lock(&connection.pending).remove(&self.id).is_some() {
            let params = CancelParams {
                id: RequestId::Number(self.id),
            };
            let _ = connection.notify::<Cancel>(&params);
        }
        connection.touch();
    }
}

fn signal_group(pid: Option<Pid>, signal: Signal) {
    if let Some(pid) = pid {
        let _ = kill_process_group(pid, signal);
    }
}

fn spawn_failed(error: impl std::fmt::Display) -> BackendError {
    BackendError::Launch(error.to_string())
}

impl Connection {
    pub async fn start(
        launch: Launch,
        services: Arc<HostServices>,
        timeout: Duration,
    ) -> Result<Arc<Self>, BackendError> {
        let mut command = Command::new(&launch.entry);
        command
            .env_clear()
            .envs(
                PASSED_ENV
                    .iter()
                    .filter_map(|key| Some((key, std::env::var_os(key)?))),
            )
            .env(EXTENSION_ID_ENV, &launch.id)
            .current_dir(&launch.initialize.dirs.data)
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(match &launch.log {
                Some(path) => {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent).map_err(spawn_failed)?;
                    }
                    let file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .map_err(spawn_failed)?;
                    Stdio::from(file)
                }
                None => Stdio::inherit(),
            });
        let mut child = command
            .spawn()
            .map_err(|error| spawn_failed(format!("{}: {error}", launch.entry.display())))?;
        let pid = child
            .id()
            .and_then(|raw| i32::try_from(raw).ok())
            .and_then(Pid::from_raw);
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            signal_group(pid, Signal::KILL);
            return Err(spawn_failed("the backend's pipes are missing"));
        };
        let (outgoing, mut lines) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(line) = lines.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                    return;
                }
            }
        });
        let pending = Pending::default();
        let grant = Grant {
            id: launch.id.clone(),
            permissions: launch.initialize.granted.clone(),
        };
        let reader = tokio::spawn(read_loop(
            stdout,
            outgoing.downgrade(),
            Arc::clone(&pending),
            services,
            grant,
            pid,
        ));
        let (exit_sender, exit) = watch::channel(None);
        let waiter_pending = Arc::clone(&pending);
        tokio::spawn(async move {
            let status = child.wait().await.ok();
            let _ = tokio::time::timeout(EXITED_READER_GRACE, reader).await;
            lock(&waiter_pending).clear();
            let _ = exit_sender.send(Some(status.unwrap_or_else(|| ExitStatus::from_raw(-1))));
        });
        let mut connection = Self {
            id: launch.id,
            pid,
            outgoing,
            pending,
            next_id: AtomicI64::new(1),
            exit,
            last_activity: Mutex::new(Instant::now()),
            retired: AtomicBool::new(false),
            initialized: InitializeResult {
                protocol: PROTOCOL_VERSION,
                methods: Vec::new(),
                topics: Vec::new(),
            },
        };
        let handshake = async {
            let result = connection
                .request::<Initialize>(&launch.initialize, Some(timeout))
                .await?;
            if result.protocol != PROTOCOL_VERSION {
                return Err(BackendError::Protocol(format!(
                    "the backend speaks protocol {}, the host speaks {PROTOCOL_VERSION}",
                    result.protocol
                )));
            }
            connection.notify::<Initialized>(&())?;
            Ok(result)
        };
        match handshake.await {
            Ok(result) => {
                connection.initialized = result;
                Ok(Arc::new(connection))
            }
            Err(error) => {
                connection.retire();
                signal_group(pid, Signal::KILL);
                Err(error)
            }
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
            .map(|pid| pid.as_raw_nonzero().get().unsigned_abs())
    }

    pub fn initialized(&self) -> &InitializeResult {
        &self.initialized
    }

    pub fn exit_status(&self) -> Option<ExitStatus> {
        *self.exit.borrow()
    }

    pub fn is_alive(&self) -> bool {
        self.exit_status().is_none()
    }

    pub fn inflight(&self) -> usize {
        lock(&self.pending).len()
    }

    pub fn idle_for(&self) -> Duration {
        lock(&self.last_activity).elapsed()
    }

    fn touch(&self) {
        *lock(&self.last_activity) = Instant::now();
    }

    pub(crate) fn retire(&self) {
        self.retired.store(true, Ordering::SeqCst);
    }

    pub(crate) fn is_retired(&self) -> bool {
        self.retired.load(Ordering::SeqCst)
    }

    pub async fn exited(&self) -> Option<ExitStatus> {
        let mut exit = self.exit.clone();
        let status = exit.wait_for(Option::is_some).await.ok()?;
        *status
    }

    async fn wait_exit(&self, timeout: Duration) -> Option<ExitStatus> {
        tokio::time::timeout(timeout, self.exited())
            .await
            .ok()
            .flatten()
    }

    pub fn notify<M: NotificationMethod>(&self, params: &M::Params) -> Result<(), BackendError> {
        let notification = Notification::new::<M>(params)
            .map_err(|error| BackendError::Protocol(error.to_string()))?;
        self.send(&Message::Notification(notification))
    }

    fn send(&self, message: &Message) -> Result<(), BackendError> {
        let line = message
            .to_line()
            .map_err(|error| BackendError::Protocol(error.to_string()))?;
        if line.len() > MAX_MESSAGE_BYTES {
            return Err(BackendError::Protocol(format!(
                "a message exceeds {MAX_MESSAGE_BYTES} bytes; pass large data by file path"
            )));
        }
        self.outgoing
            .send(line)
            .map_err(|_| BackendError::Exited(describe_exit(self.exit_status())))
    }

    pub async fn request<M: RequestMethod>(
        &self,
        params: &M::Params,
        timeout: Option<Duration>,
    ) -> Result<M::Result, BackendError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = Request::new::<M>(RequestId::Number(id), params)
            .map_err(|error| BackendError::Protocol(error.to_string()))?;
        let (sender, receiver) = oneshot::channel();
        lock(&self.pending).insert(id, sender);
        self.touch();
        let guard = PendingGuard {
            connection: self,
            id,
        };
        self.send(&Message::Request(request))?;
        let answer = match timeout {
            Some(limit) => tokio::time::timeout(limit, receiver)
                .await
                .map_err(|_| BackendError::Timeout(limit))?,
            None => receiver.await,
        };
        drop(guard);
        match answer {
            Ok(Outcome::Result(value)) => M::Result::deserialize(value)
                .map_err(|error| BackendError::Protocol(error.to_string())),
            Ok(Outcome::Error(error)) => Err(BackendError::Rpc(error)),
            Err(_) => {
                let status = self.wait_exit(EXITED_READER_GRACE).await;
                Err(BackendError::Exited(describe_exit(status)))
            }
        }
    }

    pub async fn ping(&self, timeout: Duration) -> Result<(), BackendError> {
        self.request::<HealthPing>(&(), Some(timeout)).await
    }

    pub async fn stop(&self, shutdown_grace: Duration, terminate_grace: Duration) -> Stopped {
        if let Some(status) = self.exit_status() {
            return Stopped {
                step: StopStep::AlreadyExited,
                status: Some(status),
            };
        }
        let deadline = Instant::now() + shutdown_grace;
        let _ = self.request::<Shutdown>(&(), Some(shutdown_grace)).await;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if let Some(status) = self.wait_exit(remaining).await {
            return Stopped {
                step: StopStep::Shutdown,
                status: Some(status),
            };
        }
        signal_group(self.pid, Signal::TERM);
        if let Some(status) = self.wait_exit(terminate_grace).await {
            return Stopped {
                step: StopStep::Terminated,
                status: Some(status),
            };
        }
        signal_group(self.pid, Signal::KILL);
        Stopped {
            step: StopStep::Killed,
            status: self.exited().await,
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if !self.is_alive() {
            return;
        }
        let pid = self.pid;
        let mut exit = self.exit.clone();
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(async move {
                    let exited = tokio::time::timeout(ORPHAN_GRACE, exit.wait_for(Option::is_some));
                    if !matches!(exited.await, Ok(Ok(_))) {
                        signal_group(pid, Signal::KILL);
                    }
                });
            }
            Err(_) => signal_group(pid, Signal::KILL),
        }
    }
}

async fn read_line(
    reader: &mut BufReader<ChildStdout>,
    buffer: &mut Vec<u8>,
) -> Result<Option<Message>, BackendError> {
    loop {
        buffer.clear();
        let limit = u64::try_from(MAX_MESSAGE_BYTES).unwrap_or(u64::MAX) + 1;
        let read = (&mut *reader)
            .take(limit)
            .read_until(b'\n', buffer)
            .await
            .map_err(|error| BackendError::Protocol(error.to_string()))?;
        if read == 0 {
            return Ok(None);
        }
        if buffer.len() > MAX_MESSAGE_BYTES {
            return Err(BackendError::Protocol(format!(
                "a message exceeds {MAX_MESSAGE_BYTES} bytes"
            )));
        }
        let text = std::str::from_utf8(buffer)
            .map_err(|error| BackendError::Protocol(error.to_string()))?
            .trim();
        if text.is_empty() {
            continue;
        }
        return Message::parse(text)
            .map(Some)
            .map_err(|error| BackendError::Protocol(error.message));
    }
}

fn reply(
    outgoing: &mpsc::WeakUnboundedSender<String>,
    id: RequestId,
    outcome: Result<Value, ErrorObject>,
) {
    let message = Message::Response(match outcome {
        Ok(value) => Response {
            jsonrpc: JsonRpcVersion,
            id: Some(id),
            outcome: Outcome::Result(value),
        },
        Err(error) => Response::failure(Some(id), error),
    });
    let line = message.to_line().unwrap_or_default();
    if let Some(outgoing) = outgoing.upgrade() {
        let _ = outgoing.send(line);
    }
}

async fn read_loop(
    stdout: ChildStdout,
    outgoing: mpsc::WeakUnboundedSender<String>,
    pending: Pending,
    services: Arc<HostServices>,
    grant: Grant,
    pid: Option<Pid>,
) {
    let mut reader = BufReader::new(stdout);
    let mut buffer = Vec::new();
    loop {
        let message = match read_line(&mut reader, &mut buffer).await {
            Ok(Some(message)) => message,
            Ok(None) => return,
            Err(error) => {
                services.log(&grant.id, &format!("stopping the backend: {error}"));
                signal_group(pid, Signal::KILL);
                return;
            }
        };
        match message {
            Message::Response(response) => {
                let waiter = match &response.id {
                    Some(RequestId::Number(id)) => lock(&pending).remove(id),
                    _ => None,
                };
                if let Some(waiter) = waiter {
                    let _ = waiter.send(response.outcome);
                }
            }
            Message::Request(request) if HostServices::answers_later(&request) => {
                let services = Arc::clone(&services);
                let grant = grant.clone();
                let outgoing = outgoing.clone();
                tokio::spawn(async move {
                    let outcome = services.handle_later(&grant, &request).await;
                    reply(&outgoing, request.id, outcome);
                });
            }
            Message::Request(request) => {
                let outcome = services.handle(&grant, &request);
                reply(&outgoing, request.id, outcome);
            }
            Message::Notification(notification) => services.notification(&grant, &notification),
        }
    }
}
