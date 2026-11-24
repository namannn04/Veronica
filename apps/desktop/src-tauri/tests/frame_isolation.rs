use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

const DEADLINE: Duration = Duration::from_mins(2);
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/extensions");
const FORGED_FRAME_INVOKES: usize = 20;
const REQUIRED_ATTEMPTS: [&str; 16] = [
    "own window.__TAURI_INTERNALS__",
    "own window.ipc",
    "parent.__TAURI_INTERNALS__",
    "top.__TAURI_INTERNALS__",
    "parent.document",
    "webkit.messageHandlers.ipc forged invoke",
    "parent.postMessage forged invoke",
    "fetch cors ipc://localhost/settings_set",
    "fetch no-cors http://ipc.localhost/settings_set",
    "xhr ipc://localhost/settings_set",
    "xhr tauri://localhost/index.html",
    "fetch cors veronica-ext://neighbour/secret.js",
    "frame of another extension",
    "top.location assignment",
    "window.open",
    "form post to ipc://localhost",
];

struct Run {
    outcome: Value,
    settings: String,
    log: String,
}

fn private_directory(parent: &Path, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let directory = parent.join(name);
    std::fs::create_dir_all(&directory).expect("directory");
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
        .expect("permissions");
    directory
}

fn display_authority() -> Option<PathBuf> {
    std::env::var_os("XAUTHORITY")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".Xauthority")))
        .filter(|path| path.exists())
}

fn run_probe() -> Run {
    let sandbox = tempfile::tempdir().expect("temporary directory");
    let home = sandbox.path();
    let report = home.join("report.json");
    let log = home.join("stderr.log");
    let config = private_directory(home, "config");

    let mut command = Command::new(env!("CARGO_BIN_EXE_veronica"));
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_DATA_HOME", private_directory(home, "data"))
        .env("XDG_STATE_HOME", private_directory(home, "state"))
        .env("XDG_CACHE_HOME", private_directory(home, "cache"))
        .env("XDG_RUNTIME_DIR", private_directory(home, "runtime"))
        .env("VERONICA_EXTENSIONS_DEV", FIXTURES)
        .env("VERONICA_FRAME_PROBE", &report)
        .env("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&log).expect("log file"));
    if let Some(authority) = display_authority() {
        command.env("XAUTHORITY", authority);
    }
    let mut child = command.spawn().expect("the application starts");

    let started = Instant::now();
    let finished = loop {
        if child.try_wait().expect("child status").is_some() {
            break true;
        }
        if started.elapsed() > DEADLINE {
            break false;
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    if !finished {
        let _ = child.kill();
        let _ = child.wait();
    }

    let log = std::fs::read_to_string(&log).unwrap_or_default();
    let outcome = std::fs::read_to_string(&report)
        .ok()
        .and_then(|body| serde_json::from_str(&body).ok())
        .unwrap_or_else(|| panic!("the probe wrote no report (finished: {finished})\n{log}"));
    let settings =
        std::fs::read_to_string(config.join("veronica").join("settings.json")).unwrap_or_default();
    Run {
        outcome,
        settings,
        log,
    }
}

fn rejected_keys(log: &str, marker: &str) -> usize {
    log.lines()
        .filter(|line| line.contains("refused an invoke with the wrong key"))
        .filter(|line| {
            line.split(", received ")
                .nth(1)
                .is_some_and(|received| received.starts_with(marker))
        })
        .count()
}

#[test]
#[ignore = "needs a display, a session bus and the built interface, see tests/README.md"]
fn a_hostile_bundle_cannot_leave_its_frame() {
    let run = run_probe();
    let frontend = &run.outcome["frontend"];
    let host = &frontend["host"];
    let report = &frontend["report"];
    assert!(report.is_object(), "the frame never reported: {frontend}");

    println!("exposure: {}", report["exposure"]);
    println!("assets: {}", report["assets"]);
    println!("host: {host}");
    println!("windows: {}", run.outcome["windows"]);
    let attempts = report["attempts"].as_array().expect("attempts");
    for attempt in attempts {
        println!(
            "{:<8} {} ({})",
            if attempt["escaped"] == false {
                "blocked"
            } else {
                "ESCAPED"
            },
            attempt["name"].as_str().unwrap_or_default(),
            attempt["detail"].as_str().unwrap_or_default()
        );
    }
    let frame_rejections =
        rejected_keys(&run.log, "") - rejected_keys(&run.log, "host-control-guess");
    let host_rejections = rejected_keys(&run.log, "host-control-guess");
    println!("forged frame messages rejected at the key check: {frame_rejections}");
    println!("forged host messages rejected at the key check: {host_rejections}");

    assert_eq!(report["echo"]["sent"], report["echo"]["received"]);
    assert!(report["echo"]["sent"].is_string());
    assert!(host["echoes"].as_u64().is_some_and(|echoes| echoes >= 1));
    assert_eq!(report["ready"]["hostApi"], 1);
    assert_eq!(report["ready"]["extension"], "hostile");

    assert_eq!(report["exposure"]["origin"], "null");
    assert_eq!(report["assets"]["script"], true);
    assert_eq!(report["assets"]["style"], "loaded");
    assert_eq!(report["assets"]["font"], true);
    assert_eq!(report["assets"]["fontCheck"], true);

    let names: Vec<&str> = attempts
        .iter()
        .filter_map(|attempt| attempt["name"].as_str())
        .collect();
    for required in REQUIRED_ATTEMPTS {
        assert!(names.contains(&required), "the bundle skipped: {required}");
    }
    let escaped: Vec<&Value> = attempts
        .iter()
        .filter(|attempt| attempt["escaped"] != false)
        .collect();
    assert!(escaped.is_empty(), "escapes succeeded: {escaped:#?}");
    assert_eq!(report["decoyCallbacks"], serde_json::json!([]));

    assert_eq!(run.outcome["escapeEvents"], 0);
    assert_eq!(host["control"], "ok");
    assert!(
        run.settings.contains("frameProbeControl"),
        "{}",
        run.settings
    );
    assert!(
        !run.settings.contains("frameEscapeCanary"),
        "{}",
        run.settings
    );
    assert!(
        !run.settings.contains("frameProbeForgedControl"),
        "{}",
        run.settings
    );

    assert!(
        !run.log.contains("__TAURI_INVOKE_KEY__ expected"),
        "{}",
        run.log
    );
    assert_eq!(host["forged"], "posted");
    assert_eq!(host_rejections, 1);
    let exposed = report["exposure"]["ipcHandler"] != "undefined";
    assert_eq!(
        frame_rejections,
        if exposed { FORGED_FRAME_INVOKES } else { 0 }
    );

    let windows = run.outcome["windows"].as_object().expect("windows");
    assert_eq!(windows.keys().collect::<Vec<_>>(), ["main"]);
    let location = "tauri://localhost/index.html?surface=frame-probe";
    assert!(windows["main"]
        .as_str()
        .is_some_and(|url| url.starts_with(location)));
    assert!(host["location"]
        .as_str()
        .is_some_and(|url| url.starts_with(location)));
    assert_eq!(host["frames"], 1);
}
