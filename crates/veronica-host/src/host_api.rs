use std::fs::{self, OpenOptions};
use std::io::Write;

use chrono::{SecondsFormat, Utc};
use serde_json::Value;
use veronica_protocol::manifest::Permissions;
use veronica_protocol::rpc::{error_code, ErrorObject, Notification, Request};

use crate::layout::Layout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub id: String,
    pub permissions: Permissions,
}

pub struct HostServices {
    layout: Layout,
}

impl HostServices {
    pub fn new(layout: Layout) -> Self {
        Self { layout }
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

    pub fn handle(&self, _grant: &Grant, request: &Request) -> Result<Value, ErrorObject> {
        Err(ErrorObject::new(
            error_code::UNAVAILABLE,
            format!("{} is not available in this host yet", request.method),
        ))
    }

    pub fn notification(&self, _grant: &Grant, _notification: &Notification) {}
}
