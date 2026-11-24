use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const MANIFEST_VERSION_PATTERN: &str =
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(\.(0|[1-9][0-9]*))?$";
pub const PACKAGE_VERSION_PATTERN: &str = r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ManifestVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("version {raw:?} must be {expected}")]
pub struct VersionError {
    pub raw: String,
    pub expected: &'static str,
}

fn parse_parts<const N: usize>(
    raw: &str,
    expected: &'static str,
) -> Result<[u32; N], VersionError> {
    let invalid = || VersionError {
        raw: raw.to_string(),
        expected,
    };
    let mut parts = [0; N];
    let mut pieces = raw.split('.');
    for slot in &mut parts {
        let piece = pieces.next().ok_or_else(invalid)?;
        let canonical = !piece.is_empty()
            && piece.bytes().all(|b| b.is_ascii_digit())
            && (piece == "0" || !piece.starts_with('0'));
        if !canonical {
            return Err(invalid());
        }
        *slot = piece.parse().map_err(|_| invalid())?;
    }
    if pieces.next().is_some() {
        return Err(invalid());
    }
    Ok(parts)
}

impl ManifestVersion {
    pub const fn with_patch(self, patch: u32) -> PackageVersion {
        PackageVersion {
            major: self.major,
            minor: self.minor,
            patch,
        }
    }

    pub const fn stamped(self) -> Option<PackageVersion> {
        match self.patch {
            Some(patch) => Some(self.with_patch(patch)),
            None => None,
        }
    }

    #[must_use]
    pub const fn authored(self) -> Self {
        Self {
            major: self.major,
            minor: self.minor,
            patch: None,
        }
    }
}

impl PackageVersion {
    pub const fn authored(self) -> ManifestVersion {
        ManifestVersion {
            major: self.major,
            minor: self.minor,
            patch: None,
        }
    }

    pub const fn stamp(self) -> ManifestVersion {
        ManifestVersion {
            major: self.major,
            minor: self.minor,
            patch: Some(self.patch),
        }
    }
}

impl FromStr for ManifestVersion {
    type Err = VersionError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        if raw.matches('.').count() == 2 {
            return raw
                .parse::<PackageVersion>()
                .map(PackageVersion::stamp)
                .map_err(|_| VersionError {
                    raw: raw.to_string(),
                    expected: "MAJOR.MINOR or MAJOR.MINOR.PATCH",
                });
        }
        let [major, minor] = parse_parts(raw, "MAJOR.MINOR or MAJOR.MINOR.PATCH")?;
        Ok(Self {
            major,
            minor,
            patch: None,
        })
    }
}

impl FromStr for PackageVersion {
    type Err = VersionError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let [major, minor, patch] = parse_parts(raw, "MAJOR.MINOR.PATCH")?;
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl fmt::Display for ManifestVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.patch {
            Some(patch) => write!(formatter, "{}.{}.{patch}", self.major, self.minor),
            None => write!(formatter, "{}.{}", self.major, self.minor),
        }
    }
}

impl fmt::Display for PackageVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

macro_rules! string_serde {
    ($type:ty, $name:literal, $pattern:expr) => {
        impl Serialize for $type {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                raw.parse().map_err(serde::de::Error::custom)
            }
        }

        impl JsonSchema for $type {
            fn schema_name() -> Cow<'static, str> {
                $name.into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({ "type": "string", "pattern": $pattern })
            }
        }
    };
}

string_serde!(ManifestVersion, "ManifestVersion", MANIFEST_VERSION_PATTERN);
string_serde!(PackageVersion, "PackageVersion", PACKAGE_VERSION_PATTERN);

#[cfg(test)]
mod tests {
    use super::{ManifestVersion, PackageVersion};

    #[test]
    fn parses_authored_and_stamped_versions() {
        let authored: ManifestVersion = "1.0".parse().unwrap();
        assert_eq!(authored.to_string(), "1.0");
        let stamped = authored.with_patch(7);
        assert_eq!(stamped.to_string(), "1.0.7");
        assert_eq!(stamped.authored(), authored);
        assert_eq!("12.30.400".parse::<PackageVersion>().unwrap().patch, 400);
    }

    #[test]
    fn manifests_carry_an_optional_stamped_patch() {
        let packed: ManifestVersion = "1.0.7".parse().unwrap();
        assert_eq!(packed.to_string(), "1.0.7");
        assert_eq!(
            packed.stamped(),
            Some("1.0.7".parse::<PackageVersion>().unwrap())
        );
        assert_eq!(packed.authored().to_string(), "1.0");
        assert_eq!("1.0".parse::<ManifestVersion>().unwrap().stamped(), None);
        assert_eq!("1.0.7".parse::<PackageVersion>().unwrap().stamp(), packed);
    }

    #[test]
    fn rejects_malformed_versions() {
        for raw in [
            "", "1", "1.0.0.0", "1.", ".1", "01.0", "1.00", "a.b", "1.-1", "1.0 ", "1.0.01", "1.0.",
        ] {
            assert!(raw.parse::<ManifestVersion>().is_err(), "{raw}");
        }
        for raw in ["1.0", "1.0.0.0", "1.0.01", "1.0.x"] {
            assert!(raw.parse::<PackageVersion>().is_err(), "{raw}");
        }
    }

    #[test]
    fn orders_numerically() {
        let older: PackageVersion = "1.9.10".parse().unwrap();
        let newer: PackageVersion = "1.10.2".parse().unwrap();
        assert!(older < newer);
    }
}
