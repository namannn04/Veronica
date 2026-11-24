use std::borrow::Cow;

use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ExitCode {
    Success = 0,
    Failed = 1,
    Usage = 2,
    NotFound = 3,
    Unavailable = 4,
    ConfirmationRequired = 5,
    PermissionDenied = 6,
    Unreachable = 7,
    VerificationFailed = 8,
    BackendFailed = 9,
    Timeout = 10,
    Interrupted = 130,
}

impl ExitCode {
    pub const ALL: [Self; 12] = [
        Self::Success,
        Self::Failed,
        Self::Usage,
        Self::NotFound,
        Self::Unavailable,
        Self::ConfirmationRequired,
        Self::PermissionDenied,
        Self::Unreachable,
        Self::VerificationFailed,
        Self::BackendFailed,
        Self::Timeout,
        Self::Interrupted,
    ];

    pub const fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(code: i32) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|exit| i32::from(exit.code()) == code)
    }

    pub const fn meaning(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "operation failed",
            Self::Usage => "invalid usage",
            Self::NotFound => "not found",
            Self::Unavailable => "unavailable: not installed, disabled, missing tool or capability",
            Self::ConfirmationRequired => "confirmation required",
            Self::PermissionDenied => "permission denied",
            Self::Unreachable => "network or catalog unreachable",
            Self::VerificationFailed => "verification failed",
            Self::BackendFailed => "backend crashed or protocol error",
            Self::Timeout => "timeout",
            Self::Interrupted => "interrupted",
        }
    }

    pub const fn is_success(self) -> bool {
        matches!(self, Self::Success)
    }
}

impl From<ExitCode> for i32 {
    fn from(exit: ExitCode) -> Self {
        Self::from(exit.code())
    }
}

impl Serialize for ExitCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.code())
    }
}

impl<'de> Deserialize<'de> for ExitCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = i32::deserialize(deserializer)?;
        Self::from_code(code)
            .ok_or_else(|| serde::de::Error::custom(format!("{code} is not a known exit code")))
    }
}

impl JsonSchema for ExitCode {
    fn schema_name() -> Cow<'static, str> {
        "ExitCode".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let codes: Vec<u8> = Self::ALL.iter().map(|exit| exit.code()).collect();
        json_schema!({ "type": "integer", "enum": codes })
    }
}

#[cfg(test)]
mod tests {
    use super::ExitCode;

    #[test]
    fn numeric_values_are_stable() {
        let table: Vec<(u8, ExitCode)> = vec![
            (0, ExitCode::Success),
            (1, ExitCode::Failed),
            (2, ExitCode::Usage),
            (3, ExitCode::NotFound),
            (4, ExitCode::Unavailable),
            (5, ExitCode::ConfirmationRequired),
            (6, ExitCode::PermissionDenied),
            (7, ExitCode::Unreachable),
            (8, ExitCode::VerificationFailed),
            (9, ExitCode::BackendFailed),
            (10, ExitCode::Timeout),
            (130, ExitCode::Interrupted),
        ];
        for (code, exit) in table {
            assert_eq!(exit.code(), code);
            assert_eq!(ExitCode::from_code(i32::from(code)), Some(exit));
        }
        assert_eq!(ExitCode::from_code(11), None);
        assert_eq!(ExitCode::from_code(-1), None);
    }

    #[test]
    fn serializes_as_a_number() {
        assert_eq!(
            serde_json::to_string(&ExitCode::Interrupted).unwrap(),
            "130"
        );
        assert_eq!(
            serde_json::from_str::<ExitCode>("5").unwrap(),
            ExitCode::ConfirmationRequired
        );
        assert!(serde_json::from_str::<ExitCode>("42").is_err());
    }
}
