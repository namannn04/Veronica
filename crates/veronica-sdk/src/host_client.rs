use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use veronica_protocol::manifest::ShellService;
use veronica_protocol::rpc::{
    error_code, ContractPublishParams, ErrorObject, EventEmit, HostContractPublish,
    HostContractRequest, HostContractRequestParams, HostLog, HostNotify, HostOpen, HostSecretGet,
    HostSecretSet, HostSettingsGet, HostSettingsSet, HostShellAction, HostSurfaceUpdate, KeyParams,
    LogLevel, LogParams, Message, Notification, NotificationMethod, NotifyParams, OpenParams,
    Progress, ProgressParams, Request, RequestId, RequestMethod, Response, SecretSetParams,
    SettingsSetParams, ShellActionParams, SurfaceUpdateParams, TopicParams,
};
use veronica_protocol::ContractId;

pub(crate) enum Outgoing {
    Message(Message),
    Flush(oneshot::Sender<()>),
}

type Pending = HashMap<RequestId, oneshot::Sender<Response>>;

#[derive(Clone)]
pub struct HostClient {
    inner: Arc<Inner>,
}

struct Inner {
    next_id: AtomicI64,
    pending: Mutex<Pending>,
    outgoing: mpsc::UnboundedSender<Outgoing>,
}

struct PendingGuard<'a> {
    client: &'a HostClient,
    id: RequestId,
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        self.client.pending().remove(&self.id);
    }
}

fn encode_error(error: &serde_json::Error) -> ErrorObject {
    ErrorObject::new(error_code::INTERNAL_ERROR, error.to_string())
}

fn host_closed() -> ErrorObject {
    ErrorObject::new(error_code::UNAVAILABLE, "the host closed the connection")
}

impl HostClient {
    pub(crate) fn new(outgoing: mpsc::UnboundedSender<Outgoing>) -> Self {
        Self {
            inner: Arc::new(Inner {
                next_id: AtomicI64::new(1),
                pending: Mutex::new(HashMap::new()),
                outgoing,
            }),
        }
    }

    fn pending(&self) -> MutexGuard<'_, Pending> {
        self.inner
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn send(&self, message: Message) -> bool {
        self.inner.outgoing.send(Outgoing::Message(message)).is_ok()
    }

    pub(crate) fn resolve(&self, response: Response) {
        let Some(id) = response.id.clone() else {
            tracing::warn!(?response, "host sent an error without a request id");
            return;
        };
        if let Some(waiter) = self.pending().remove(&id) {
            let _ = waiter.send(response);
        } else {
            tracing::warn!(?id, "host answered an unknown request");
        }
    }

    pub(crate) fn close(&self) {
        self.pending().clear();
    }

    pub async fn request<M: RequestMethod>(
        &self,
        params: &M::Params,
    ) -> Result<M::Result, ErrorObject> {
        let id = RequestId::Number(self.inner.next_id.fetch_add(1, Ordering::Relaxed));
        let request =
            Request::new::<M>(id.clone(), params).map_err(|error| encode_error(&error))?;
        let (waiter, answer) = oneshot::channel();
        self.pending().insert(id.clone(), waiter);
        let _guard = PendingGuard { client: self, id };
        if !self.send(Message::Request(request)) {
            return Err(host_closed());
        }
        answer.await.map_err(|_| host_closed())?.result::<M>()
    }

    pub fn notification<M: NotificationMethod>(
        &self,
        params: &M::Params,
    ) -> Result<(), ErrorObject> {
        let notification = Notification::new::<M>(params).map_err(|error| encode_error(&error))?;
        if self.send(Message::Notification(notification)) {
            Ok(())
        } else {
            Err(host_closed())
        }
    }

    pub async fn settings_get(&self, key: &str) -> Result<Option<Value>, ErrorObject> {
        let params = KeyParams {
            key: key.to_string(),
        };
        Ok(self.request::<HostSettingsGet>(&params).await?.value)
    }

    pub async fn settings_set(&self, key: &str, value: Value) -> Result<(), ErrorObject> {
        let params = SettingsSetParams {
            key: key.to_string(),
            value,
        };
        self.request::<HostSettingsSet>(&params).await
    }

    pub async fn notify(&self, params: NotifyParams) -> Result<(), ErrorObject> {
        self.request::<HostNotify>(&params).await
    }

    pub async fn surface_update(&self, surface: &str, state: Value) -> Result<(), ErrorObject> {
        let params = SurfaceUpdateParams {
            surface: surface.to_string(),
            state,
        };
        self.request::<HostSurfaceUpdate>(&params).await
    }

    pub async fn contract_publish(
        &self,
        contract: ContractId,
        payload: Value,
    ) -> Result<(), ErrorObject> {
        let params = ContractPublishParams { contract, payload };
        self.request::<HostContractPublish>(&params).await
    }

    pub async fn contract_request(
        &self,
        contract: ContractId,
        params: Value,
    ) -> Result<Value, ErrorObject> {
        let params = HostContractRequestParams { contract, params };
        self.request::<HostContractRequest>(&params).await
    }

    pub async fn shell_action(
        &self,
        action: ShellService,
        args: Value,
    ) -> Result<Value, ErrorObject> {
        let params = ShellActionParams { action, args };
        self.request::<HostShellAction>(&params).await
    }

    pub async fn secret_get(&self, key: &str) -> Result<Option<String>, ErrorObject> {
        let params = KeyParams {
            key: key.to_string(),
        };
        Ok(self.request::<HostSecretGet>(&params).await?.value)
    }

    pub async fn secret_set(&self, key: &str, value: Option<String>) -> Result<(), ErrorObject> {
        let params = SecretSetParams {
            key: key.to_string(),
            value,
        };
        self.request::<HostSecretSet>(&params).await
    }

    pub async fn open(&self, target: &str) -> Result<(), ErrorObject> {
        let params = OpenParams {
            target: target.to_string(),
        };
        self.request::<HostOpen>(&params).await
    }

    pub async fn log(&self, level: LogLevel, message: &str) -> Result<(), ErrorObject> {
        let params = LogParams {
            level,
            message: message.to_string(),
        };
        self.request::<HostLog>(&params).await
    }

    pub fn emit(&self, topic: &str, payload: Value) -> Result<(), ErrorObject> {
        let params = TopicParams {
            topic: topic.to_string(),
            payload,
        };
        self.notification::<EventEmit>(&params)
    }

    pub fn progress(
        &self,
        message: Option<String>,
        fraction: Option<f64>,
    ) -> Result<(), ErrorObject> {
        self.notification::<Progress>(&ProgressParams { message, fraction })
    }
}
