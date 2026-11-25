use std::sync::LazyLock;

use anyhow::{Context as _, Result};
use serde_json::json;
use tokio::sync::watch;
use veronica_sdk::paths::AppDirectories;
use veronica_sdk::Context;
use veronica_usage::collector;

pub(crate) const PROGRESS: &str = "usage.progress";
pub(crate) const STARTED: &str = "usage.refreshStarted";
pub(crate) const FINISHED: &str = "usage.refreshFinished";
pub(crate) const UPDATED: &str = "usage.updated";

static RUNNING: LazyLock<watch::Sender<bool>> = LazyLock::new(|| watch::Sender::new(false));

pub(crate) fn running() -> bool {
    *RUNNING.borrow()
}

pub(crate) async fn run(context: &Context, directories: &AppDirectories) -> Result<String> {
    let claimed = RUNNING.send_if_modified(|running| !std::mem::replace(running, true));
    if !claimed {
        return join(directories).await;
    }
    let host = context.host();
    let _ = host.emit(STARTED, json!(null));
    let result = collect(context, directories).await;
    RUNNING.send_replace(false);
    let _ = host.emit(FINISHED, json!(result.is_ok()));
    if result.is_ok() {
        crate::surfaces::publish_usage(context, directories).await;
    }
    result
}

async fn join(directories: &AppDirectories) -> Result<String> {
    let mut receiver = RUNNING.subscribe();
    receiver
        .wait_for(|running| !*running)
        .await
        .context("the usage collector stopped unexpectedly")?;
    collector::read_document(&directories.usage_file())?
        .map(|document| document.generated_at)
        .context("The collection finished without any usage to show.")
}

async fn collect(context: &Context, directories: &AppDirectories) -> Result<String> {
    let script = directories.collector_script();
    collector::install_script(&script)?;
    let host = context.host().clone();
    let outcome = collector::refresh(
        &script,
        &directories.usage_dir(),
        &directories.cache,
        move |event| {
            let _ = host.emit(PROGRESS, serde_json::to_value(event).unwrap_or_default());
        },
    )
    .await?;
    let generated_at = outcome.document.generated_at;
    let _ = context.host().emit(UPDATED, json!(generated_at));
    Ok(generated_at)
}
