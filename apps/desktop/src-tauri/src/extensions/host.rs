use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::oneshot;
use veronica_core::AppDirectories;
use veronica_host::connection::BackendError;
use veronica_host::contracts::{consumers, topic};
use veronica_host::host_api::{Frontend, HostServices};
use veronica_host::registry::{InstalledExtension, Registry};
use veronica_host::surfaces::Surface;
use veronica_host::{HostIdentity, Installer, Layout, Router, Supervisor};
use veronica_protocol::rpc::{
    error_code, ErrorObject, HostTopic, NotifyParams, ProgressParams, TopicParams,
};
use veronica_protocol::{ContractId, Manifest};

use super::contracts::{enabled_manifests, HostTopics, Routing};
use super::files::FileTokens;
use super::jobs::Jobs;
use super::storage::Storage;

pub(crate) const EVENT: &str = "extension-event";
pub(crate) const PROGRESS: &str = "extension-progress";
pub(crate) const SURFACE: &str = "extension-surface";
pub(crate) const INTERFACE: &str = "extension-interface";
const STORAGE_FILE: &str = "interface-storage.json";

pub(crate) type Pending<T> = Pin<Box<dyn Future<Output = T> + Send>>;

pub(crate) trait Shell: Send + Sync {
    fn emit(&self, event: &str, payload: Value);

    fn notify(&self, extension: &str, params: NotifyParams) -> Result<(), ErrorObject>;

    fn open_external(&self, url: &str) -> Result<(), String>;

    fn copy(&self, text: String) -> Pending<Result<(), String>>;

    fn push_surfaces(&self, _surfaces: Vec<Surface>) {}
}

type Topics = BTreeMap<String, BTreeMap<String, BTreeSet<String>>>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct Desk {
    shell: OnceLock<Arc<dyn Shell>>,
    services: OnceLock<Weak<HostServices>>,
    supervisor: OnceLock<Supervisor>,
    topics: Mutex<Topics>,
}

impl Desk {
    fn emit(&self, event: &str, payload: Value) {
        if let Some(shell) = self.shell.get() {
            shell.emit(event, payload);
        }
    }

    fn shell(&self) -> Result<&Arc<dyn Shell>, ErrorObject> {
        self.shell.get().ok_or_else(|| {
            ErrorObject::new(error_code::UNAVAILABLE, "the desktop is still starting")
        })
    }

    fn push_surfaces(&self, extension: &str) {
        self.emit(SURFACE, json!({ "extension": extension }));
        let services = self.services.get().and_then(Weak::upgrade);
        if let (Some(shell), Some(services)) = (self.shell.get(), services) {
            shell.push_surfaces(services.surfaces().list());
        }
    }

    fn deliver(&self, contract: &ContractId, payload: &Value) {
        let topic = topic(contract);
        let listeners: Vec<(String, Vec<String>)> = lock(&self.topics)
            .iter()
            .map(|(extension, pages)| {
                let pages = pages
                    .iter()
                    .filter(|(_, topics)| topics.contains(&topic))
                    .map(|(page, _)| page.clone())
                    .collect::<Vec<_>>();
                (extension.clone(), pages)
            })
            .filter(|(_, pages)| !pages.is_empty())
            .collect();
        for (extension, pages) in listeners {
            self.emit(
                EVENT,
                json!({ "extension": extension, "pages": pages, "topic": topic, "payload": payload }),
            );
        }
        let (Some(supervisor), Some(services)) = (
            self.supervisor.get().cloned(),
            self.services.get().and_then(Weak::upgrade),
        ) else {
            return;
        };
        let manifests = enabled_manifests(services.layout());
        let readers = consumers(&manifests, contract);
        if readers.is_empty() {
            return;
        }
        let params = TopicParams {
            topic,
            payload: payload.clone(),
        };
        tauri::async_runtime::spawn(async move {
            for reader in readers {
                if let Some(connection) = supervisor.running(&reader).await {
                    let _ = connection.notify::<HostTopic>(&params);
                }
            }
        });
    }
}

impl Frontend for Desk {
    fn notify(&self, extension: &str, params: NotifyParams) -> Result<(), ErrorObject> {
        self.shell()?.notify(extension, params)
    }

    fn accepts_surfaces(&self) -> bool {
        self.shell.get().is_some()
    }

    fn surfaces_changed(&self, extension: &str) {
        self.push_surfaces(extension);
    }

    fn contract_published(&self, _extension: &str, contract: &ContractId, payload: &Value) {
        self.deliver(contract, payload);
    }

    fn event(&self, extension: &str, params: TopicParams) {
        let pages: Vec<String> = lock(&self.topics)
            .get(extension)
            .into_iter()
            .flatten()
            .filter(|(_, topics)| topics.contains(&params.topic))
            .map(|(page, _)| page.clone())
            .collect();
        if !pages.is_empty() {
            self.emit(
                EVENT,
                json!({
                    "extension": extension,
                    "pages": pages,
                    "topic": params.topic,
                    "payload": params.payload,
                }),
            );
        }
    }

    fn progress(&self, extension: &str, params: ProgressParams) {
        self.emit(
            PROGRESS,
            json!({ "extension": extension, "message": params.message, "fraction": params.fraction }),
        );
    }
}

pub(crate) struct Installed {
    pub entry: InstalledExtension,
    pub manifest: Manifest,
}

pub(crate) struct Host {
    desk: Arc<Desk>,
    supervisor: Supervisor,
    router: Router,
    files: Arc<FileTokens>,
    jobs: Jobs,
    home: PathBuf,
    storage: Mutex<()>,
    confirmations: Mutex<HashMap<u64, oneshot::Sender<bool>>>,
    next_confirmation: AtomicU64,
    host_topics: Arc<HostTopics>,
    routing: Routing,
}

impl Host {
    pub(crate) fn new(directories: &AppDirectories) -> Self {
        let desk = Arc::new(Desk::default());
        let frontend: Arc<dyn Frontend> = Arc::clone(&desk) as Arc<dyn Frontend>;
        let layout = Layout::new(directories);
        let jobs = Jobs::load(layout.jobs_file());
        let services = Arc::new(
            HostServices::new(layout.clone(), directories.settings_file()).with_frontend(frontend),
        );
        let supervisor = Supervisor::new(Arc::clone(&services));
        let router = Router::supervised(supervisor.clone());
        let host_topics = Arc::new(HostTopics::default());
        let routing = Routing::new(layout, router.clone(), Arc::clone(&host_topics));
        services.set_contract_router(Arc::new(routing.clone()));
        let _ = desk.services.set(Arc::downgrade(&services));
        let _ = desk.supervisor.set(supervisor.clone());
        Self {
            desk,
            router,
            supervisor,
            host_topics,
            routing,
            files: Arc::default(),
            jobs,
            home: veronica_core::paths::home_dir().unwrap_or_default(),
            storage: Mutex::new(()),
            confirmations: Mutex::default(),
            next_confirmation: AtomicU64::new(1),
        }
    }

    pub(crate) fn attach(&self, shell: Arc<dyn Shell>) {
        let _ = self.desk.shell.set(shell);
    }

    pub(crate) fn shell(&self) -> Result<&Arc<dyn Shell>, ErrorObject> {
        self.desk.shell()
    }

    pub(crate) fn emit(&self, event: &str, payload: Value) {
        self.desk.emit(event, payload);
    }

    pub(crate) fn services(&self) -> &Arc<HostServices> {
        self.supervisor.services()
    }

    pub(crate) fn layout(&self) -> &Layout {
        self.services().layout()
    }

    pub(crate) fn supervisor(&self) -> &Supervisor {
        &self.supervisor
    }

    pub(crate) fn router(&self) -> &Router {
        &self.router
    }

    pub(crate) fn files(&self) -> &Arc<FileTokens> {
        &self.files
    }

    pub(crate) fn jobs(&self) -> &Jobs {
        &self.jobs
    }

    pub(crate) fn home(&self) -> &Path {
        &self.home
    }

    pub(crate) fn installer(&self) -> Installer {
        Installer::new(self.layout().clone(), HostIdentity::current())
            .with_health_check(Arc::clone(self.services()))
    }

    pub(crate) fn registry(&self) -> Result<Registry, BackendError> {
        Registry::load(&self.layout().installed_file())
            .map_err(|error| BackendError::Launch(format!("{error:#}")))
    }

    pub(crate) fn manifest(&self, id: &str) -> Result<Manifest, BackendError> {
        let install = self
            .layout()
            .current_link(id)
            .canonicalize()
            .map_err(|_| BackendError::NotInstalled(id.to_string()))?;
        let raw = std::fs::read_to_string(install.join("manifest.json"))
            .map_err(|error| BackendError::Launch(error.to_string()))?;
        Manifest::parse(&raw).map_err(|error| BackendError::Launch(error.to_string()))
    }

    pub(crate) fn installed(&self, id: &str) -> Result<Installed, BackendError> {
        let entry = self
            .registry()?
            .get(id)
            .cloned()
            .ok_or_else(|| BackendError::NotInstalled(id.to_string()))?;
        Ok(Installed {
            entry,
            manifest: self.manifest(id)?,
        })
    }

    pub(crate) fn with_storage<T>(
        &self,
        id: &str,
        action: impl FnOnce(&mut Storage) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let _guard = lock(&self.storage);
        action(&mut Storage::open(
            self.layout().data_dir(id).join(STORAGE_FILE),
        )?)
    }

    pub(crate) fn open_page(&self, extension: &str, page: &str) {
        lock(&self.desk.topics)
            .entry(extension.to_string())
            .or_default()
            .entry(page.to_string())
            .or_default();
    }

    pub(crate) fn close_page(&self, extension: &str, page: &str) {
        let mut topics = lock(&self.desk.topics);
        if let Some(pages) = topics.get_mut(extension) {
            pages.remove(page);
            if pages.is_empty() {
                topics.remove(extension);
            }
        }
    }

    pub(crate) fn open_pages(&self, extension: &str) -> Vec<String> {
        lock(&self.desk.topics)
            .get(extension)
            .map(|pages| pages.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn subscribe(&self, extension: &str, page: &str, topic: &str, wanted: bool) {
        let mut topics = lock(&self.desk.topics);
        let subscribed = topics
            .entry(extension.to_string())
            .or_default()
            .entry(page.to_string())
            .or_default();
        if wanted {
            subscribed.insert(topic.to_string());
        } else {
            subscribed.remove(topic);
        }
    }

    pub(crate) fn surfaces(&self, extension: &str) -> Vec<Surface> {
        self.services().surfaces().for_extension(extension)
    }

    pub(crate) fn all_surfaces(&self) -> Vec<Surface> {
        self.services().surfaces().list()
    }

    pub(crate) fn routing(&self) -> &Routing {
        &self.routing
    }

    pub(crate) fn publish_host_topic(&self, contract: &ContractId, payload: &Value) {
        if self.host_topics.set(contract, payload) {
            self.desk.deliver(contract, payload);
        }
    }

    pub(crate) fn forget(&self, extension: &str) {
        self.files.revoke(extension);
        if self.services().surfaces().remove_extension(extension) {
            self.desk.push_surfaces(extension);
        }
    }

    pub(crate) async fn confirm(&self, mut payload: Value, patience: Duration) -> Option<bool> {
        let request = self.next_confirmation.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        lock(&self.confirmations).insert(request, sender);
        payload["request"] = json!(request);
        self.emit(INTERFACE, payload);
        let answer = tokio::time::timeout(patience, receiver).await;
        lock(&self.confirmations).remove(&request);
        answer.ok()?.ok()
    }

    pub(crate) fn answer(&self, request: u64, accepted: bool) -> bool {
        lock(&self.confirmations)
            .remove(&request)
            .is_some_and(|sender| sender.send(accepted).is_ok())
    }

    pub(crate) async fn start(&self) {
        for (id, error) in self.supervisor.start_background().await {
            tracing::warn!(target: "veronica", "cannot start the {id} extension: {error}");
        }
    }

    pub(crate) async fn stop(&self) {
        self.supervisor.stop_all().await;
    }
}
