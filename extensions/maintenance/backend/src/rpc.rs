use serde::{Deserialize, Serialize};
use serde_json::Value;
use veronica_sdk::protocol::rpc::{error_code, ErrorObject};
use veronica_sdk::rpc::{failure, params, reply};
use veronica_system::packages::{self, Source};

pub(crate) const METHODS: [&str; 8] = [
    "sources",
    "updates",
    "inventory",
    "update",
    "removalPlan",
    "cleanerCategories",
    "cleanerScan",
    "cleanerClean",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageSource {
    source: String,
    available: bool,
    needs_root: bool,
}

#[derive(Deserialize)]
struct UpdateParams {
    source: String,
    #[serde(default)]
    names: Vec<String>,
}

#[derive(Deserialize)]
struct RemovalParams {
    package: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ScanParams {
    categories: Vec<String>,
}

#[derive(Deserialize)]
struct CleanParams {
    items: Vec<veronica_core::cleaner::Item>,
}

fn home() -> Result<std::path::PathBuf, ErrorObject> {
    veronica_sdk::paths::home_dir().ok_or_else(|| {
        ErrorObject::new(error_code::UNAVAILABLE, "cannot resolve the home directory")
    })
}

fn clean(items: Vec<veronica_core::cleaner::Item>) -> Result<Value, ErrorObject> {
    let home = home()?;
    let mut report = veronica_core::cleaner::CleanReport::default();
    for item in items {
        let target = std::path::Path::new(&item.path);
        if !target.starts_with(&home) {
            report
                .failed
                .push((item.path, "outside the home directory".to_string()));
            continue;
        }
        let bytes = veronica_core::cleaner::directory_size(target);
        match veronica_core::cleaner::trash(&home, target) {
            Ok(_) => {
                report.bytes_reclaimed = report.bytes_reclaimed.saturating_add(bytes);
                report
                    .trashed
                    .push(veronica_core::cleaner::Item { bytes, ..item });
            }
            Err(error) => report.failed.push((item.path, format!("{error:#}"))),
        }
    }
    reply(&report)
}

async fn blocking(
    work: impl FnOnce() -> Result<Value, ErrorObject> + Send + 'static,
) -> Result<Value, ErrorObject> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| failure(&error.into()))?
}

pub(crate) async fn call(method: &str, arguments: Value) -> Result<Value, ErrorObject> {
    match method {
        "sources" => reply(
            &Source::ALL
                .into_iter()
                .map(|source| PackageSource {
                    available: packages::available(source),
                    needs_root: source.needs_root(),
                    source: source.title().to_string(),
                })
                .collect::<Vec<_>>(),
        ),
        "updates" => reply(&packages::updates().await),
        "inventory" => reply(&packages::inventory().await),
        "update" => {
            let UpdateParams { source, names } = params(arguments)?;
            let source = Source::parse(&source).ok_or_else(|| {
                ErrorObject::new(
                    error_code::INVALID_PARAMS,
                    format!("unknown package source {source:?}"),
                )
            })?;
            reply(
                &packages::apply_update(source, &names)
                    .await
                    .map_err(|error| failure(&error))?,
            )
        }
        "removalPlan" => {
            let RemovalParams { package } = params(arguments)?;
            reply(
                &packages::removal_plan(&package)
                    .await
                    .map_err(|error| failure(&error))?,
            )
        }
        "cleanerCategories" => reply(&veronica_core::cleaner::CATEGORIES),
        "cleanerScan" => {
            let ScanParams { categories } = params(arguments)?;
            blocking(move || {
                let selected = (!categories.is_empty()).then_some(categories);
                reply(&veronica_core::cleaner::scan_caches(
                    &home()?,
                    selected.as_deref(),
                ))
            })
            .await
        }
        "cleanerClean" => {
            let CleanParams { items } = params(arguments)?;
            blocking(move || clean(items)).await
        }
        other => Err(ErrorObject::method_not_found(other)),
    }
}
