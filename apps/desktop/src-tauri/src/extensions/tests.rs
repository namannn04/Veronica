use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::host_api::Grant;
use veronica_protocol::manifest::Permissions;
use veronica_protocol::rpc::{ErrorObject, EventEmit, Notification, NotifyParams, TopicParams};

use super::bridge::{dispatch, Code, Failure};
use super::host::{Host, Pending, Shell, EVENT, INTERFACE};
use super::jobs::{submit, Action, Job, Jobs, Kind, Phase, CHANGED, JOB};
use super::library;

const ID: &str = "fixture";
const PAGE: &str = "main";
const PATIENCE: Duration = Duration::from_mins(1);

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

#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<(String, Value)>>,
    opened: Mutex<Vec<String>>,
    copied: Mutex<Vec<String>>,
}

impl Recorder {
    fn events(&self, name: &str) -> Vec<Value> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|(event, _)| event == name)
            .map(|(_, payload)| payload.clone())
            .collect()
    }
}

impl Shell for Recorder {
    fn emit(&self, event: &str, payload: Value) {
        self.events
            .lock()
            .unwrap()
            .push((event.to_string(), payload));
    }

    fn notify(&self, _extension: &str, _params: NotifyParams) -> Result<(), ErrorObject> {
        Ok(())
    }

    fn open_external(&self, url: &str) -> Result<(), String> {
        self.opened.lock().unwrap().push(url.to_string());
        Ok(())
    }

    fn copy(&self, text: String) -> Pending<Result<(), String>> {
        self.copied.lock().unwrap().push(text);
        Box::pin(async { Ok(()) })
    }
}

struct Env {
    home: TempDir,
    directories: AppDirectories,
    host: Arc<Host>,
    shell: Arc<Recorder>,
    packages: Mutex<usize>,
}

impl Env {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let directories = AppDirectories::with_env(home.path(), EnvOverrides::default());
        let host = Arc::new(Host::new(&directories));
        let shell = Arc::new(Recorder::default());
        host.attach(Arc::clone(&shell) as Arc<dyn Shell>);
        Self {
            home,
            directories,
            host,
            shell,
            packages: Mutex::new(0),
        }
    }

    fn package(&self, edit: impl FnOnce(&mut Value)) -> PathBuf {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../extensions/fixture");
        let mut manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(path.join("manifest.json")).unwrap())
                .unwrap();
        manifest["ui"] = json!({
            "root": "ui",
            "pages": [{ "id": PAGE, "title": "Fixture", "entry": "index.html" }]
        });
        edit(&mut manifest);
        let mut packages = self.packages.lock().unwrap();
        *packages += 1;
        let source = self.home.path().join(format!("package-{packages}"));
        std::fs::create_dir_all(source.join("bin")).unwrap();
        std::fs::create_dir_all(source.join("ui")).unwrap();
        std::fs::write(source.join("manifest.json"), manifest.to_string()).unwrap();
        std::fs::write(source.join("ui/index.html"), "<!doctype html>").unwrap();
        let backend = source.join("bin/backend");
        std::fs::copy(fixture_backend(), &backend).unwrap();
        std::fs::set_permissions(&backend, std::fs::Permissions::from_mode(0o755)).unwrap();
        source
    }

    fn install(&self, edit: impl FnOnce(&mut Value)) {
        let source = self.package(edit);
        self.host.installer().install_sideload(&source).unwrap();
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, Failure> {
        dispatch(&self.host, ID, PAGE, method, params).await
    }

    async fn code(&self, method: &str, params: Value) -> Code {
        self.call(method, params)
            .await
            .expect_err("the call is refused")
            .code
    }

    async fn finish(&self, action: Action) -> Job {
        let announced = self.shell.events(CHANGED).len();
        let id = submit(&self.host, action).id;
        tokio::time::timeout(PATIENCE, async {
            loop {
                let job = self.host.jobs().list().into_iter().find(|job| job.id == id);
                match job {
                    Some(job)
                        if matches!(job.phase, Phase::Succeeded | Phase::Failed)
                            && self.shell.events(CHANGED).len() > announced =>
                    {
                        return job
                    }
                    _ => tokio::time::sleep(Duration::from_millis(20)).await,
                }
            }
        })
        .await
        .expect("the job finishes")
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn every_gated_method_needs_its_grant() {
    let env = Env::new();
    let shared = env.home.path().canonicalize().unwrap().join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::write(shared.join("song.mp3"), "song").unwrap();
    let song = shared.join("song.mp3").display().to_string();
    let gated = [
        ("ui.openExternal", json!({ "url": "https://example.org/a" })),
        ("clipboard.writeText", json!({ "text": "copied" })),
        ("files.url", json!({ "path": song })),
        ("contracts.request", json!({ "id": "provider.echo@1" })),
        ("contracts.subscribe", json!({ "id": "provider.echo@1" })),
    ];

    env.install(|_| {});
    for (method, params) in &gated {
        let refused = env.call(method, params.clone()).await.unwrap_err();
        assert_eq!(refused.code, Code::Permission, "{method}");
        assert!(refused
            .data
            .is_some_and(|data| data["permission"].is_string()));
    }
    assert!(env.shell.opened.lock().unwrap().is_empty());
    assert!(env.shell.copied.lock().unwrap().is_empty());

    env.install(|manifest| {
        manifest["permissions"] = json!({
            "host": ["openExternal", "clipboard:write"],
            "filesystem": [{ "path": shared, "access": "read" }]
        });
        manifest["contracts"] =
            json!({ "uses": ["provider.echo@1", "provider.echo@2", "absent.thing@1"] });
    });
    let provider = env.package(|manifest| {
        manifest["id"] = json!("provider");
        manifest["commands"] = json!([]);
        manifest.as_object_mut().unwrap().remove("backend");
        manifest["contracts"] =
            json!({ "provides": [{ "id": "provider.echo@2", "kind": "request" }] });
    });
    env.host.installer().install_sideload(&provider).unwrap();

    assert_eq!(
        env.call(gated[0].0, gated[0].1.clone()).await,
        Ok(Value::Null)
    );
    assert_eq!(*env.shell.opened.lock().unwrap(), ["https://example.org/a"]);
    for url in [
        "http://example.org",
        "file:///etc/passwd",
        "https://",
        "https://a b",
        "javascript:1",
    ] {
        assert_eq!(
            env.code("ui.openExternal", json!({ "url": url })).await,
            Code::Invalid,
            "{url}"
        );
    }
    assert_eq!(
        env.call(gated[1].0, gated[1].1.clone()).await,
        Ok(Value::Null)
    );
    assert_eq!(*env.shell.copied.lock().unwrap(), ["copied"]);

    let url = env.call("files.url", gated[2].1.clone()).await.unwrap();
    let token = url
        .as_str()
        .unwrap()
        .strip_prefix("veronica-ext://fixture/_fs/")
        .unwrap();
    assert_eq!(
        env.host.files().lookup(ID, token),
        Some(shared.join("song.mp3"))
    );
    let outside = env.home.path().join("private.txt");
    std::fs::write(&outside, "private").unwrap();
    for path in [
        outside.display().to_string(),
        format!("{}/absent.mp3", shared.display()),
    ] {
        assert_eq!(
            env.code("files.url", json!({ "path": path })).await,
            Code::Permission
        );
    }

    assert_eq!(
        env.code(gated[3].0, gated[3].1.clone()).await,
        Code::Version
    );
    for contract in ["provider.echo@2", "absent.thing@1"] {
        assert_eq!(
            env.code("contracts.request", json!({ "id": contract }))
                .await,
            Code::Unavailable
        );
    }
    assert_eq!(
        env.code("contracts.request", json!({ "id": "other.thing@1" }))
            .await,
        Code::Permission
    );
    assert_eq!(
        env.code("contracts.request", json!({ "id": "nonsense" }))
            .await,
        Code::Invalid
    );
    assert_eq!(
        env.call(gated[4].0, gated[4].1.clone()).await,
        Ok(Value::Null)
    );
    env.host.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn backend_calls_map_every_failure_to_a_bridge_code() {
    let env = Env::new();
    assert_eq!(
        env.code("backend.call", json!({ "method": "mode" })).await,
        Code::Unavailable
    );
    env.install(|_| {});

    let mode = env
        .call("backend.call", json!({ "method": "mode" }))
        .await
        .unwrap();
    assert_eq!(mode["mode"], "service");
    let slow = json!({ "method": "sleep", "params": { "seconds": 30 }, "timeoutMs": 200 });
    assert_eq!(env.code("backend.call", slow).await, Code::Timeout);
    assert_eq!(
        env.code("backend.call", json!({ "method": "absent" }))
            .await,
        Code::Invalid
    );
    assert_eq!(
        env.code("backend.call", json!({ "method": "sleep", "params": {} }))
            .await,
        Code::Invalid
    );
    let open = json!({ "method": "host", "params": { "method": "open", "target": "https://example.org" } });
    assert_eq!(env.code("backend.call", open).await, Code::Permission);
    assert_eq!(
        env.code("backend.call", json!({ "method": "crash" })).await,
        Code::Backend
    );
    assert_eq!(env.code("backend.call", json!({})).await, Code::Invalid);
    assert_eq!(
        env.code(
            "backend.call",
            json!({ "method": "mode", "timeoutMs": "soon" })
        )
        .await,
        Code::Invalid
    );
    assert_eq!(env.code("absent.method", json!({})).await, Code::Invalid);
    let stranger = dispatch(&env.host, ID, "absent", "storage.keys", Value::Null).await;
    assert_eq!(stranger.unwrap_err().code, Code::Invalid);
    let absent = dispatch(&env.host, "absent", PAGE, "storage.keys", Value::Null).await;
    assert_eq!(absent.unwrap_err().code, Code::Unavailable);

    env.host.installer().set_enabled(ID, false).unwrap();
    assert_eq!(
        env.code("storage.keys", Value::Null).await,
        Code::Unavailable
    );
    env.host.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn storage_is_capped_and_settings_stay_in_the_namespace() {
    let env = Env::new();
    env.install(|_| {});
    assert_eq!(
        env.call(
            "storage.set",
            json!({ "key": "view", "value": { "tab": 2 } })
        )
        .await,
        Ok(Value::Null)
    );
    assert_eq!(
        env.call("storage.get", json!({ "key": "view" })).await,
        Ok(json!({ "tab": 2 }))
    );
    assert_eq!(
        env.call("storage.get", json!({ "key": "absent" })).await,
        Ok(Value::Null)
    );
    let oversized = json!({ "key": "blob", "value": "x".repeat(1024 * 1024) });
    let refused = env.call("storage.set", oversized).await.unwrap_err();
    assert_eq!(refused.code, Code::Invalid);
    assert_eq!(refused.data.unwrap()["quota"], 1024 * 1024);
    assert_eq!(
        env.call("storage.keys", Value::Null).await,
        Ok(json!(["view"]))
    );
    assert_eq!(
        env.call("storage.remove", json!({ "key": "view" })).await,
        Ok(json!(true))
    );
    for params in [
        json!({}),
        json!({ "key": "" }),
        json!({ "key": "k".repeat(257) }),
    ] {
        assert_eq!(env.code("storage.get", params).await, Code::Invalid);
    }
    assert_eq!(
        env.code("storage.set", json!({ "key": "view" })).await,
        Code::Invalid
    );

    assert_eq!(
        env.call("settings.set", json!({ "key": "interval", "value": 5 }))
            .await,
        Ok(Value::Null)
    );
    assert_eq!(
        env.call("settings.get", json!({ "key": "interval" })).await,
        Ok(json!(5))
    );
    let stored = veronica_core::Settings::load(&env.directories.settings_file()).unwrap();
    assert_eq!(stored.get("ext.fixture.interval"), Some(&json!(5)));
    for key in ["", "a b", "../appearance", "a/b"] {
        assert_eq!(
            env.code("settings.set", json!({ "key": key, "value": 1 }))
                .await,
            Code::Invalid
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn navigation_and_interface_requests_reach_the_shell() {
    let env = Env::new();
    env.install(|_| {});
    env.call("nav.open", json!({ "page": PAGE, "params": { "tab": 1 } }))
        .await
        .unwrap();
    env.call("nav.setBadge", json!({ "badge": 3 }))
        .await
        .unwrap();
    env.call("nav.setTitle", json!({ "title": "Now playing" }))
        .await
        .unwrap();
    env.call("ui.toast", json!({ "message": "Saved", "kind": "success" }))
        .await
        .unwrap();
    let actions: Vec<Value> = env
        .shell
        .events(INTERFACE)
        .iter()
        .map(|event| {
            assert_eq!(
                (&event["extension"], &event["page"]),
                (&json!(ID), &json!(PAGE))
            );
            event["action"].clone()
        })
        .collect();
    assert_eq!(actions, ["open", "badge", "title", "toast"]);
    for (method, params) in [
        ("nav.open", json!({ "page": "absent" })),
        ("nav.setBadge", json!({ "badge": "far too long a badge" })),
        ("nav.setTitle", json!({ "title": "" })),
        ("ui.toast", json!({ "message": "Saved", "kind": "loud" })),
        ("ui.confirm", json!({})),
    ] {
        assert_eq!(env.code(method, params).await, Code::Invalid, "{method}");
    }

    let host = Arc::clone(&env.host);
    let shell = Arc::clone(&env.shell);
    let answering = tokio::spawn(async move {
        loop {
            let asked = shell
                .events(INTERFACE)
                .into_iter()
                .find(|event| event["action"] == "confirm");
            if let Some(request) = asked.and_then(|event| event["request"].as_u64()) {
                return host.answer(request, true);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    let confirmed = env
        .call(
            "ui.confirm",
            json!({ "title": "Delete it?", "destructive": true }),
        )
        .await;
    assert_eq!(confirmed, Ok(json!(true)));
    assert!(answering.await.unwrap());
    assert!(!env.host.answer(999, true));
}

#[tokio::test(flavor = "multi_thread")]
async fn backend_events_reach_only_subscribed_pages() {
    let env = Env::new();
    env.install(|_| {});
    let grant = Grant {
        id: ID.to_string(),
        permissions: Permissions::default(),
    };
    let emit = |topic: &str| {
        let params = TopicParams {
            topic: topic.to_string(),
            payload: json!(1),
        };
        let notification = Notification::new::<EventEmit>(&params).unwrap();
        env.host.services().notification(&grant, &notification);
    };
    emit("tick");
    assert!(env.shell.events(EVENT).is_empty());
    env.call("backend.subscribe", json!({ "topic": "tick" }))
        .await
        .unwrap();
    emit("tick");
    emit("other");
    let events = env.shell.events(EVENT);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["pages"], json!([PAGE]));
    assert_eq!(env.host.open_pages(ID), [PAGE]);
    env.call("backend.unsubscribe", json!({ "topic": "tick" }))
        .await
        .unwrap();
    env.host.close_page(ID, PAGE);
    emit("tick");
    assert_eq!(env.shell.events(EVENT).len(), 1);
    assert!(env.host.open_pages(ID).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn jobs_install_switch_and_remove_an_extension() {
    let env = Env::new();
    let source = env.package(|manifest| {
        manifest["activation"] = json!({ "mode": "background" });
    });
    let installed = env.finish(Action::sideload(source).unwrap()).await;
    assert_eq!(
        (installed.phase, installed.kind),
        (Phase::Succeeded, Kind::Sideload)
    );
    assert_eq!(installed.extension, ID);
    let phases: Vec<Value> = env
        .shell
        .events(JOB)
        .iter()
        .map(|job| job["phase"].clone())
        .collect();
    assert_eq!(phases, ["queued", "running", "succeeded"]);
    assert_eq!(env.shell.events(CHANGED), [json!({ "extension": ID })]);

    let row = library::row(&env.host, ID).await.unwrap();
    assert_eq!(
        (row.state, row.enabled, row.update),
        ("running", true, None)
    );
    assert!(row.pid.is_some() && row.manifest.is_some());

    let disabled = env.finish(Action::on(Kind::Disable, ID.to_string())).await;
    assert_eq!(disabled.phase, Phase::Succeeded);
    let row = library::row(&env.host, ID).await.unwrap();
    assert_eq!((row.state, row.enabled), ("stopped", false));
    assert_eq!(
        env.code("backend.call", json!({ "method": "mode" })).await,
        Code::Unavailable
    );

    let enabled = env.finish(Action::on(Kind::Enable, ID.to_string())).await;
    assert_eq!(enabled.phase, Phase::Succeeded);
    assert_eq!(library::row(&env.host, ID).await.unwrap().state, "running");

    let rollback = env.finish(Action::on(Kind::Rollback, ID.to_string())).await;
    assert_eq!(rollback.phase, Phase::Failed);
    assert!(rollback.error.unwrap().contains("no previous version"));
    let update = env.finish(Action::on(Kind::Update, ID.to_string())).await;
    assert!(update
        .error
        .unwrap()
        .contains("not installed from the catalog"));

    let removed = env.finish(Action::on(Kind::Purge, ID.to_string())).await;
    assert_eq!(removed.phase, Phase::Succeeded);
    assert!(library::rows(&env.host).await.unwrap().is_empty());
    assert!(!env.host.layout().data_dir(ID).exists());

    let kept = Jobs::load(env.host.layout().jobs_file()).list();
    assert_eq!(kept, env.host.jobs().list());
    assert_eq!(kept.len(), 6);
    assert!(Action::sideload(env.home.path().join("absent")).is_err());
    env.host.stop().await;
}

#[test]
fn jobs_cut_short_by_a_restart_are_marked_failed() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("jobs.json");
    let running = json!([{
        "id": 4, "kind": "install", "extension": "media", "phase": "running",
        "message": "Downloading and verifying", "error": null,
        "startedAt": "2026-11-24T12:00:00Z", "finishedAt": null
    }]);
    std::fs::write(&file, running.to_string()).unwrap();
    let jobs = Jobs::load(file).list();
    assert_eq!(jobs[0].phase, Phase::Failed);
    assert!(jobs[0].error.is_some() && jobs[0].finished_at.is_some());
}

#[test]
fn the_catalog_shelf_searches_the_seed_offline() {
    let env = Env::new();
    let everything = library::search(&env.host, "", None).unwrap();
    assert_eq!(everything.provenance, "seed");
    let nothing = library::search(&env.host, "no such extension anywhere", None).unwrap();
    assert!(nothing.extensions.is_empty());
    assert!(library::info(&env.host, "absent").is_err());
    let failure = serde_json::to_value(Failure {
        code: Code::Version,
        message: "m".to_string(),
        data: None,
    })
    .unwrap();
    assert_eq!(failure, json!({ "code": "E_VERSION", "message": "m" }));
}
