use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Family {
    Relational,
    KeyValue,
    Document,
    Search,
    Analytical,
}

impl Family {
    pub fn title(self) -> &'static str {
        match self {
            Self::Relational => "Relational",
            Self::KeyValue => "Key-value",
            Self::Document => "Document",
            Self::Search => "Search",
            Self::Analytical => "Analytical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Product {
    Postgresql,
    Mysql,
    #[serde(rename = "mariadb")]
    MariaDb,
    Sqlite,
    Redis,
    Valkey,
    #[serde(rename = "mongodb")]
    MongoDb,
    Elasticsearch,
    #[serde(rename = "opensearch")]
    OpenSearch,
    #[serde(rename = "clickhouse")]
    ClickHouse,
}

impl Product {
    pub const ALL: [Self; 10] = [
        Self::Postgresql,
        Self::Mysql,
        Self::MariaDb,
        Self::Sqlite,
        Self::Redis,
        Self::Valkey,
        Self::MongoDb,
        Self::Elasticsearch,
        Self::OpenSearch,
        Self::ClickHouse,
    ];

    pub fn family(self) -> Family {
        match self {
            Self::Postgresql | Self::Mysql | Self::MariaDb | Self::Sqlite => Family::Relational,
            Self::Redis | Self::Valkey => Family::KeyValue,
            Self::MongoDb => Family::Document,
            Self::Elasticsearch | Self::OpenSearch => Family::Search,
            Self::ClickHouse => Family::Analytical,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Postgresql => "PostgreSQL",
            Self::Mysql => "MySQL",
            Self::MariaDb => "MariaDB",
            Self::Sqlite => "SQLite",
            Self::Redis => "Redis",
            Self::Valkey => "Valkey",
            Self::MongoDb => "MongoDB",
            Self::Elasticsearch => "Elasticsearch",
            Self::OpenSearch => "OpenSearch",
            Self::ClickHouse => "ClickHouse",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Postgresql => "postgresql",
            Self::Mysql => "mysql",
            Self::MariaDb => "mariadb",
            Self::Sqlite => "sqlite",
            Self::Redis => "redis",
            Self::Valkey => "valkey",
            Self::MongoDb => "mongodb",
            Self::Elasticsearch => "elasticsearch",
            Self::OpenSearch => "opensearch",
            Self::ClickHouse => "clickhouse",
        }
    }

    pub fn default_port(self) -> Option<u16> {
        match self {
            Self::Postgresql => Some(5432),
            Self::Mysql | Self::MariaDb => Some(3306),
            Self::Redis | Self::Valkey => Some(6379),
            Self::MongoDb => Some(27017),
            Self::Elasticsearch | Self::OpenSearch => Some(9200),
            Self::ClickHouse => Some(8123),
            Self::Sqlite => None,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let lowered = raw.to_lowercase();
        Self::ALL
            .into_iter()
            .find(|product| product.key() == lowered)
            .or(match lowered.as_str() {
                "postgres" | "pg" => Some(Self::Postgresql),
                "mongo" => Some(Self::MongoDb),
                _ => None,
            })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub string: String,
    pub major: Option<u32>,
    pub minor: Option<u32>,
    pub patch: Option<u32>,
}

impl Version {
    pub fn parse(reported: &str) -> Self {
        let mut numbers: Vec<u32> = Vec::new();
        let mut current = String::new();
        let mut started = false;
        for character in reported.chars() {
            if character.is_ascii_digit() {
                current.push(character);
                started = true;
            } else if character == '.' && started && !current.is_empty() {
                numbers.push(current.parse().unwrap_or(0));
                current.clear();
            } else if started {
                break;
            }
        }
        if !current.is_empty() {
            numbers.push(current.parse().unwrap_or(0));
        }
        Self {
            string: reported.trim().to_string(),
            major: numbers.first().copied(),
            minor: numbers.get(1).copied(),
            patch: numbers.get(2).copied(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TopologyKind {
    #[default]
    Unknown,
    Embedded,
    Standalone,
    PrimaryReplica,
    Sentinel,
    Cluster,
    ReplicaSet,
    ShardedCluster,
    Distributed,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StringAttribute {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Topology {
    pub kind: TopologyKind,
    pub name: Option<String>,
    pub local_role: Option<String>,
    pub node_count: Option<u32>,
    pub replica_count: Option<u32>,
    pub shard_count: Option<u32>,
    #[serde(default)]
    pub attributes: Vec<StringAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionIdentity {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductIdentity {
    pub product: Product,
    pub version: Option<Version>,
    pub distribution: Option<String>,
    pub topology: Topology,
    pub server_identifier: Option<String>,
    #[serde(default)]
    pub modules: Vec<ExtensionIdentity>,
    #[serde(default)]
    pub plugins: Vec<ExtensionIdentity>,
    #[serde(default)]
    pub compatibility_notes: Vec<String>,
}

impl ProductIdentity {
    pub fn new(product: Product) -> Self {
        Self {
            product,
            version: None,
            distribution: None,
            topology: Topology::default(),
            server_identifier: None,
            modules: Vec::new(),
            plugins: Vec::new(),
            compatibility_notes: Vec::new(),
        }
    }

    pub fn family(&self) -> Family {
        self.product.family()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_product_maps_to_the_family_edith_gives_it() {
        assert_eq!(Product::Postgresql.family(), Family::Relational);
        assert_eq!(Product::Sqlite.family(), Family::Relational);
        assert_eq!(Product::Redis.family(), Family::KeyValue);
        assert_eq!(Product::Valkey.family(), Family::KeyValue);
        assert_eq!(Product::MongoDb.family(), Family::Document);
        assert_eq!(Product::OpenSearch.family(), Family::Search);
        assert_eq!(Product::ClickHouse.family(), Family::Analytical);
    }

    #[test]
    fn the_wire_names_match_ediths_so_a_definition_moves_between_platforms() {
        let pairs = [
            (Product::Postgresql, "postgresql"),
            (Product::MariaDb, "mariadb"),
            (Product::MongoDb, "mongodb"),
            (Product::OpenSearch, "opensearch"),
            (Product::ClickHouse, "clickhouse"),
        ];
        for (product, key) in pairs {
            assert_eq!(product.key(), key);
            assert_eq!(
                serde_json::to_string(&product).unwrap(),
                format!("\"{key}\"")
            );
            assert_eq!(
                serde_json::from_str::<Product>(&format!("\"{key}\"")).unwrap(),
                product
            );
        }
    }

    #[test]
    fn the_names_people_actually_type_resolve() {
        assert_eq!(Product::parse("postgres"), Some(Product::Postgresql));
        assert_eq!(Product::parse("PG"), Some(Product::Postgresql));
        assert_eq!(Product::parse("Mongo"), Some(Product::MongoDb));
        assert_eq!(Product::parse("MySQL"), Some(Product::Mysql));
        assert_eq!(Product::parse("oracle"), None);
    }

    #[test]
    fn sqlite_has_no_port_because_it_is_a_file() {
        assert_eq!(Product::Sqlite.default_port(), None);
        assert_eq!(Product::Postgresql.default_port(), Some(5432));
        assert_eq!(Product::MariaDb.default_port(), Some(3306));
    }

    #[test]
    fn a_version_is_read_out_of_whatever_the_server_reported() {
        let cases = [
            ("8.0.35-0ubuntu0.1", (8, 0, 35)),
            ("16.2", (16, 2, 0)),
            ("7.2.4", (7, 2, 4)),
        ];
        for (reported, (major, minor, patch)) in cases {
            let version = Version::parse(reported);
            assert_eq!(version.major, Some(major), "{reported}");
            assert_eq!(version.minor, Some(minor), "{reported}");
            if patch > 0 {
                assert_eq!(version.patch, Some(patch), "{reported}");
            }
            assert_eq!(version.string, reported);
        }
    }

    #[test]
    fn a_version_with_no_numbers_keeps_the_string_and_claims_nothing() {
        let version = Version::parse("unknown");
        assert_eq!(version.string, "unknown");
        assert_eq!(version.major, None);
    }
}
