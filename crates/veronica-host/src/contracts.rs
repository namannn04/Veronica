use veronica_protocol::manifest::{ContractKind, Manifest};
use veronica_protocol::rpc::{error_code, ErrorObject};
use veronica_protocol::ContractId;

pub const HOST_NAMESPACE: &str = "host";
pub const HOST_CONTRACTS: [&str; 5] = [
    "host.presenter@1",
    "host.privacy@1",
    "host.appearance@1",
    "host.session@1",
    "host.capabilities@1",
];
const TOPIC_PREFIX: &str = "contract:";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractError {
    #[error("{extension} does not list {contract} in contracts.uses")]
    NotUsed { extension: String, contract: String },
    #[error("{extension} does not provide {contract} as a {kind}")]
    NotProvided {
        extension: String,
        contract: String,
        kind: &'static str,
    },
    #[error("{contract} is published by the host only")]
    HostOnly { contract: String },
    #[error("{name} is provided at version {available}, not {wanted}")]
    Version {
        name: String,
        available: u32,
        wanted: u32,
    },
    #[error("no enabled extension provides {contract}")]
    Unavailable { contract: String },
}

impl ContractError {
    pub fn code(&self) -> i64 {
        match self {
            Self::NotUsed { .. } | Self::NotProvided { .. } | Self::HostOnly { .. } => {
                error_code::PERMISSION_DENIED
            }
            Self::Version { .. } => error_code::VERSION_MISMATCH,
            Self::Unavailable { .. } => error_code::UNAVAILABLE,
        }
    }
}

impl From<ContractError> for ErrorObject {
    fn from(error: ContractError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

pub fn topic(contract: &ContractId) -> String {
    format!("{TOPIC_PREFIX}{contract}")
}

pub fn from_topic(topic: &str) -> Option<ContractId> {
    topic.strip_prefix(TOPIC_PREFIX)?.parse().ok()
}

pub fn is_host_contract(contract: &ContractId) -> bool {
    contract.namespace() == HOST_NAMESPACE
}

fn kind_name(kind: ContractKind) -> &'static str {
    match kind {
        ContractKind::Topic => "topic",
        ContractKind::Request => "request",
    }
}

pub fn may_use(manifest: &Manifest, contract: &ContractId) -> Result<(), ContractError> {
    if manifest.contracts.uses.contains(contract) {
        Ok(())
    } else {
        Err(ContractError::NotUsed {
            extension: manifest.id.clone(),
            contract: contract.to_string(),
        })
    }
}

fn provides(manifest: &Manifest, contract: &ContractId, kind: ContractKind) -> bool {
    manifest
        .contracts
        .provides
        .iter()
        .any(|provided| provided.id == *contract && provided.kind == kind)
}

pub fn may_publish(manifest: &Manifest, contract: &ContractId) -> Result<(), ContractError> {
    if is_host_contract(contract) {
        return Err(ContractError::HostOnly {
            contract: contract.to_string(),
        });
    }
    if provides(manifest, contract, ContractKind::Topic) {
        Ok(())
    } else {
        Err(ContractError::NotProvided {
            extension: manifest.id.clone(),
            contract: contract.to_string(),
            kind: kind_name(ContractKind::Topic),
        })
    }
}

pub fn provider<'a>(
    manifests: impl IntoIterator<Item = &'a Manifest>,
    contract: &ContractId,
) -> Result<String, ContractError> {
    let mut available = None;
    for manifest in manifests {
        if provides(manifest, contract, ContractKind::Request) {
            return Ok(manifest.id.clone());
        }
        let other = manifest
            .contracts
            .provides
            .iter()
            .find(|provided| provided.id.name() == contract.name());
        if let Some(other) = other {
            available.get_or_insert_with(|| other.id.version());
        }
    }
    match available {
        Some(available) => Err(ContractError::Version {
            name: contract.name().to_string(),
            available,
            wanted: contract.version(),
        }),
        None => Err(ContractError::Unavailable {
            contract: contract.to_string(),
        }),
    }
}

pub fn consumers<'a>(
    manifests: impl IntoIterator<Item = &'a Manifest>,
    contract: &ContractId,
) -> Vec<String> {
    manifests
        .into_iter()
        .filter(|manifest| manifest.contracts.uses.contains(contract))
        .map(|manifest| manifest.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn manifest(id: &str, contracts: &serde_json::Value) -> Manifest {
        serde_json::from_value(json!({
            "schema": 1,
            "id": id,
            "version": "1.0.0",
            "title": id,
            "summary": "test",
            "category": "labs",
            "engines": { "host": ">=0.1.0", "hostApi": 1 },
            "targets": ["linux-x86_64"],
            "contracts": contracts
        }))
        .expect("manifest")
    }

    fn id(text: &str) -> ContractId {
        text.parse().expect("contract id")
    }

    fn media() -> Manifest {
        manifest(
            "media",
            &json!({
                "provides": [
                    { "id": "media.nowPlaying@1", "kind": "topic" },
                    { "id": "media.control@1", "kind": "request" }
                ],
                "uses": ["host.presenter@1"]
            }),
        )
    }

    fn usage() -> Manifest {
        manifest(
            "usage",
            &json!({ "provides": [], "uses": ["media.nowPlaying@1", "media.control@2"] }),
        )
    }

    #[test]
    fn publishing_needs_a_provided_topic_and_never_a_host_contract() {
        assert_eq!(may_publish(&media(), &id("media.nowPlaying@1")), Ok(()));
        assert!(matches!(
            may_publish(&media(), &id("media.control@1")),
            Err(ContractError::NotProvided { kind: "topic", .. })
        ));
        assert!(matches!(
            may_publish(&usage(), &id("media.nowPlaying@1")),
            Err(ContractError::NotProvided { .. })
        ));
        let error = may_publish(&media(), &id("host.presenter@1")).expect_err("host only");
        assert_eq!(error.code(), error_code::PERMISSION_DENIED);
    }

    #[test]
    fn using_needs_the_contract_in_uses() {
        assert_eq!(may_use(&usage(), &id("media.nowPlaying@1")), Ok(()));
        assert_eq!(may_use(&media(), &id("host.presenter@1")), Ok(()));
        let error = may_use(&media(), &id("host.session@1")).expect_err("not used");
        assert_eq!(ErrorObject::from(error).code, error_code::PERMISSION_DENIED);
    }

    #[test]
    fn requests_route_to_the_provider_of_that_exact_version() {
        let all = [usage(), media()];
        assert_eq!(
            provider(&all, &id("media.control@1")),
            Ok("media".to_string())
        );
        let mismatch = provider(&all, &id("media.control@2")).expect_err("version");
        assert_eq!(mismatch.code(), error_code::VERSION_MISMATCH);
        assert!(matches!(
            mismatch,
            ContractError::Version {
                available: 1,
                wanted: 2,
                ..
            }
        ));
        let topic_only = provider(&all, &id("media.nowPlaying@1")).expect_err("topic");
        assert!(matches!(topic_only, ContractError::Version { .. }));
        let missing = provider(&all, &id("calendar.today@1")).expect_err("missing");
        assert_eq!(missing.code(), error_code::UNAVAILABLE);
    }

    #[test]
    fn consumers_and_topics_follow_uses() {
        let all = [usage(), media()];
        assert_eq!(consumers(&all, &id("media.nowPlaying@1")), ["usage"]);
        assert_eq!(consumers(&all, &id("host.presenter@1")), ["media"]);
        assert_eq!(
            topic(&id("media.nowPlaying@1")),
            "contract:media.nowPlaying@1"
        );
        assert_eq!(
            from_topic("contract:host.session@1"),
            Some(id("host.session@1"))
        );
        assert_eq!(from_topic("tick"), None);
        assert!(HOST_CONTRACTS
            .iter()
            .all(|contract| is_host_contract(&id(contract))));
        assert!(!is_host_contract(&id("media.nowPlaying@1")));
    }
}
