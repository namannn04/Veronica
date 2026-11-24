use std::io::IsTerminal;
use std::sync::Arc;

use serde_json::{json, Value};
use veronica_protocol::rpc::{
    error_code, Caller, CliRun, CliRunParams, CliRunResult, ContractRequest, ContractRequestParams,
    RequestMethod, RpcCall, RpcCallParams, RunMode, ShortcutFired, ShortcutFiredParams,
};
use veronica_protocol::ContractId;

use crate::connection::{BackendError, Connection, Launch};
use crate::host_api::HostServices;
use crate::supervisor::{Policy, Supervisor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    Preview,
    Confirmed,
}

impl Confirmation {
    pub fn from_yes_flag(yes: bool) -> Self {
        if yes {
            Self::Confirmed
        } else {
            Self::Preview
        }
    }
}

#[derive(Clone)]
pub struct Router {
    services: Arc<HostServices>,
    supervisor: Option<Supervisor>,
    policy: Policy,
}

fn failure(error: &BackendError, json: bool) -> CliRunResult {
    let exit_code = error.exit_code();
    let message = error.to_string();
    if json {
        CliRunResult {
            exit_code,
            document: Some(json!({ "error": { "code": exit_code.code(), "message": message } })),
            text: None,
        }
    } else {
        CliRunResult {
            exit_code,
            document: None,
            text: Some(format!("error: {message}\n")),
        }
    }
}

impl Router {
    pub fn oneshot(services: Arc<HostServices>) -> Self {
        Self {
            services,
            supervisor: None,
            policy: Policy::default(),
        }
    }

    pub fn supervised(supervisor: Supervisor) -> Self {
        Self {
            services: Arc::clone(supervisor.services()),
            policy: supervisor.policy().clone(),
            supervisor: Some(supervisor),
        }
    }

    pub async fn run_cli(
        &self,
        id: &str,
        argv: Vec<String>,
        json: bool,
        confirmation: Confirmation,
    ) -> CliRunResult {
        let params = CliRunParams {
            argv,
            json,
            confirm: confirmation == Confirmation::Confirmed,
            stdin: None,
            cwd: std::env::current_dir().unwrap_or_default(),
            tty: !json && std::io::stdout().is_terminal(),
        };
        match self.call::<CliRun>(id, &params).await {
            Ok(result) => result,
            Err(error) => failure(&error, json),
        }
    }

    pub async fn invoke(
        &self,
        id: &str,
        method: &str,
        params: Value,
        caller: Caller,
    ) -> Result<Value, BackendError> {
        let params = RpcCallParams {
            method: method.to_string(),
            params,
            caller,
        };
        self.call::<RpcCall>(id, &params).await
    }

    pub async fn contract_request(
        &self,
        id: &str,
        contract: ContractId,
        params: Value,
        caller: Caller,
    ) -> Result<Value, BackendError> {
        let params = ContractRequestParams {
            contract,
            params,
            caller,
        };
        self.call::<ContractRequest>(id, &params).await
    }

    pub async fn shortcut(
        &self,
        id: &str,
        shortcut: &str,
        run: Vec<String>,
    ) -> Result<(), BackendError> {
        let params = ShortcutFiredParams {
            id: shortcut.to_string(),
        };
        match self.call::<ShortcutFired>(id, &params).await {
            Err(BackendError::Rpc(error)) if error.code == error_code::METHOD_NOT_FOUND => {
                let result = self.run_cli(id, run, true, Confirmation::Preview).await;
                if result.exit_code.code() == 0 {
                    Ok(())
                } else {
                    Err(BackendError::Exited(format!(
                        "{shortcut} finished with exit code {}",
                        result.exit_code.code()
                    )))
                }
            }
            outcome => outcome,
        }
    }

    async fn call<M: RequestMethod>(
        &self,
        id: &str,
        params: &M::Params,
    ) -> Result<M::Result, BackendError> {
        if let Some(supervisor) = &self.supervisor {
            let connection = supervisor.connection(id).await?;
            return connection.request::<M>(params, None).await;
        }
        let launch = Launch::prepare(self.services.layout(), id, RunMode::Oneshot)?;
        if !launch.enabled {
            return Err(BackendError::Disabled(id.to_string()));
        }
        let connection = Connection::start(
            launch,
            Arc::clone(&self.services),
            self.policy.start_timeout,
        )
        .await?;
        let outcome = connection.request::<M>(params, None).await;
        connection.retire();
        self.policy.stop(&connection).await;
        outcome
    }
}
