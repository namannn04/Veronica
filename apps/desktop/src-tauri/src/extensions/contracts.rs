use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::Value;
use veronica_host::connection::BackendError;
use veronica_host::contracts::{self, is_host_contract};
use veronica_host::host_api::{Answer, ContractRouter};
use veronica_host::registry::Registry;
use veronica_host::{Layout, Router};
use veronica_protocol::rpc::{error_code, Caller, CallerKind, ErrorObject};
use veronica_protocol::{ContractId, Manifest};

pub(crate) const REQUEST_PATIENCE: Duration = Duration::from_mins(1);

#[derive(Debug, Default)]
pub(crate) struct HostTopics {
    latest: Mutex<BTreeMap<ContractId, Value>>,
}

impl HostTopics {
    pub(crate) fn set(&self, contract: &ContractId, payload: &Value) -> bool {
        let mut latest = self.latest.lock().unwrap_or_else(PoisonError::into_inner);
        if latest.get(contract) == Some(payload) {
            return false;
        }
        latest.insert(contract.clone(), payload.clone());
        true
    }

    pub(crate) fn get(&self, contract: &ContractId) -> Option<Value> {
        self.latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(contract)
            .cloned()
    }
}

pub(crate) fn enabled_manifests(layout: &Layout) -> Vec<Manifest> {
    let Ok(registry) = Registry::load(&layout.installed_file()) else {
        return Vec::new();
    };
    registry
        .extensions
        .iter()
        .filter(|(_, entry)| entry.enabled)
        .filter_map(|(id, _)| {
            let raw =
                std::fs::read_to_string(layout.current_link(id).join("manifest.json")).ok()?;
            Manifest::parse(&raw).ok()
        })
        .collect()
}

pub(crate) fn backend_error(error: BackendError) -> ErrorObject {
    match error {
        BackendError::Rpc(error) => error,
        BackendError::Timeout(_) => ErrorObject::new(error_code::TIMEOUT, error.to_string()),
        BackendError::NotInstalled(_)
        | BackendError::Disabled(_)
        | BackendError::NoBackend(_)
        | BackendError::CrashLooping(_) => {
            ErrorObject::new(error_code::UNAVAILABLE, error.to_string())
        }
        BackendError::Launch(_) | BackendError::Exited(_) | BackendError::Protocol(_) => {
            ErrorObject::new(error_code::INTERNAL_ERROR, error.to_string())
        }
    }
}

#[derive(Clone)]
pub(crate) struct Routing {
    layout: Layout,
    router: Router,
    topics: Arc<HostTopics>,
}

impl Routing {
    pub(crate) fn new(layout: Layout, router: Router, topics: Arc<HostTopics>) -> Self {
        Self {
            layout,
            router,
            topics,
        }
    }

    pub(crate) async fn request(
        &self,
        caller: Caller,
        contract: ContractId,
        params: Value,
        patience: Duration,
    ) -> Result<Value, ErrorObject> {
        if is_host_contract(&contract) {
            return self.topics.get(&contract).ok_or_else(|| {
                ErrorObject::new(
                    error_code::UNAVAILABLE,
                    format!("{contract} has not been published yet"),
                )
            });
        }
        let manifests = enabled_manifests(&self.layout);
        let provider = contracts::provider(&manifests, &contract)?;
        let call = self
            .router
            .contract_request(&provider, contract, params, caller);
        match tokio::time::timeout(patience, call).await {
            Ok(answer) => answer.map_err(backend_error),
            Err(_) => Err(backend_error(BackendError::Timeout(patience))),
        }
    }
}

impl ContractRouter for Routing {
    fn request(&self, caller: &str, contract: ContractId, params: Value) -> Answer {
        let routing = self.clone();
        let caller = Caller {
            kind: CallerKind::Extension,
            extension: Some(caller.to_string()),
        };
        Box::pin(async move {
            routing
                .request(caller, contract, params, REQUEST_PATIENCE)
                .await
        })
    }
}
