use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use serde_json::{json, Value};
use veronica_core::AppDirectories;
use veronica_host::host_api::{Frontend, HostServices};
use veronica_host::{Layout, Supervisor};
use veronica_protocol::rpc::{
    error_code, ErrorObject, NotifyParams, ProgressParams, SurfaceUpdateParams, TopicParams,
};

pub(crate) const EVENT: &str = "extension-event";
pub(crate) const PROGRESS: &str = "extension-progress";
pub(crate) const SURFACE: &str = "extension-surface";

pub(crate) trait Shell: Send + Sync {
    fn emit(&self, event: &str, payload: Value);

    fn notify(&self, extension: &str, params: NotifyParams) -> Result<(), ErrorObject>;
}

type Surfaces = BTreeMap<String, BTreeMap<String, Value>>;

#[derive(Default)]
struct Desk {
    shell: OnceLock<Arc<dyn Shell>>,
    surfaces: Mutex<Surfaces>,
}

impl Desk {
    fn emit(&self, event: &str, payload: Value) {
        if let Some(shell) = self.shell.get() {
            shell.emit(event, payload);
        }
    }
}

impl Frontend for Desk {
    fn notify(&self, extension: &str, params: NotifyParams) -> Result<(), ErrorObject> {
        let shell = self.shell.get().ok_or_else(|| {
            ErrorObject::new(error_code::UNAVAILABLE, "the desktop is still starting")
        })?;
        shell.notify(extension, params)
    }

    fn surface(&self, extension: &str, params: SurfaceUpdateParams) -> Result<(), ErrorObject> {
        self.surfaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(extension.to_string())
            .or_default()
            .insert(params.surface.clone(), params.state);
        self.emit(
            SURFACE,
            json!({ "extension": extension, "surface": params.surface }),
        );
        Ok(())
    }

    fn event(&self, extension: &str, params: TopicParams) {
        self.emit(
            EVENT,
            json!({ "extension": extension, "topic": params.topic, "payload": params.payload }),
        );
    }

    fn progress(&self, extension: &str, params: ProgressParams) {
        self.emit(
            PROGRESS,
            json!({ "extension": extension, "message": params.message, "fraction": params.fraction }),
        );
    }
}

pub(crate) struct Host {
    desk: Arc<Desk>,
    supervisor: Supervisor,
}

impl Host {
    pub(crate) fn new(directories: &AppDirectories) -> Self {
        let desk = Arc::new(Desk::default());
        let frontend: Arc<dyn Frontend> = Arc::clone(&desk) as Arc<dyn Frontend>;
        let services = HostServices::new(Layout::new(directories), directories.settings_file())
            .with_frontend(frontend);
        Self {
            desk,
            supervisor: Supervisor::new(Arc::new(services)),
        }
    }

    pub(crate) fn attach(&self, shell: Arc<dyn Shell>) {
        let _ = self.desk.shell.set(shell);
    }

    pub(crate) fn layout(&self) -> &Layout {
        self.supervisor.services().layout()
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

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    #[derive(Default)]
    pub(crate) struct RecordingShell {
        pub(crate) events: Mutex<Vec<(String, Value)>>,
        pub(crate) notices: Mutex<Vec<(String, NotifyParams)>>,
    }

    impl RecordingShell {
        pub(crate) fn emitted(&self, event: &str) -> Vec<Value> {
            let events = self.events.lock().expect("events");
            let matching = events.iter().filter(|(name, _)| name == event);
            matching.map(|(_, payload)| payload.clone()).collect()
        }
    }

    impl Shell for RecordingShell {
        fn emit(&self, event: &str, payload: Value) {
            let mut events = self.events.lock().expect("events");
            events.push((event.to_string(), payload));
        }

        fn notify(&self, extension: &str, params: NotifyParams) -> Result<(), ErrorObject> {
            let mut notices = self.notices.lock().expect("notices");
            notices.push((extension.to_string(), params));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::RecordingShell;
    use super::*;

    fn notice() -> NotifyParams {
        NotifyParams {
            title: "Done".to_string(),
            body: None,
            replace_key: None,
        }
    }

    #[test]
    fn the_desk_forwards_backend_traffic_once_a_shell_is_attached() {
        let desk = Desk::default();
        assert_eq!(
            desk.notify("probe", notice()).map_err(|error| error.code),
            Err(error_code::UNAVAILABLE)
        );
        desk.event(
            "probe",
            TopicParams {
                topic: "lost".to_string(),
                payload: Value::Null,
            },
        );

        let shell = Arc::new(RecordingShell::default());
        let _ = desk.shell.set(Arc::clone(&shell) as Arc<dyn Shell>);
        desk.notify("probe", notice()).expect("notify");
        desk.event(
            "probe",
            TopicParams {
                topic: "tick".to_string(),
                payload: json!(3),
            },
        );
        desk.progress(
            "probe",
            ProgressParams {
                message: Some("Half".to_string()),
                fraction: Some(0.5),
            },
        );
        let state = json!({ "title": "Now playing" });
        let update = SurfaceUpdateParams {
            surface: "card".to_string(),
            state: state.clone(),
        };
        desk.surface("probe", update).expect("surface");

        assert_eq!(shell.notices.lock().expect("notices").len(), 1);
        assert_eq!(
            shell.emitted(EVENT),
            [json!({ "extension": "probe", "topic": "tick", "payload": 3 })]
        );
        assert_eq!(
            shell.emitted(PROGRESS),
            [json!({ "extension": "probe", "message": "Half", "fraction": 0.5 })]
        );
        assert_eq!(
            shell.emitted(SURFACE),
            [json!({ "extension": "probe", "surface": "card" })]
        );
        assert_eq!(
            desk.surfaces.lock().expect("surfaces")["probe"]["card"],
            state
        );
    }
}
