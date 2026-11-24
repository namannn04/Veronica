use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const CONTRACT_ID_PATTERN: &str = r"^[a-z][A-Za-z0-9]*(\.[a-z][A-Za-z0-9]*)*@[1-9][0-9]*$";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractId {
    name: String,
    version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("contract id {0:?} must look like name@N, for example media.nowPlaying@1")]
pub struct ContractIdError(pub String);

impl ContractId {
    pub fn new(name: &str, version: u32) -> Result<Self, ContractIdError> {
        if version == 0 || !is_contract_name(name) {
            return Err(ContractIdError(format!("{name}@{version}")));
        }
        Ok(Self {
            name: name.to_string(),
            version,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn namespace(&self) -> &str {
        self.name.split('.').next().unwrap_or(&self.name)
    }
}

fn is_contract_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|segment| {
            let mut chars = segment.chars();
            chars.next().is_some_and(|first| first.is_ascii_lowercase())
                && chars.all(|c| c.is_ascii_alphanumeric())
        })
}

fn is_version_number(digits: &str) -> bool {
    !digits.is_empty() && !digits.starts_with('0') && digits.bytes().all(|b| b.is_ascii_digit())
}

impl FromStr for ContractId {
    type Err = ContractIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let invalid = || ContractIdError(value.to_string());
        let (name, digits) = value.rsplit_once('@').ok_or_else(invalid)?;
        if !is_version_number(digits) {
            return Err(invalid());
        }
        let version = digits.parse().map_err(|_| invalid())?;
        Self::new(name, version).map_err(|_| invalid())
    }
}

impl fmt::Display for ContractId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.name, self.version)
    }
}

impl Serialize for ContractId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ContractId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for ContractId {
    fn schema_name() -> Cow<'static, str> {
        "ContractId".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "pattern": CONTRACT_ID_PATTERN })
    }
}

#[cfg(test)]
mod tests {
    use super::ContractId;

    #[test]
    fn parses_and_displays() {
        let id: ContractId = "media.nowPlaying@1".parse().unwrap();
        assert_eq!(id.name(), "media.nowPlaying");
        assert_eq!(id.version(), 1);
        assert_eq!(id.namespace(), "media");
        assert_eq!(id.to_string(), "media.nowPlaying@1");
        assert_eq!("host@12".parse::<ContractId>().unwrap().version(), 12);
    }

    #[test]
    fn rejects_malformed_ids() {
        for raw in [
            "",
            "media",
            "media@",
            "media@0",
            "media@01",
            "media@1x",
            "@1",
            "Media.now@1",
            "media..now@1",
            "media.now-playing@1",
            "media.@1",
            "media@1@2",
            "media@99999999999",
        ] {
            assert!(
                raw.parse::<ContractId>().is_err(),
                "{raw} should be rejected"
            );
        }
    }

    #[test]
    fn serde_uses_the_string_form() {
        let id: ContractId = serde_json::from_str("\"host.presenter@1\"").unwrap();
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"host.presenter@1\"");
        assert!(serde_json::from_str::<ContractId>("\"host.presenter\"").is_err());
    }
}
