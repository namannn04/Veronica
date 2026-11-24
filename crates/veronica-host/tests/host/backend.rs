use std::fs;
use std::future::Future;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::connection::{BackendError, StopStep};
use veronica_host::host_api::HostServices;
use veronica_host::supervisor::{CrashRecord, Policy, Status};
use veronica_host::{Confirmation, HostIdentity, Installer, Layout, Router, Supervisor};
use veronica_protocol::rpc::{error_code, Caller, CallerKind};
use veronica_protocol::ExitCode;

use crate::common::write_tree;

const PATIENCE: Duration = Duration::from_secs(30);
const ID: &str = "fixture";
static PACKAGES: AtomicUsize = AtomicUsize::new(0);

fn fixture_backend() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let output = std::process::Command::new(cargo)
            .args([
                "build",
                "--quiet",
                "--package",
                "veronica-ext-fixture",
                "--message-format=json",
            ])
            .stderr(Stdio::inherit())
            .output()
            .expect("cargo runs");
        assert!(output.status.success(), "building the fixture failed");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|message| message["target"]["name"] == "veronica-ext-fixture")
            .find_map(|message| message["executable"].as_str().map(PathBuf::from))
            .expect("cargo reports the fixture executable")
    })
}

fn fixture_manifest() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions/fixture/manifest.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

struct Env {
    home: TempDir,
    services: Arc<HostServices>,
}

impl Env {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let directories = AppDirectories::with_env(home.path(), EnvOverrides::default());
        let services = HostServices::new(Layout::new(&directories), directories.settings_file());
        Self {
            home,
            services: Arc::new(services),
        }
    }

    fn layout(&self) -> &Layout {
        self.services.layout()
    }

    fn installer(&self) -> Installer {
        Installer::new(self.layout().clone(), HostIdentity::current())
    }

    fn checked_installer(&self) -> Installer {
        self.installer()
            .with_health_check(Arc::clone(&self.services))
    }

    fn package(&self, edit: impl FnOnce(&mut Value), backend: &[u8]) -> PathBuf {
        let mut manifest = fixture_manifest();
        edit(&mut manifest);
        let index = PACKAGES.fetch_add(1, Ordering::Relaxed);
        let source = self.home.path().join(format!("package-{index}"));
        let manifest = serde_json::to_vec(&manifest).unwrap();
        write_tree(
            &source,
            &[
                ("manifest.json", &manifest, 0o644),
                ("bin/backend", backend, 0o755),
            ],
        );
        source
    }

    fn install(&self, edit: impl FnOnce(&mut Value)) {
        let backend = fs::read(fixture_backend()).unwrap();
        let source = self.package(edit, &backend);
        self.installer().install_sideload(&source).unwrap();
    }

    fn supervisor(&self, policy: Policy) -> Supervisor {
        Supervisor::with_policy(Arc::clone(&self.services), policy)
    }

    fn log(&self) -> String {
        fs::read_to_string(self.layout().log_file(ID)).unwrap_or_default()
    }
}

fn background(manifest: &mut Value) {
    manifest["activation"] = json!({ "mode": "background" });
}

fn host() -> Caller {
    Caller {
        kind: CallerKind::Host,
        extension: None,
    }
}

fn fast() -> Policy {
    Policy {
        backoff_min: Duration::from_millis(100),
        backoff_max: Duration::from_millis(400),
        idle_check: Duration::from_millis(100),
        shutdown_grace: Duration::from_secs(1),
        terminate_grace: Duration::from_secs(1),
        ..Policy::default()
    }
}

async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = Instant::now() + PATIENCE;
    while !check().await {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn process_exists(pid: u32) -> bool {
    let Ok(status) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    !status
        .rsplit(')')
        .next()
        .is_some_and(|rest| rest.trim_start().starts_with('Z'))
}

async fn running_pid(supervisor: &Supervisor, other_than: Option<u32>) -> u32 {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Status::Running { pid: Some(pid) } = supervisor.status(ID).await {
            if Some(pid) != other_than {
                return pid;
            }
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for a running backend"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn pid_of(value: &Value) -> u32 {
    u32::try_from(value["pid"].as_u64().unwrap()).unwrap()
}

fn rpc_code(error: &BackendError) -> Option<i64> {
    match error {
        BackendError::Rpc(error) => Some(error.code),
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn oneshot_cli_run_end_to_end() {
    let env = Env::new();
    env.install(|_| {});
    let router = Router::oneshot(Arc::clone(&env.services));
    let argv = |args: &[&str]| args.iter().map(ToString::to_string).collect::<Vec<_>>();

    let echoed = router
        .run_cli(
            ID,
            argv(&["fixture", "echo", "hello"]),
            true,
            Confirmation::Preview,
        )
        .await;
    assert_eq!(echoed.exit_code, ExitCode::Success);
    let document = echoed.document.unwrap();
    assert_eq!(document["text"], "hello");
    assert_eq!(document["mode"], "oneshot");
    let pid = pid_of(&document);
    eventually("the oneshot backend exits", || async move {
        !process_exists(pid)
    })
    .await;

    let text = router
        .run_cli(
            ID,
            argv(&["fixture", "echo", "plain"]),
            false,
            Confirmation::Preview,
        )
        .await;
    assert_eq!(text.text.as_deref(), Some("plain\n"));

    let failed = router
        .run_cli(
            ID,
            argv(&["fixture", "fail", "--code", "6"]),
            true,
            Confirmation::Preview,
        )
        .await;
    assert_eq!(failed.exit_code, ExitCode::PermissionDenied);
    assert_eq!(failed.document.unwrap()["error"]["code"], 6);

    let usage = router
        .run_cli(
            ID,
            argv(&["fixture", "nonsense"]),
            true,
            Confirmation::Preview,
        )
        .await;
    assert_eq!(usage.exit_code, ExitCode::Usage);

    let previewed = router
        .run_cli(
            ID,
            argv(&["fixture", "wipe", "--", "--yes"]),
            true,
            Confirmation::from_yes_flag(false),
        )
        .await;
    assert_eq!(previewed.exit_code, ExitCode::ConfirmationRequired);
    assert_eq!(previewed.document.unwrap()["wouldRemove"], "--yes");
    let wiped = router
        .run_cli(
            ID,
            argv(&["fixture", "wipe", "cache"]),
            true,
            Confirmation::from_yes_flag(true),
        )
        .await;
    assert_eq!(wiped.exit_code, ExitCode::Success);
    assert_eq!(wiped.document.unwrap()["removed"], "cache");

    let missing = router
        .run_cli("absent", argv(&["absent"]), true, Confirmation::Preview)
        .await;
    assert_eq!(missing.exit_code, ExitCode::Unavailable);
    assert!(missing.document.unwrap()["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not installed"));

    let contract = router
        .contract_request(ID, "fixture.nothing@1".parse().unwrap(), json!({}), host())
        .await
        .unwrap_err();
    assert_eq!(rpc_code(&contract), Some(error_code::METHOD_NOT_FOUND));
}

#[tokio::test(flavor = "multi_thread")]
async fn on_demand_backends_start_on_first_call_and_stop_when_idle() {
    let env = Env::new();
    env.install(|manifest| manifest["activation"]["idleStopSeconds"] = json!(1));
    let supervisor = env.supervisor(fast());
    assert_eq!(supervisor.status(ID).await, Status::Stopped);

    let router = Router::supervised(supervisor.clone());
    let answer = router.invoke(ID, "mode", json!({}), host()).await.unwrap();
    assert_eq!(answer["mode"], "service");
    let pid = pid_of(&answer);
    assert_eq!(
        supervisor.status(ID).await,
        Status::Running { pid: Some(pid) }
    );

    eventually("the idle backend stops", || {
        let supervisor = supervisor.clone();
        async move { supervisor.status(ID).await == Status::Stopped }
    })
    .await;
    eventually(
        "the idle backend exits",
        || async move { !process_exists(pid) },
    )
    .await;

    let again = router.invoke(ID, "mode", json!({}), host()).await.unwrap();
    assert_ne!(pid_of(&again), pid);
    assert!(supervisor.stop(ID).await.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn crashed_background_backends_restart_after_a_backoff() {
    let env = Env::new();
    env.install(background);
    let supervisor = env.supervisor(fast());
    let failures = supervisor.start_background().await;
    assert!(failures.is_empty(), "{failures:?}");
    let first = running_pid(&supervisor, None).await;

    let crash = Router::supervised(supervisor.clone())
        .invoke(ID, "crash", json!({}), host())
        .await
        .unwrap_err();
    assert!(matches!(crash, BackendError::Exited(_)), "{crash:?}");
    assert_eq!(crash.exit_code(), ExitCode::BackendFailed);

    running_pid(&supervisor, Some(first)).await;
    assert!(env.log().contains("signal 6"), "{}", env.log());
    assert!(CrashRecord::load(env.layout(), ID).is_none());
    supervisor.stop_all().await;
    assert_eq!(supervisor.status(ID).await, Status::Stopped);
}

#[tokio::test(flavor = "multi_thread")]
async fn repeated_crashes_mark_the_backend_crash_looping() {
    let env = Env::new();
    env.install(background);
    let supervisor = env.supervisor(fast());
    let router = Router::supervised(supervisor.clone());
    supervisor.start(ID).await.unwrap();
    for _ in 0..fast().crash_limit {
        if supervisor.status(ID).await == Status::CrashLooping {
            break;
        }
        let pid = running_pid(&supervisor, None).await;
        let _ = router
            .invoke(ID, "exit", json!({ "code": 3 }), host())
            .await;
        eventually("the exit is noticed", || {
            let supervisor = supervisor.clone();
            async move { supervisor.status(ID).await != Status::Running { pid: Some(pid) } }
        })
        .await;
    }
    eventually("the backend is crash looping", || {
        let supervisor = supervisor.clone();
        async move { supervisor.status(ID).await == Status::CrashLooping }
    })
    .await;
    let record = CrashRecord::load(env.layout(), ID).unwrap();
    assert!(record.crash_looping);
    assert_eq!(record.last_exit, "exit code 3");
    let refused = router.invoke(ID, "mode", json!({}), host()).await;
    assert!(matches!(refused, Err(BackendError::CrashLooping(_))));
    let fresh = env.supervisor(fast());
    assert_eq!(fresh.status(ID).await, Status::CrashLooping);

    supervisor.restart(ID).await.unwrap();
    assert!(CrashRecord::load(env.layout(), ID).is_none());
    assert!(matches!(
        supervisor.status(ID).await,
        Status::Running { .. }
    ));
    supervisor.stop_all().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_frozen_backend_is_killed_after_shutdown_and_sigterm() {
    let env = Env::new();
    env.install(|_| {});
    let supervisor = env.supervisor(fast());
    let connection = supervisor.connection(ID).await.unwrap();
    let pid = connection.pid().unwrap();
    Router::supervised(supervisor.clone())
        .invoke(ID, "freeze", json!({}), host())
        .await
        .unwrap();
    eventually("the backend is stopped by SIGSTOP", || async move {
        fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
            stat.rsplit(')')
                .next()
                .unwrap()
                .trim_start()
                .starts_with('T')
        })
    })
    .await;
    let stopped = supervisor.stop(ID).await.unwrap();
    assert_eq!(stopped.step, StopStep::Killed);
    assert_eq!(stopped.status.unwrap().signal(), Some(9));
    assert!(!process_exists(pid));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_background_backend_that_stops_answering_pings_is_replaced() {
    let env = Env::new();
    env.install(background);
    let supervisor = env.supervisor(Policy {
        health_interval: Duration::from_millis(500),
        health_timeout: Duration::from_secs(1),
        ..fast()
    });
    supervisor.start(ID).await.unwrap();
    let first = running_pid(&supervisor, None).await;
    Router::supervised(supervisor.clone())
        .invoke(ID, "freeze", json!({}), host())
        .await
        .unwrap();
    eventually("the frozen backend is replaced", || {
        let supervisor = supervisor.clone();
        async move {
            matches!(supervisor.status(ID).await, Status::Running { pid: Some(pid) } if pid != first)
        }
    })
    .await;
    assert!(!process_exists(first));
    assert!(env.log().contains("signal 9"), "{}", env.log());
    supervisor.stop_all().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn host_requests_are_checked_against_the_manifest_grant() {
    let env = Env::new();
    env.install(|_| {});
    let router = Router::oneshot(Arc::clone(&env.services));
    let ask = |params: Value| {
        let router = router.clone();
        async move { router.invoke(ID, "host", params, host()).await }
    };

    let denied = ask(json!({ "method": "settings.get", "key": "appearance" }))
        .await
        .unwrap_err();
    assert_eq!(rpc_code(&denied), Some(error_code::PERMISSION_DENIED));
    assert_eq!(denied.exit_code(), ExitCode::PermissionDenied);
    let opened = ask(json!({ "method": "open", "target": "https://example.org" }))
        .await
        .unwrap_err();
    assert_eq!(rpc_code(&opened), Some(error_code::PERMISSION_DENIED));
    let foreign = ask(json!({ "method": "settings.set", "key": "ext.other.x", "value": 1 }))
        .await
        .unwrap_err();
    assert_eq!(rpc_code(&foreign), Some(error_code::PERMISSION_DENIED));

    let notified = ask(json!({ "method": "notify", "title": "hello" }))
        .await
        .unwrap_err();
    assert_eq!(rpc_code(&notified), Some(error_code::UNAVAILABLE));

    ask(json!({ "method": "settings.set", "key": "ext.fixture.color", "value": "teal" }))
        .await
        .unwrap();
    let read = ask(json!({ "method": "settings.get", "key": "ext.fixture.color" }))
        .await
        .unwrap();
    assert_eq!(read, json!("teal"));
    let settings_file = env.home.path().join(".config/veronica/settings.json");
    let settings: Value =
        serde_json::from_str(&fs::read_to_string(settings_file).unwrap()).unwrap();
    assert_eq!(settings["ext.fixture.color"], "teal");

    ask(json!({ "method": "log", "message": "written through the host" }))
        .await
        .unwrap();
    assert!(env.log().contains("info: written through the host"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dropped_call_cancels_the_backend_request() {
    let env = Env::new();
    env.install(|_| {});
    let router = Router::oneshot(Arc::clone(&env.services));
    let call = router.invoke(ID, "sleep", json!({ "seconds": 600 }), host());
    assert!(tokio::time::timeout(Duration::from_secs(3), call)
        .await
        .is_err());
    let marker = env.layout().data_dir(ID).join("cancelled");
    eventually("the backend sees the cancellation", || {
        let marker = marker.clone();
        async move { marker.exists() }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn oneshot_and_service_instances_run_side_by_side() {
    let env = Env::new();
    env.install(|_| {});
    let supervisor = env.supervisor(fast());
    let service = Router::supervised(supervisor.clone());
    let oneshot = Router::oneshot(Arc::clone(&env.services));
    service.invoke(ID, "mode", json!({}), host()).await.unwrap();

    let mut calls = tokio::task::JoinSet::new();
    for index in 0..4 {
        let (service, oneshot) = (service.clone(), oneshot.clone());
        calls.spawn(async move {
            if index % 2 == 0 {
                service.invoke(ID, "mode", json!({}), host()).await.unwrap()
            } else {
                let argv = vec!["fixture".into(), "echo".into(), index.to_string()];
                oneshot
                    .run_cli(ID, argv, true, Confirmation::Preview)
                    .await
                    .document
                    .unwrap()
            }
        });
    }
    let answers = calls.join_all().await;
    let service_pids: Vec<u32> = answers
        .iter()
        .filter(|answer| answer["mode"] == "service")
        .map(pid_of)
        .collect();
    let oneshot_pids: Vec<u32> = answers
        .iter()
        .filter(|answer| answer["mode"] == "oneshot")
        .map(pid_of)
        .collect();
    assert_eq!(service_pids.len(), 2);
    assert_eq!(service_pids[0], service_pids[1]);
    assert_eq!(oneshot_pids.len(), 2);
    assert_ne!(oneshot_pids[0], oneshot_pids[1]);
    assert!(!oneshot_pids.contains(&service_pids[0]));
    supervisor.stop_all().await;
}

#[test]
fn a_backend_that_fails_its_health_check_is_rolled_back_and_quarantined() {
    let env = Env::new();
    let backend = fs::read(fixture_backend()).unwrap();
    let installer = env.checked_installer();
    let good = env.package(|_| {}, &backend);
    let first = installer.install_sideload(&good).unwrap();

    let broken = env.package(
        |manifest| manifest["version"] = json!("0.2"),
        b"#!/bin/sh\nexit 3\n",
    );
    let error = installer.install_sideload(&broken).unwrap_err();
    assert!(format!("{error:#}").contains("health check"), "{error:#}");
    let entry = installer.registry().unwrap().extensions[ID].clone();
    assert_eq!(entry.version, first.version);
    assert!(entry.quarantined.contains(&"0.2.0".parse().unwrap()));
    let current = fs::read_link(env.layout().current_link(ID)).unwrap();
    assert_eq!(current, Path::new("0.1.0"));
    assert!(!env.layout().package_dir(ID).join("0.2.0").exists());

    let fresh = Env::new();
    let broken = fresh.package(|_| {}, b"#!/bin/sh\nexit 3\n");
    assert!(fresh.checked_installer().install_sideload(&broken).is_err());
    assert!(fresh.installer().registry().unwrap().get(ID).is_none());
    assert!(!fresh.layout().current_link(ID).exists());
}
