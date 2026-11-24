use serde::Serialize;
use serde_json::json;
use veronica_protocol::manifest::{HostPermission, Permissions, ShellService};
use veronica_protocol::rpc::{
    error_code, ErrorObject, HostContractPublish, HostContractRequest, HostLog, HostNotify,
    HostOpen, HostPromptSecret, HostSecretGet, HostSecretSet, HostSettingsGet, HostSettingsSet,
    HostShellAction, HostSurfaceUpdate, Request, RequestMethod,
};

const SETTINGS_PREFIX: &str = "ext.";

pub fn settings_namespace(id: &str) -> String {
    format!("{SETTINGS_PREFIX}{id}.")
}

fn name_of(grant: &impl Serialize) -> String {
    serde_json::to_value(grant)
        .ok()
        .and_then(|value| value.as_str().map(String::from))
        .unwrap_or_default()
}

pub fn denied(id: &str, permission: &str, reason: &str) -> ErrorObject {
    ErrorObject::new(
        error_code::PERMISSION_DENIED,
        format!("{id} {reason} ({permission})"),
    )
    .with_data(json!({ "permission": permission }))
}

fn host_grant(id: &str, granted: &Permissions, needed: HostPermission) -> Result<(), ErrorObject> {
    if granted.host.contains(&needed) {
        Ok(())
    } else {
        Err(denied(
            id,
            &name_of(&needed),
            "was not granted this permission",
        ))
    }
}

fn shell_grant(id: &str, granted: &Permissions, needed: ShellService) -> Result<(), ErrorObject> {
    if granted.shell.contains(&needed) {
        Ok(())
    } else {
        Err(denied(
            id,
            &name_of(&needed),
            "was not granted this shell service",
        ))
    }
}

fn settings_read(id: &str, granted: &Permissions, key: &str) -> Result<(), ErrorObject> {
    if key.starts_with(&settings_namespace(id)) {
        Ok(())
    } else if key.starts_with(SETTINGS_PREFIX) {
        Err(denied(id, key, "cannot read another extension's settings"))
    } else {
        host_grant(id, granted, HostPermission::SettingsRead)
    }
}

fn settings_write(id: &str, key: &str) -> Result<(), ErrorObject> {
    if key.starts_with(&settings_namespace(id)) {
        Ok(())
    } else {
        Err(denied(id, key, "can only write its own settings"))
    }
}

pub fn check(id: &str, granted: &Permissions, request: &Request) -> Result<(), ErrorObject> {
    let method = request.method.as_str();
    match method {
        HostLog::NAME => Ok(()),
        HostSettingsGet::NAME => {
            settings_read(id, granted, &request.params::<HostSettingsGet>()?.key)
        }
        HostSettingsSet::NAME => settings_write(id, &request.params::<HostSettingsSet>()?.key),
        HostNotify::NAME => host_grant(id, granted, HostPermission::Notifications),
        HostSurfaceUpdate::NAME => host_grant(id, granted, HostPermission::Surfaces),
        HostContractPublish::NAME | HostContractRequest::NAME => {
            host_grant(id, granted, HostPermission::Contracts)
        }
        HostShellAction::NAME => {
            shell_grant(id, granted, request.params::<HostShellAction>()?.action)
        }
        HostSecretGet::NAME | HostSecretSet::NAME | HostPromptSecret::NAME => {
            host_grant(id, granted, HostPermission::Secrets)
        }
        HostOpen::NAME => host_grant(id, granted, HostPermission::OpenExternal),
        _ => Err(ErrorObject::method_not_found(method)),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};
    use veronica_protocol::manifest::{HostPermission, Permissions, ShellService};
    use veronica_protocol::rpc::{error_code, JsonRpcVersion, Request, RequestId};

    use super::check;

    fn request(method: &str, params: Value) -> Request {
        Request {
            jsonrpc: JsonRpcVersion,
            id: RequestId::Number(1),
            method: method.to_string(),
            params: Some(params),
        }
    }

    fn outcome(granted: &Permissions, method: &str, params: Value) -> Result<(), i64> {
        check("probe", granted, &request(method, params)).map_err(|error| error.code)
    }

    #[test]
    fn every_host_method_needs_its_grant() {
        let none = Permissions::default();
        let all = Permissions {
            host: vec![
                HostPermission::Notifications,
                HostPermission::Surfaces,
                HostPermission::Contracts,
                HostPermission::SettingsRead,
                HostPermission::Secrets,
                HostPermission::OpenExternal,
            ],
            shell: vec![ShellService::PickColor],
            ..Permissions::default()
        };
        let cases = [
            ("host.notify", json!({ "title": "t" })),
            (
                "host.surface.update",
                json!({ "surface": "s", "state": {} }),
            ),
            (
                "host.contract.publish",
                json!({ "contract": "a.b@1", "payload": {} }),
            ),
            ("host.contract.request", json!({ "contract": "a.b@1" })),
            ("host.shell.action", json!({ "action": "pickColor" })),
            ("host.secret.get", json!({ "key": "k" })),
            ("host.secret.set", json!({ "key": "k", "value": null })),
            ("host.prompt.secret", json!({ "prompt": "p" })),
            ("host.open", json!({ "target": "https://example.org" })),
            ("host.settings.get", json!({ "key": "appearance" })),
        ];
        for (method, params) in cases {
            assert_eq!(
                outcome(&none, method, params.clone()),
                Err(error_code::PERMISSION_DENIED),
                "{method} without a grant"
            );
            assert_eq!(
                outcome(&all, method, params),
                Ok(()),
                "{method} with a grant"
            );
        }
    }

    #[test]
    fn settings_are_scoped_to_the_extension_namespace() {
        let none = Permissions::default();
        let reader = Permissions {
            host: vec![HostPermission::SettingsRead],
            ..Permissions::default()
        };
        let get = |granted: &Permissions, key: &str| {
            outcome(granted, "host.settings.get", json!({ "key": key }))
        };
        let set = |granted: &Permissions, key: &str| {
            outcome(
                granted,
                "host.settings.set",
                json!({ "key": key, "value": 1 }),
            )
        };
        assert_eq!(get(&none, "ext.probe.interval"), Ok(()));
        assert_eq!(set(&none, "ext.probe.interval"), Ok(()));
        assert_eq!(
            get(&reader, "ext.other.token"),
            Err(error_code::PERMISSION_DENIED)
        );
        assert_eq!(
            get(&none, "ext.probed.x"),
            Err(error_code::PERMISSION_DENIED)
        );
        assert_eq!(
            set(&reader, "appearance"),
            Err(error_code::PERMISSION_DENIED)
        );
        assert_eq!(
            set(&reader, "ext.other.x"),
            Err(error_code::PERMISSION_DENIED)
        );
    }

    #[test]
    fn shell_actions_need_the_named_service_and_logging_is_free() {
        let color = Permissions {
            shell: vec![ShellService::PickColor],
            ..Permissions::default()
        };
        let action = |name: &str| outcome(&color, "host.shell.action", json!({ "action": name }));
        assert_eq!(action("pickColor"), Ok(()));
        assert_eq!(action("focusDim"), Err(error_code::PERMISSION_DENIED));
        assert_eq!(action("nonsense"), Err(error_code::INVALID_PARAMS));
        assert_eq!(
            outcome(
                &color,
                "host.log",
                json!({ "level": "info", "message": "m" })
            ),
            Ok(())
        );
        assert_eq!(
            outcome(&color, "host.unknown", json!({})),
            Err(error_code::METHOD_NOT_FOUND)
        );
    }
}
