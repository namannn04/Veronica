use std::collections::{HashMap, VecDeque};
use std::fs;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::process::ExitStatus;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use veronica_core::fsutil::write_atomic;
use veronica_protocol::manifest::ActivationMode;
use veronica_protocol::rpc::RunMode;

use crate::connection::{describe_exit, BackendError, Connection, Launch, Stopped};
use crate::host_api::HostServices;
use crate::layout::Layout;

const DEFAULT_IDLE_STOP_SECONDS: u32 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub start_timeout: Duration,
    pub health_interval: Duration,
    pub health_timeout: Duration,
    pub backoff_min: Duration,
    pub backoff_max: Duration,
    pub crash_window: Duration,
    pub crash_limit: usize,
    pub shutdown_grace: Duration,
    pub terminate_grace: Duration,
    pub idle_check: Duration,
    pub stagger: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            start_timeout: Duration::from_secs(10),
            health_interval: Duration::from_secs(30),
            health_timeout: Duration::from_secs(5),
            backoff_min: Duration::from_millis(500),
            backoff_max: Duration::from_mins(1),
            crash_window: Duration::from_mins(2),
            crash_limit: 5,
            shutdown_grace: Duration::from_secs(3),
            terminate_grace: Duration::from_secs(2),
            idle_check: Duration::from_secs(1),
            stagger: Duration::from_millis(250),
        }
    }
}

impl Policy {
    pub fn backoff(&self, attempt: usize) -> Duration {
        let exponent = u32::try_from(attempt.saturating_sub(1))
            .unwrap_or(u32::MAX)
            .min(16);
        let base = self
            .backoff_min
            .saturating_mul(1 << exponent)
            .min(self.backoff_max);
        base.mul_f64(0.75 + fastrand::f64() * 0.5)
            .clamp(self.backoff_min, self.backoff_max)
    }

    pub async fn stop(&self, connection: &Connection) -> Stopped {
        connection
            .stop(self.shutdown_grace, self.terminate_grace)
            .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashRecord {
    pub crash_looping: bool,
    pub exits: usize,
    pub recorded_at: String,
    pub last_exit: String,
}

impl CrashRecord {
    pub fn load(layout: &Layout, id: &str) -> Option<Self> {
        let raw = fs::read(layout.crash_file(id)).ok()?;
        serde_json::from_slice(&raw).ok()
    }

    pub fn clear(layout: &Layout, id: &str) -> io::Result<()> {
        match fs::remove_file(layout.crash_file(id)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Stopped,
    Running { pid: Option<u32> },
    CrashLooping,
}

pub async fn verify_starts(services: &Arc<HostServices>, id: &str) -> Result<(), BackendError> {
    let policy = Policy::default();
    let launch = Launch::prepare(services.layout(), id, RunMode::Service)?;
    let connection = Connection::start(launch, Arc::clone(services), policy.start_timeout).await?;
    let health = connection.ping(policy.health_timeout).await;
    connection.retire();
    policy.stop(&connection).await;
    health
}

type Launching<'a> =
    Pin<Box<dyn Future<Output = Result<Arc<Connection>, BackendError>> + Send + 'a>>;

#[derive(Default)]
struct SlotState {
    connection: Option<Arc<Connection>>,
    generation: u64,
    exits: VecDeque<Instant>,
    crash_looping: bool,
}

#[derive(Default)]
struct Slot {
    state: tokio::sync::Mutex<SlotState>,
}

struct Inner {
    services: Arc<HostServices>,
    policy: Policy,
    slots: Mutex<HashMap<String, Arc<Slot>>>,
}

#[derive(Clone)]
pub struct Supervisor {
    inner: Arc<Inner>,
}

struct Watch {
    id: String,
    slot: Arc<Slot>,
    connection: Arc<Connection>,
    generation: u64,
    background: bool,
    idle_stop: Option<Duration>,
}

impl Supervisor {
    pub fn new(services: Arc<HostServices>) -> Self {
        Self::with_policy(services, Policy::default())
    }

    pub fn with_policy(services: Arc<HostServices>, policy: Policy) -> Self {
        Self {
            inner: Arc::new(Inner {
                services,
                policy,
                slots: Mutex::default(),
            }),
        }
    }

    pub fn services(&self) -> &Arc<HostServices> {
        &self.inner.services
    }

    pub fn policy(&self) -> &Policy {
        &self.inner.policy
    }

    fn layout(&self) -> &Layout {
        self.inner.services.layout()
    }

    fn slot(&self, id: &str) -> Arc<Slot> {
        let mut slots = self
            .inner
            .slots
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Arc::clone(slots.entry(id.to_string()).or_default())
    }

    pub async fn status(&self, id: &str) -> Status {
        let slot = self.slot(id);
        let state = slot.state.lock().await;
        if state.crash_looping
            || CrashRecord::load(self.layout(), id).is_some_and(|record| record.crash_looping)
        {
            return Status::CrashLooping;
        }
        match &state.connection {
            Some(connection) if connection.is_alive() => Status::Running {
                pid: connection.pid(),
            },
            _ => Status::Stopped,
        }
    }

    pub async fn running(&self, id: &str) -> Option<Arc<Connection>> {
        let slot = self.slot(id);
        let state = slot.state.lock().await;
        state
            .connection
            .as_ref()
            .filter(|connection| connection.is_alive())
            .cloned()
    }

    pub async fn connection(&self, id: &str) -> Result<Arc<Connection>, BackendError> {
        let slot = self.slot(id);
        let mut state = slot.state.lock().await;
        if let Some(connection) = &state.connection {
            if connection.is_alive() {
                return Ok(Arc::clone(connection));
            }
        }
        if state.crash_looping
            || CrashRecord::load(self.layout(), id).is_some_and(|record| record.crash_looping)
        {
            state.crash_looping = true;
            return Err(BackendError::CrashLooping(id.to_string()));
        }
        match self.launch(id, &slot, &mut state).await {
            Ok(connection) => Ok(connection),
            Err(error) => {
                self.record_exit(id, &mut state, &error.to_string());
                Err(error)
            }
        }
    }

    pub async fn start(&self, id: &str) -> Result<(), BackendError> {
        self.connection(id).await.map(drop)
    }

    pub async fn start_background(&self) -> Vec<(String, BackendError)> {
        let registry = match crate::registry::Registry::load(&self.layout().installed_file()) {
            Ok(registry) => registry,
            Err(error) => return vec![(String::new(), BackendError::Launch(format!("{error:#}")))],
        };
        let mut failures = Vec::new();
        for (id, entry) in &registry.extensions {
            if !entry.enabled {
                continue;
            }
            let background = Launch::prepare(self.layout(), id, RunMode::Service)
                .is_ok_and(|launch| launch.manifest.activation.mode == ActivationMode::Background);
            if !background {
                continue;
            }
            if let Err(error) = self.start(id).await {
                failures.push((id.clone(), error));
            }
            tokio::time::sleep(self.inner.policy.stagger).await;
        }
        failures
    }

    pub async fn stop(&self, id: &str) -> Option<Stopped> {
        let slot = self.slot(id);
        let connection = {
            let mut state = slot.state.lock().await;
            state.generation += 1;
            state.connection.take()
        }?;
        connection.retire();
        Some(self.inner.policy.stop(&connection).await)
    }

    pub async fn stop_all(&self) {
        let ids: Vec<String> = self
            .inner
            .slots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect();
        let mut stops = tokio::task::JoinSet::new();
        for id in ids {
            let supervisor = self.clone();
            stops.spawn(async move { supervisor.stop(&id).await });
        }
        while stops.join_next().await.is_some() {}
    }

    pub async fn restart(&self, id: &str) -> Result<(), BackendError> {
        self.stop(id).await;
        {
            let slot = self.slot(id);
            let mut state = slot.state.lock().await;
            state.crash_looping = false;
            state.exits.clear();
        }
        CrashRecord::clear(self.layout(), id)
            .map_err(|error| BackendError::Launch(error.to_string()))?;
        self.start(id).await
    }

    fn launch<'a>(
        &'a self,
        id: &'a str,
        slot: &'a Arc<Slot>,
        state: &'a mut SlotState,
    ) -> Launching<'a> {
        Box::pin(self.launch_now(id, slot, state))
    }

    async fn launch_now(
        &self,
        id: &str,
        slot: &Arc<Slot>,
        state: &mut SlotState,
    ) -> Result<Arc<Connection>, BackendError> {
        let launch = Launch::prepare(self.layout(), id, RunMode::Service)?;
        if !launch.enabled {
            return Err(BackendError::Disabled(id.to_string()));
        }
        let activation = launch.manifest.activation.clone();
        let connection = Connection::start(
            launch,
            Arc::clone(&self.inner.services),
            self.inner.policy.start_timeout,
        )
        .await?;
        state.generation += 1;
        state.connection = Some(Arc::clone(&connection));
        let background = activation.mode == ActivationMode::Background;
        let idle_seconds = activation
            .idle_stop_seconds
            .unwrap_or(DEFAULT_IDLE_STOP_SECONDS);
        let watch = Watch {
            id: id.to_string(),
            slot: Arc::clone(slot),
            connection: Arc::clone(&connection),
            generation: state.generation,
            background,
            idle_stop: (!background && idle_seconds > 0)
                .then(|| Duration::from_secs(u64::from(idle_seconds))),
        };
        tokio::spawn(self.clone().watch(watch));
        Ok(connection)
    }

    async fn watch(self, watch: Watch) {
        let policy = &self.inner.policy;
        let connection = &watch.connection;
        let mut health = tokio::time::interval(policy.health_interval);
        health.tick().await;
        let status = loop {
            tokio::select! {
                status = connection.exited() => break status,
                () = tokio::time::sleep(policy.idle_check), if watch.idle_stop.is_some() => {
                    let idle = watch.idle_stop.unwrap_or(Duration::MAX);
                    if connection.inflight() == 0 && connection.idle_for() >= idle
                        && self.stop_generation(&watch).await
                    {
                        return;
                    }
                }
                _ = health.tick(), if watch.background => {
                    if connection.ping(policy.health_timeout).await.is_err() {
                        break policy.stop(connection).await.status;
                    }
                }
            }
        };
        if connection.is_retired() {
            return;
        }
        self.recover(&watch, status).await;
    }

    async fn stop_generation(&self, watch: &Watch) -> bool {
        {
            let mut state = watch.slot.state.lock().await;
            if state.generation != watch.generation || watch.connection.inflight() > 0 {
                return false;
            }
            state.generation += 1;
            state.connection = None;
            watch.connection.retire();
        }
        self.inner.policy.stop(&watch.connection).await;
        true
    }

    fn record_exit(&self, id: &str, state: &mut SlotState, last_exit: &str) -> bool {
        let policy = &self.inner.policy;
        let now = Instant::now();
        state.exits.push_back(now);
        while state
            .exits
            .front()
            .is_some_and(|exit| now.duration_since(*exit) > policy.crash_window)
        {
            state.exits.pop_front();
        }
        if state.exits.len() < policy.crash_limit {
            return false;
        }
        state.crash_looping = true;
        let record = CrashRecord {
            crash_looping: true,
            exits: state.exits.len(),
            recorded_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            last_exit: last_exit.to_string(),
        };
        if let Ok(mut body) = serde_json::to_vec_pretty(&record) {
            body.push(b'\n');
            let _ = write_atomic(&self.layout().crash_file(id), &body);
        }
        self.inner.services.log(
            id,
            &format!(
                "crash looping after {} exits, restarts stopped",
                record.exits
            ),
        );
        true
    }

    async fn recover(&self, watch: &Watch, status: Option<ExitStatus>) {
        let mut last_exit = describe_exit(status);
        loop {
            let delay = {
                let mut state = watch.slot.state.lock().await;
                if state.generation != watch.generation {
                    return;
                }
                state.connection = None;
                self.inner
                    .services
                    .log(&watch.id, &format!("the backend exited ({last_exit})"));
                if self.record_exit(&watch.id, &mut state, &last_exit) || !watch.background {
                    return;
                }
                self.inner.policy.backoff(state.exits.len())
            };
            tokio::time::sleep(delay).await;
            let mut state = watch.slot.state.lock().await;
            if state.generation != watch.generation || state.crash_looping {
                return;
            }
            match self.launch(&watch.id, &watch.slot, &mut state).await {
                Ok(_) => return,
                Err(error) => last_exit = error.to_string(),
            }
        }
    }
}
