use anyhow::Context as _;
use serde::Deserialize;
use serde_json::Value;
use veronica_machines::host;
use veronica_machines::{Machine, Reach, DEFAULT_TIMEOUT};
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::protocol::rpc::ErrorObject;
use veronica_sdk::rpc::{failure, reply};
use veronica_sdk::settings::Settings;

use crate::cli::MACHINES_KEY;

pub(crate) const METHODS: [&str; 12] = [
    "probe",
    "add",
    "remove",
    "discover",
    "terminal",
    "files",
    "openFile",
    "containers",
    "containerAction",
    "containerLogs",
    "power",
    "wake",
];

#[derive(Deserialize)]
struct Target {
    id: String,
}

#[derive(Deserialize)]
struct AddParams {
    target: String,
    name: Option<String>,
    port: Option<u16>,
}

#[derive(Deserialize)]
struct FilesParams {
    id: String,
    path: Option<String>,
}

#[derive(Deserialize)]
struct FileParams {
    id: String,
    path: String,
}

#[derive(Deserialize)]
struct ContainerParams {
    id: String,
    engine: String,
    container: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    lines: usize,
}

#[derive(Deserialize)]
struct PowerParams {
    id: String,
    action: String,
}

fn stored(settings: &Settings) -> Vec<Machine> {
    settings
        .get(MACHINES_KEY)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

fn settings() -> anyhow::Result<(std::path::PathBuf, Settings)> {
    let path = AppDirectories::current()?.settings_file();
    let settings = Settings::load(&path)?;
    Ok((path, settings))
}

fn machine(id: &str) -> anyhow::Result<Machine> {
    let (_, settings) = settings()?;
    veronica_machines::fleet(stored(&settings))
        .into_iter()
        .find(|machine| machine.id == id)
        .with_context(|| format!("no machine called {id}"))
}

fn add(request: AddParams) -> anyhow::Result<Machine> {
    let target = request.target.trim().to_string();
    host::validate_ssh_target(&target)?;
    let label = request
        .name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| target.clone());
    let id = host::slugify(&label);
    anyhow::ensure!(id != "local", "\"local\" is reserved for this computer");
    let machine = Machine {
        id,
        name: label,
        reach: Reach::Ssh {
            target,
            port: request.port,
        },
        mac: None,
    };
    let (path, mut settings) = settings()?;
    let mut machines = stored(&settings);
    anyhow::ensure!(
        !machines.iter().any(|existing| existing.id == machine.id),
        "a machine called {} already exists",
        machine.id
    );
    machines.push(machine.clone());
    settings.set(MACHINES_KEY, serde_json::to_value(&machines)?);
    settings.save(&path)?;
    Ok(machine)
}

fn remove(id: &str) -> anyhow::Result<()> {
    let (path, mut settings) = settings()?;
    let mut machines = stored(&settings);
    let before = machines.len();
    machines.retain(|machine| machine.id != id);
    anyhow::ensure!(machines.len() != before, "no machine called {id}");
    settings.set(MACHINES_KEY, serde_json::to_value(&machines)?);
    settings.save(&path)
}

fn discover() -> anyhow::Result<Vec<String>> {
    let (_, settings) = settings()?;
    let configured = stored(&settings);
    let config = veronica_sdk::paths::home_dir()
        .map(|home| home.join(".ssh/config"))
        .unwrap_or_default();
    Ok(host::ssh_config_hosts(&config)
        .into_iter()
        .filter(|alias| {
            !configured
                .iter()
                .any(|machine| machine.ssh_target() == Some(alias.as_str()))
        })
        .collect())
}

async fn open_file(request: FileParams) -> anyhow::Result<String> {
    let machine = machine(&request.id)?;
    let local = if machine.is_local() {
        anyhow::ensure!(
            std::path::Path::new(&request.path).is_file(),
            "selected path is not a file"
        );
        std::path::PathBuf::from(&request.path)
    } else {
        let downloads = veronica_sdk::paths::home_dir()
            .context("cannot resolve Downloads directory")?
            .join("Downloads");
        veronica_machines::manage::download_file(
            &machine,
            &request.path,
            &downloads,
            DEFAULT_TIMEOUT,
        )
        .await?
    };
    veronica_sdk::spawn::spawn_detached(veronica_sdk::spawn::command("xdg-open").arg(&local))?;
    Ok(local.display().to_string())
}

async fn dispatch(method: &str, arguments: Value) -> anyhow::Result<Value> {
    use veronica_machines::manage;
    Ok(match method {
        "probe" => {
            let (_, settings) = settings()?;
            let fleet = veronica_machines::fleet(stored(&settings));
            serde_json::to_value(veronica_machines::probe_fleet(&fleet, DEFAULT_TIMEOUT).await)?
        }
        "add" => serde_json::to_value(add(serde_json::from_value(arguments)?)?)?,
        "remove" => {
            let Target { id } = serde_json::from_value(arguments)?;
            remove(&id)?;
            Value::Null
        }
        "discover" => serde_json::to_value(discover()?)?,
        "terminal" => {
            let Target { id } = serde_json::from_value(arguments)?;
            manage::open_terminal(&machine(&id)?)?;
            Value::Null
        }
        "files" => {
            let FilesParams { id, path } = serde_json::from_value(arguments)?;
            serde_json::to_value(
                manage::list_directory(&machine(&id)?, path.as_deref(), DEFAULT_TIMEOUT).await?,
            )?
        }
        "openFile" => serde_json::to_value(open_file(serde_json::from_value(arguments)?).await?)?,
        "containers" => {
            let Target { id } = serde_json::from_value(arguments)?;
            serde_json::to_value(manage::containers(&machine(&id)?, DEFAULT_TIMEOUT).await?)?
        }
        "containerAction" => {
            let request: ContainerParams = serde_json::from_value(arguments)?;
            manage::container_action(
                &machine(&request.id)?,
                &request.engine,
                &request.container,
                &request.action,
                DEFAULT_TIMEOUT,
            )
            .await?;
            Value::Null
        }
        "containerLogs" => {
            let request: ContainerParams = serde_json::from_value(arguments)?;
            serde_json::to_value(
                manage::container_logs(
                    &machine(&request.id)?,
                    &request.engine,
                    &request.container,
                    request.lines,
                    DEFAULT_TIMEOUT,
                )
                .await?,
            )?
        }
        "power" => {
            let PowerParams { id, action } = serde_json::from_value(arguments)?;
            let parsed = manage::PowerAction::parse(&action)
                .with_context(|| format!("unknown power action {action:?}"))?;
            manage::power(&machine(&id)?, parsed, DEFAULT_TIMEOUT).await?;
            Value::Null
        }
        "wake" => {
            let Target { id } = serde_json::from_value(arguments)?;
            let machine = machine(&id)?;
            let mac = machine.mac.clone().with_context(|| {
                format!(
                    "{} has no MAC address stored; add one with `vr machines add <target> --mac aa:bb:cc:dd:ee:ff`",
                    machine.name
                )
            })?;
            manage::wake(&mac)?;
            Value::Null
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
