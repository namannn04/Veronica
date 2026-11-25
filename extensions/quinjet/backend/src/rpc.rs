use serde::{Deserialize, Serialize};
use serde_json::Value;
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::protocol::rpc::{error_code, ErrorObject};
use veronica_sdk::rpc::{failure, params, reply};
use veronica_sdk::settings::Settings;
use veronica_system::quinjet::{self, LaunchOptions, Project};

pub(crate) const METHODS: [&str; 2] = ["projects", "open"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuinjetBoard {
    pub installed: bool,
    pub projects: Vec<Project>,
}

#[derive(Deserialize)]
struct OpenParams {
    worktree: String,
}

pub(crate) fn launch_options(settings: &Settings) -> LaunchOptions {
    LaunchOptions {
        theme: settings.string("quinjetTheme").map(str::to_string),
        appearance: veronica_core::appearance::scheme(
            settings.string("appearance").unwrap_or("system"),
        )
        .name()
        .map(str::to_string),
    }
}

fn board() -> anyhow::Result<QuinjetBoard> {
    if !quinjet::installed() {
        return Ok(QuinjetBoard {
            installed: false,
            projects: Vec::new(),
        });
    }
    Ok(QuinjetBoard {
        installed: true,
        projects: quinjet::projects()?,
    })
}

fn open(worktree: &str) -> anyhow::Result<()> {
    let settings = Settings::load(&AppDirectories::current()?.settings_file())?;
    quinjet::open_terminal(worktree, &launch_options(&settings))
}

fn joined(error: &tokio::task::JoinError) -> ErrorObject {
    ErrorObject::new(error_code::INTERNAL_ERROR, error.to_string())
}

pub(crate) async fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    let fail = |error: anyhow::Error| failure(&error);
    match method {
        "projects" => {
            let board = tokio::task::spawn_blocking(board)
                .await
                .map_err(|error| joined(&error))?
                .map_err(fail)?;
            reply(&board)
        }
        "open" => {
            let OpenParams { worktree } = params(arguments)?;
            tokio::task::spawn_blocking(move || open(&worktree))
                .await
                .map_err(|error| joined(&error))?
                .map_err(fail)?;
            Ok(Value::Null)
        }
        other => Err(ErrorObject::method_not_found(other)),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn launch_options_follow_the_theme_and_appearance_settings() {
        let mut settings = Settings::default();
        settings.set("appearance", json!("midnight"));
        settings.set("quinjetTheme", json!("tokyo"));
        let options = launch_options(&settings);
        assert_eq!(options.theme.as_deref(), Some("tokyo"));
        assert_eq!(options.appearance.as_deref(), Some("dark"));
        assert_eq!(launch_options(&Settings::default()).theme, None);
    }
}
