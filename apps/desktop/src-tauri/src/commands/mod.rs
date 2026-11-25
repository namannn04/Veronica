mod attention;
mod emoji;
mod legacy;
mod maintenance;
mod shortcuts;
mod system;

pub(crate) use attention::*;
pub(crate) use emoji::*;
pub(crate) use legacy::*;
pub(crate) use maintenance::*;
pub(crate) use shortcuts::*;
pub(crate) use system::*;

type CommandResult<T> = Result<T, String>;

fn fail(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}

#[cfg(test)]
fn keys(value: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
    if let serde_json::Value::Object(map) = value {
        for (key, nested) in map {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            out.push(path.clone());
            keys(nested, &path, out);
        }
    }
}

#[cfg(test)]
fn assert_camel_case<T: serde::Serialize>(value: &T, label: &str) {
    let json = serde_json::to_value(value).expect("serialisable");
    let mut found = Vec::new();
    keys(&json, "", &mut found);
    assert!(!found.is_empty(), "{label} serialised to nothing");
    for path in &found {
        let leaf = path.rsplit('.').next().unwrap();
        assert!(
            !leaf.contains('_'),
            "{label} exposes snake_case at {path}; the interface reads camelCase"
        );
    }
}
