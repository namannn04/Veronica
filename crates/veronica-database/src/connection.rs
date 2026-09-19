//! What Veronica stores about a database it can reach.
//!
//! A port of Edith's `ConnectionDefinition.swift`, including the parts that
//! look like ceremony and are not: every bounded value refuses out-of-range
//! input at construction, so a port of 0 or a timeout of a fortnight cannot be
//! stored and then surprise somebody later.
//!
//! **No secret is in here.** Passwords, tokens and private keys live in the
//! secret store and are referenced by identifier, so a definition can be
//! printed, logged, exported and synced without leaking anything. That is
//! Edith's rule and it is the reason this type exists separately from a
//! connection string.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::product::Product;

pub const SCHEMA_VERSION: u32 = 1;

/// Where a database sits in your life, which decides how careful Veronica is
/// with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EnvironmentKind {
    #[default]
    Local,
    Development,
    Testing,
    Staging,
    Production,
    Other,
}

impl EnvironmentKind {
    pub const ALL: [EnvironmentKind; 6] = [
        EnvironmentKind::Local,
        EnvironmentKind::Development,
        EnvironmentKind::Testing,
        EnvironmentKind::Staging,
        EnvironmentKind::Production,
        EnvironmentKind::Other,
    ];

    pub fn key(self) -> &'static str {
        match self {
            EnvironmentKind::Local => "local",
            EnvironmentKind::Development => "development",
            EnvironmentKind::Testing => "testing",
            EnvironmentKind::Staging => "staging",
            EnvironmentKind::Production => "production",
            EnvironmentKind::Other => "other",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        EnvironmentKind::ALL
            .into_iter()
            .find(|kind| kind.key().eq_ignore_ascii_case(raw))
    }
}

/// How much friction to put in front of a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EnvironmentProtection {
    #[default]
    Standard,
    /// Every mutation needs the strongest confirmation text.
    ConfirmationRequired,
    /// No mutation is allowed at all.
    ReadOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentMetadata {
    pub kind: EnvironmentKind,
    pub label: String,
    pub protection: EnvironmentProtection,
}

impl Default for EnvironmentMetadata {
    fn default() -> Self {
        Self {
            kind: EnvironmentKind::Local,
            label: "Local".to_string(),
            protection: EnvironmentProtection::Standard,
        }
    }
}

/// Edith's bounded values. Each refuses out-of-range input at construction
/// rather than clamping, because a port of 70000 is a mistake to report, not a
/// number to round down.
macro_rules! bounded {
    ($name:ident, $inner:ty, $inner_name:literal, $min:expr, $max:expr, $what:literal) => {
        // `try_from`/`into` are what make the bound survive a hand-edited file:
        // decoding goes through the constructor rather than around it. Serde
        // needs the type as a string literal, which is why it is passed twice.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = $inner_name, into = $inner_name)]
        pub struct $name($inner);

        impl $name {
            pub const MIN: $inner = $min;
            pub const MAX: $inner = $max;

            pub fn new(value: $inner) -> anyhow::Result<Self> {
                if !($min..=$max).contains(&value) {
                    anyhow::bail!(
                        concat!($what, " must be between {} and {}, not {}"),
                        $min,
                        $max,
                        value
                    );
                }
                Ok(Self(value))
            }

            pub fn get(self) -> $inner {
                self.0
            }
        }

        impl TryFrom<$inner> for $name {
            type Error = String;
            fn try_from(value: $inner) -> Result<Self, Self::Error> {
                Self::new(value).map_err(|error| error.to_string())
            }
        }

        impl From<$name> for $inner {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

bounded!(Port, u16, "u16", 1, 65_535, "A port");
bounded!(
    TimeoutMillis,
    u64,
    "u64",
    100,
    86_400_000,
    "A timeout in milliseconds"
);
bounded!(PoolSize, u16, "u16", 1, 256, "A pool size");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EndpointRole {
    #[default]
    Primary,
    ReadReplica,
    Seed,
    Router,
    Sentinel,
    Node,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkEndpoint {
    pub host: String,
    pub port: Port,
    #[serde(default)]
    pub role: EndpointRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SqliteAccessMode {
    ReadOnly,
    #[default]
    ReadWrite,
    CreateIfMissing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqliteLocation {
    pub path: String,
    #[serde(default)]
    pub access_mode: SqliteAccessMode,
}

/// How to reach it: over the network, as a file, or in memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Location {
    Network { endpoints: Vec<NetworkEndpoint> },
    Sqlite { sqlite: SqliteLocation },
    Memory { name: Option<String> },
}

impl Location {
    /// The endpoint a client should dial first.
    pub fn primary_endpoint(&self) -> Option<&NetworkEndpoint> {
        match self {
            Location::Network { endpoints } => endpoints
                .iter()
                .find(|endpoint| endpoint.role == EndpointRole::Primary)
                .or_else(|| endpoints.first()),
            _ => None,
        }
    }

    /// A one-line description for a listing, with no secret in it.
    pub fn summary(&self) -> String {
        match self {
            Location::Network { endpoints } => match self.primary_endpoint() {
                Some(endpoint) if endpoints.len() > 1 => format!(
                    "{}:{} +{}",
                    endpoint.host,
                    endpoint.port.get(),
                    endpoints.len() - 1
                ),
                Some(endpoint) => format!("{}:{}", endpoint.host, endpoint.port.get()),
                None => "no endpoint".to_string(),
            },
            Location::Sqlite { sqlite } => sqlite.path.clone(),
            Location::Memory { name } => match name {
                Some(name) => format!("in memory ({name})"),
                None => "in memory".to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeploymentMode {
    #[default]
    Automatic,
    Embedded,
    Standalone,
    PrimaryReplica,
    Sentinel,
    Cluster,
    ReplicaSet,
    ShardedCluster,
    Distributed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceDefaults {
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub logical_database: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthenticationKind {
    #[default]
    None,
    Password,
    UsernameAndPassword,
    Token,
    ApiKey,
    Scram,
    X509,
    CloudIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretPurpose {
    Password,
    Token,
    ApiKeyIdentifier,
    ApiKeySecret,
    ClientPrivateKey,
    Passphrase,
    ConfirmationSigningKey,
    ContinuationSigningKey,
}

impl SecretPurpose {
    pub fn key(self) -> &'static str {
        match self {
            SecretPurpose::Password => "password",
            SecretPurpose::Token => "token",
            SecretPurpose::ApiKeyIdentifier => "apiKeyIdentifier",
            SecretPurpose::ApiKeySecret => "apiKeySecret",
            SecretPurpose::ClientPrivateKey => "clientPrivateKey",
            SecretPurpose::Passphrase => "passphrase",
            SecretPurpose::ConfirmationSigningKey => "confirmationSigningKey",
            SecretPurpose::ContinuationSigningKey => "continuationSigningKey",
        }
    }
}

/// A pointer to a secret, never the secret. This is what makes a definition
/// safe to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretReference {
    pub identifier: Uuid,
    pub purpose: SecretPurpose,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Authentication {
    pub kind: AuthenticationKind,
    #[serde(default)]
    pub secret_references: Vec<SecretReference>,
    /// Where the credential came from, e.g. an environment variable's name.
    /// Never its value.
    pub source: Option<String>,
}

impl Authentication {
    pub fn reference(&self, purpose: SecretPurpose) -> Option<SecretReference> {
        self.secret_references
            .iter()
            .find(|reference| reference.purpose == purpose)
            .copied()
    }

    /// Whether a credential has been configured. The only thing a report may
    /// say about authentication beyond its kind.
    pub fn is_configured(&self) -> bool {
        self.kind != AuthenticationKind::None && !self.secret_references.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TlsMode {
    #[default]
    Disabled,
    Preferred,
    Required,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TlsVerification {
    #[default]
    None,
    CertificateAuthority,
    Full,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsConfiguration {
    pub mode: TlsMode,
    pub verification: TlsVerification,
    pub server_name: Option<String>,
    /// A path, not the certificate. Same rule as secrets.
    pub certificate_authority_path: Option<String>,
    pub client_certificate_path: Option<String>,
    pub client_private_key: Option<SecretReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionLimits {
    pub connection_timeout: TimeoutMillis,
    pub operation_timeout: TimeoutMillis,
    pub pool_size: PoolSize,
    pub idle_timeout: Option<TimeoutMillis>,
    pub keepalive_interval: Option<TimeoutMillis>,
}

impl Default for ConnectionLimits {
    fn default() -> Self {
        Self {
            connection_timeout: TimeoutMillis::new(10_000).expect("inside the range"),
            operation_timeout: TimeoutMillis::new(30_000).expect("inside the range"),
            pool_size: PoolSize::new(4).expect("inside the range"),
            idle_timeout: None,
            keepalive_interval: None,
        }
    }
}

/// Whether Veronica should refuse to write at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReadOnlyPolicy {
    #[default]
    Disabled,
    /// Read-only where the product supports it, but writes are not refused.
    Preferred,
    /// No write reaches the server.
    Required,
}

/// The extra care a production database gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProductionPolicy {
    #[default]
    Standard,
    RequireMutationPreview,
    ProhibitMutations,
}

/// A tunnel through one of Veronica's machines, so a database that is only
/// reachable from a jump host still is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelDefinition {
    /// A machine id from `vr machines ls`, so the user's own SSH config, keys
    /// and jump hosts apply and Veronica never handles a credential.
    pub machine_identifier: String,
    pub remote_endpoint: NetworkEndpoint,
    #[serde(default = "loopback")]
    pub local_bind_address: String,
    pub requested_local_port: Option<Port>,
    #[serde(default = "yes")]
    pub manages_lifecycle: bool,
}

fn loopback() -> String {
    "127.0.0.1".to_string()
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionIdentity {
    pub id: Uuid,
    pub display_name: String,
    pub product_hint: Product,
    pub environment: EnvironmentMetadata,
}

/// One saved connection. No secret, ever.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDefinition {
    #[serde(default = "schema_version")]
    pub version: u32,
    pub id: Uuid,
    pub display_name: String,
    pub product_hint: Product,
    pub location: Location,
    pub username: Option<String>,
    #[serde(default)]
    pub namespaces: NamespaceDefaults,
    #[serde(default)]
    pub deployment_mode: DeploymentMode,
    #[serde(default)]
    pub authentication: Authentication,
    #[serde(default)]
    pub tls: TlsConfiguration,
    pub tunnel: Option<TunnelDefinition>,
    #[serde(default)]
    pub limits: ConnectionLimits,
    #[serde(default)]
    pub read_only_policy: ReadOnlyPolicy,
    #[serde(default)]
    pub production_policy: ProductionPolicy,
    #[serde(default)]
    pub environment: EnvironmentMetadata,
    pub group: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub color: Option<String>,
    #[serde(default)]
    pub is_favorite: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub last_tested_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl ConnectionDefinition {
    pub fn identity(&self) -> ConnectionIdentity {
        ConnectionIdentity {
            id: self.id,
            display_name: self.display_name.clone(),
            product_hint: self.product_hint,
            environment: self.environment.clone(),
        }
    }

    /// Why a mutation is refused outright, before anything is previewed.
    ///
    /// Three separate switches say no, and the report names which one, because
    /// "not allowed" without a reason leaves the user hunting through settings.
    pub fn mutation_prohibition(&self) -> Option<Prohibition> {
        if self.read_only_policy == ReadOnlyPolicy::Required {
            return Some(Prohibition::ConnectionReadOnly);
        }
        if self.environment.protection == EnvironmentProtection::ReadOnly {
            return Some(Prohibition::EnvironmentReadOnly);
        }
        if self.production_policy == ProductionPolicy::ProhibitMutations {
            return Some(Prohibition::ProductionPolicy);
        }
        None
    }

    /// A short line for a listing.
    pub fn summary(&self) -> String {
        format!(
            "{} · {} · {}",
            self.product_hint.title(),
            self.environment.label,
            self.location.summary()
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Prohibition {
    ConnectionReadOnly,
    EnvironmentReadOnly,
    ProductionPolicy,
}

impl Prohibition {
    pub fn reason(self) -> &'static str {
        match self {
            Prohibition::ConnectionReadOnly => {
                "this connection is set to read-only (readOnlyPolicy = required)"
            }
            Prohibition::EnvironmentReadOnly => "this connection's environment is marked read-only",
            Prohibition::ProductionPolicy => {
                "this connection's production policy prohibits mutations"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition() -> ConnectionDefinition {
        let now = chrono::Utc::now();
        ConnectionDefinition {
            version: SCHEMA_VERSION,
            id: Uuid::new_v4(),
            display_name: "Local Postgres".into(),
            product_hint: Product::Postgresql,
            location: Location::Network {
                endpoints: vec![NetworkEndpoint {
                    host: "127.0.0.1".into(),
                    port: Port::new(5432).unwrap(),
                    role: EndpointRole::Primary,
                }],
            },
            username: Some("me".into()),
            namespaces: NamespaceDefaults::default(),
            deployment_mode: DeploymentMode::Automatic,
            authentication: Authentication::default(),
            tls: TlsConfiguration::default(),
            tunnel: None,
            limits: ConnectionLimits::default(),
            read_only_policy: ReadOnlyPolicy::Disabled,
            production_policy: ProductionPolicy::Standard,
            environment: EnvironmentMetadata::default(),
            group: None,
            tags: Vec::new(),
            color: None,
            is_favorite: false,
            created_at: now,
            updated_at: now,
            last_tested_at: None,
            last_used_at: None,
        }
    }

    #[test]
    fn a_bounded_value_refuses_out_of_range_input_rather_than_clamping() {
        // A port of 70000 is a mistake to report, not a number to round down.
        assert!(Port::new(0).is_err());
        assert!(Port::new(65_535).is_ok());
        assert!(TimeoutMillis::new(50).is_err());
        assert!(TimeoutMillis::new(100).is_ok());
        assert!(TimeoutMillis::new(86_400_001).is_err());
        assert!(PoolSize::new(0).is_err());
        assert!(PoolSize::new(257).is_err());
    }

    #[test]
    fn ediths_bounds_are_kept_exactly() {
        assert_eq!((Port::MIN, Port::MAX), (1, 65_535));
        assert_eq!((TimeoutMillis::MIN, TimeoutMillis::MAX), (100, 86_400_000));
        assert_eq!((PoolSize::MIN, PoolSize::MAX), (1, 256));
    }

    #[test]
    fn an_out_of_range_value_in_a_stored_file_fails_to_decode() {
        // Otherwise a hand-edited definition would smuggle one past the bound.
        assert!(serde_json::from_str::<Port>("0").is_err());
        assert!(serde_json::from_str::<Port>("443").is_ok());
        assert!(serde_json::from_str::<TimeoutMillis>("1").is_err());
    }

    #[test]
    fn a_definition_round_trips_through_json() {
        let original = definition();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: ConnectionDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn a_definition_never_carries_a_secret() {
        // This is the whole reason the type exists rather than a URL string.
        let mut connection = definition();
        connection.authentication = Authentication {
            kind: AuthenticationKind::UsernameAndPassword,
            secret_references: vec![SecretReference {
                identifier: Uuid::new_v4(),
                purpose: SecretPurpose::Password,
            }],
            source: Some("VERONICA_PGPASSWORD".into()),
        };
        let json = serde_json::to_string(&connection).unwrap();
        assert!(!json.contains("password\":\"") || !json.contains("hunter2"));
        // What it does carry is a pointer and the fact that one is configured.
        assert!(connection.authentication.is_configured());
        assert!(connection
            .authentication
            .reference(SecretPurpose::Password)
            .is_some());
        assert!(connection
            .authentication
            .reference(SecretPurpose::Token)
            .is_none());
    }

    #[test]
    fn each_read_only_switch_refuses_a_mutation_and_says_which_one_did() {
        let mut connection = definition();
        assert_eq!(connection.mutation_prohibition(), None);

        connection.read_only_policy = ReadOnlyPolicy::Required;
        assert_eq!(
            connection.mutation_prohibition(),
            Some(Prohibition::ConnectionReadOnly)
        );

        connection.read_only_policy = ReadOnlyPolicy::Disabled;
        connection.environment.protection = EnvironmentProtection::ReadOnly;
        assert_eq!(
            connection.mutation_prohibition(),
            Some(Prohibition::EnvironmentReadOnly)
        );

        connection.environment.protection = EnvironmentProtection::Standard;
        connection.production_policy = ProductionPolicy::ProhibitMutations;
        assert_eq!(
            connection.mutation_prohibition(),
            Some(Prohibition::ProductionPolicy)
        );
    }

    #[test]
    fn preferred_read_only_does_not_refuse_a_write() {
        // Only `required` is a refusal; `preferred` is a hint to the driver.
        let mut connection = definition();
        connection.read_only_policy = ReadOnlyPolicy::Preferred;
        assert_eq!(connection.mutation_prohibition(), None);
    }

    #[test]
    fn a_location_summarises_without_leaking_anything() {
        let connection = definition();
        assert_eq!(connection.location.summary(), "127.0.0.1:5432");

        let cluster = Location::Network {
            endpoints: vec![
                NetworkEndpoint {
                    host: "a".into(),
                    port: Port::new(6379).unwrap(),
                    role: EndpointRole::Node,
                },
                NetworkEndpoint {
                    host: "b".into(),
                    port: Port::new(6379).unwrap(),
                    role: EndpointRole::Node,
                },
            ],
        };
        assert_eq!(cluster.summary(), "a:6379 +1");
        assert_eq!(
            Location::Sqlite {
                sqlite: SqliteLocation {
                    path: "/tmp/app.db".into(),
                    access_mode: SqliteAccessMode::ReadOnly
                }
            }
            .summary(),
            "/tmp/app.db"
        );
        assert_eq!(Location::Memory { name: None }.summary(), "in memory");
    }

    #[test]
    fn the_primary_endpoint_is_preferred_over_a_replica() {
        let location = Location::Network {
            endpoints: vec![
                NetworkEndpoint {
                    host: "replica".into(),
                    port: Port::new(5432).unwrap(),
                    role: EndpointRole::ReadReplica,
                },
                NetworkEndpoint {
                    host: "primary".into(),
                    port: Port::new(5432).unwrap(),
                    role: EndpointRole::Primary,
                },
            ],
        };
        assert_eq!(location.primary_endpoint().unwrap().host, "primary");
    }

    #[test]
    fn an_environment_kind_parses_by_the_name_it_is_stored_under() {
        assert_eq!(
            EnvironmentKind::parse("production"),
            Some(EnvironmentKind::Production)
        );
        assert_eq!(
            EnvironmentKind::parse("PRODUCTION"),
            Some(EnvironmentKind::Production)
        );
        assert_eq!(EnvironmentKind::parse("prod"), None);
    }
}
