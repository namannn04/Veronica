use super::{fail, CommandResult};

#[tauri::command]
pub(crate) async fn audit_site(
    site: String,
    limit: usize,
    concurrency: usize,
) -> CommandResult<veronica_audit::Report> {
    veronica_audit::audit(&site, limit, concurrency)
        .await
        .map_err(fail)
}
