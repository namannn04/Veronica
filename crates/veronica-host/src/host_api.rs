use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;
use veronica_core::paths::AppDirectories;
use veronica_core::settings::Settings;
use veronica_protocol::manifest::Permissions;
use veronica_protocol::rpc::{
    error_code, ErrorObject, EventEmit, HostLog, HostNotify, HostSettingsGet, HostSettingsSet,
    HostSurfaceUpdate, Notification, NotifyParams, Progress, ProgressParams, Request,
    RequestMethod, SurfaceUpdateParams, TopicParams, ValueResult,
};

use crate::layout::Layout;
use crate::permissions;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub id: String,
    pub permissions: Permissions,
}

pub trait Frontend: Send + Sync {
    fn notify(&self, extension: &str, params: NotifyParams) -> Result<(), ErrorObject>;

    fn surface(&self, _extension: &str, _params: SurfaceUpdateParams) -> Result<(), ErrorObject> {
        Err(unavailable("surfaces need the desktop app"))
    }

    fn event(&self, _extension: &str, _params: TopicParams) {}

    fn progress(&self, _extension: &str, _params: ProgressParams) {}
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Headless;

impl Frontend for Headless {
    fn notify(&self, _extension: &str, _params: NotifyParams) -> Result<(), ErrorObject> {
        Err(unavailable("notifications need the desktop app"))
    }
}

fn unavailable(message: &str) -> ErrorObject {
    ErrorObject::new(error_code::UNAVAILABLE, message)
}

fn internal(error: &anyhow::Error) -> ErrorObject {
    ErrorObject::new(error_code::INTERNAL_ERROR, format!("{error:#}"))
}

fn encode<T: Serialize>(value: &T) -> Result<Value, ErrorObject> {
    serde_json::to_value(value)
        .map_err(|error| ErrorObject::new(error_code::INTERNAL_ERROR, error.to_string()))
}

pub struct HostServices {
    layout: Layout,
    settings_file: PathBuf,
    frontend: Arc<dyn Frontend>,
    settings_lock: Mutex<()>,
}

impl HostServices {
    pub fn new(layout: Layout, settings_file: PathBuf) -> Self {
        Self {
            layout,
            settings_file,
            frontend: Arc::new(Headless),
            settings_lock: Mutex::new(()),
        }
    }

    pub fn current() -> Result<Self> {
        let directories = AppDirectories::current()?;
        Ok(Self::new(
            Layout::new(&directories),
            directories.settings_file(),
        ))
    }

    #[must_use]
    pub fn with_frontend(mut self, frontend: Arc<dyn Frontend>) -> Self {
        self.frontend = frontend;
        self
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn log(&self, id: &str, message: &str) {
        let path = self.layout.log_file(id);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let stamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{stamp} host: {message}");
        }
    }

    pub fn handle(&self, grant: &Grant, request: &Request) -> Result<Value, ErrorObject> {
        permissions::check(&grant.id, &grant.permissions, request)?;
        match request.method.as_str() {
            HostLog::NAME => {
                let params = request.params::<HostLog>()?;
                let level = serde_json::to_value(params.level).unwrap_or_default();
                let level = level.as_str().unwrap_or("info");
                self.log(&grant.id, &format!("{level}: {}", params.message));
                Ok(Value::Null)
            }
            HostSettingsGet::NAME => {
                let key = request.params::<HostSettingsGet>()?.key;
                let settings = Settings::load(&self.settings_file).map_err(|e| internal(&e))?;
                encode(&ValueResult {
                    value: settings.get(&key).cloned(),
                })
            }
            HostSettingsSet::NAME => {
                let params = request.params::<HostSettingsSet>()?;
                let _guard = self
                    .settings_lock
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                let mut settings = Settings::load(&self.settings_file).map_err(|e| internal(&e))?;
                if params.value.is_null() {
                    settings.remove(&params.key);
                } else {
                    settings.set(&params.key, params.value);
                }
                settings
                    .save(&self.settings_file)
                    .map_err(|e| internal(&e))?;
                Ok(Value::Null)
            }
            HostNotify::NAME => {
                let params = request.params::<HostNotify>()?;
                self.frontend.notify(&grant.id, params)?;
                Ok(Value::Null)
            }
            HostSurfaceUpdate::NAME => {
                let params = request.params::<HostSurfaceUpdate>()?;
                self.frontend.surface(&grant.id, params)?;
                Ok(Value::Null)
            }
            method => Err(unavailable(&format!(
                "{method} is not available in this host yet"
            ))),
        }
    }

    pub fn notification(&self, grant: &Grant, notification: &Notification) {
        if notification.is::<EventEmit>() {
            if let Ok(params) = notification.params::<EventEmit>() {
                self.frontend.event(&grant.id, params);
            }
        } else if notification.is::<Progress>() {
            if let Ok(params) = notification.params::<Progress>() {
                self.frontend.progress(&grant.id, params);
            }
        }
    }
}
