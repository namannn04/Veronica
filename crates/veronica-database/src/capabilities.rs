use serde::{Deserialize, Serialize};

use crate::product::{Family, Product, ProductIdentity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    BrowseObjects,
    ReadRecords,
    RunQuery,
    MutateRecords,
    Transactions,
    CountBeforeMutation,
    DescribeSchema,
    ExplainQuery,
    ListSessions,
    SessionReadOnly,
}

impl Capability {
    pub const ALL: [Self; 10] = [
        Self::BrowseObjects,
        Self::ReadRecords,
        Self::RunQuery,
        Self::MutateRecords,
        Self::Transactions,
        Self::CountBeforeMutation,
        Self::DescribeSchema,
        Self::ExplainQuery,
        Self::ListSessions,
        Self::SessionReadOnly,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::BrowseObjects => "Browse objects",
            Self::ReadRecords => "Read records",
            Self::RunQuery => "Run a query",
            Self::MutateRecords => "Change records",
            Self::Transactions => "Transactions",
            Self::CountBeforeMutation => "Count before changing",
            Self::DescribeSchema => "Describe schema",
            Self::ExplainQuery => "Explain a query",
            Self::ListSessions => "List sessions",
            Self::SessionReadOnly => "Read-only sessions",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum State {
    Available,
    Unsupported { reason: String },
    Unavailable { reason: String },
}

impl State {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    fn unsupported(reason: &str) -> Self {
        Self::Unsupported {
            reason: reason.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub product: Product,
    pub family: Family,
    pub states: std::collections::BTreeMap<Capability, State>,
    pub object_hierarchy: Vec<String>,
}

impl Report {
    pub fn state(&self, capability: Capability) -> &State {
        static UNKNOWN: std::sync::LazyLock<State> =
            std::sync::LazyLock::new(|| State::Unsupported {
                reason: "Veronica has no answer for this capability.".to_string(),
            });
        self.states.get(&capability).unwrap_or(&UNKNOWN)
    }

    pub fn supports(&self, capability: Capability) -> bool {
        self.state(capability).is_available()
    }

    pub fn for_product(product: Product) -> Self {
        Self::resolve(product, None)
    }

    pub fn for_identity(identity: &ProductIdentity) -> Self {
        Self::resolve(identity.product, Some(identity))
    }

    fn resolve(product: Product, identity: Option<&ProductIdentity>) -> Self {
        use Capability::{
            BrowseObjects, CountBeforeMutation, DescribeSchema, ExplainQuery, ListSessions,
            MutateRecords, ReadRecords, RunQuery, SessionReadOnly, Transactions,
        };
        let family = product.family();
        let mut states = std::collections::BTreeMap::new();
        let mut set = |capability: Capability, state: State| {
            states.insert(capability, state);
        };

        set(BrowseObjects, State::Available);
        set(ReadRecords, State::Available);
        set(RunQuery, State::Available);
        set(MutateRecords, State::Available);

        set(
            Transactions,
            match product {
                Product::Postgresql | Product::Mysql | Product::MariaDb | Product::Sqlite => {
                    State::Available
                }
                Product::MongoDb => match identity.and_then(|identity| {
                    identity.version.as_ref().and_then(|version| version.major)
                }) {
                    Some(major) if major >= 4 => {
                        match identity.map(|identity| identity.topology.kind) {
                            Some(
                                crate::product::TopologyKind::ReplicaSet
                                | crate::product::TopologyKind::ShardedCluster,
                            )
                            | None => State::Available,
                            Some(_) => State::Unavailable {
                                reason: "MongoDB needs a replica set for transactions; this \
                                         server is standalone."
                                    .into(),
                            },
                        }
                    }
                    Some(major) => State::Unavailable {
                        reason: format!(
                            "MongoDB gained multi-document transactions in 4.0; this server \
                             reports {major}."
                        ),
                    },
                    None => State::Available,
                },
                Product::Redis | Product::Valkey => State::unsupported(
                    "Redis has MULTI/EXEC, but it cannot roll back a command that has \
                     already run, so Veronica does not call it a transaction.",
                ),
                Product::Elasticsearch | Product::OpenSearch => {
                    State::unsupported("A search index has no transactions.")
                }
                Product::ClickHouse => State::unsupported(
                    "ClickHouse has no general transactions; a mutation is applied in the \
                     background and cannot be rolled back.",
                ),
            },
        );

        set(
            CountBeforeMutation,
            match family {
                Family::Relational | Family::Document | Family::Search | Family::Analytical => {
                    State::Available
                }
                Family::KeyValue => State::unsupported(
                    "Counting the keys a pattern matches means scanning the keyspace, which \
                     would block the server.",
                ),
            },
        );

        set(
            DescribeSchema,
            match family {
                Family::Relational | Family::Analytical | Family::Search => State::Available,
                Family::Document => State::Unavailable {
                    reason: "A document collection has no declared schema; Veronica infers \
                             the fields from a sample."
                        .into(),
                },
                Family::KeyValue => {
                    State::unsupported("A key-value store has no schema to describe.")
                }
            },
        );

        set(
            ExplainQuery,
            match product {
                Product::Postgresql
                | Product::Mysql
                | Product::MariaDb
                | Product::Sqlite
                | Product::ClickHouse
                | Product::MongoDb
                | Product::Elasticsearch
                | Product::OpenSearch => State::Available,
                Product::Redis | Product::Valkey => {
                    State::unsupported("A Redis command has no query plan.")
                }
            },
        );

        set(
            ListSessions,
            match product {
                Product::Postgresql
                | Product::Mysql
                | Product::MariaDb
                | Product::ClickHouse
                | Product::Redis
                | Product::Valkey
                | Product::MongoDb
                | Product::Elasticsearch
                | Product::OpenSearch => State::Available,
                Product::Sqlite => {
                    State::unsupported("SQLite is a file; there are no sessions to list.")
                }
            },
        );

        set(
            SessionReadOnly,
            match product {
                Product::Postgresql | Product::Mysql | Product::MariaDb | Product::Sqlite => {
                    State::Available
                }
                _ => State::Unavailable {
                    reason: "This product has no session-level read-only mode; Veronica \
                             enforces read-only itself by refusing to send a mutation."
                        .into(),
                },
            },
        );

        Self {
            product,
            family,
            states,
            object_hierarchy: hierarchy(product),
        }
    }
}

pub fn hierarchy(product: Product) -> Vec<String> {
    match product {
        Product::Postgresql => vec!["database".into(), "schema".into(), "table".into()],
        Product::Mysql | Product::MariaDb | Product::ClickHouse => {
            vec!["database".into(), "table".into()]
        }
        Product::Sqlite => vec!["table".into()],
        Product::Redis | Product::Valkey => vec!["database".into(), "key".into()],
        Product::MongoDb => vec!["database".into(), "collection".into()],
        Product::Elasticsearch | Product::OpenSearch => vec!["index".into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product::{Topology, TopologyKind, Version};

    #[test]
    fn every_product_answers_for_every_capability() {
        for product in Product::ALL {
            let report = Report::for_product(product);
            for capability in Capability::ALL {
                assert!(
                    report.states.contains_key(&capability),
                    "{product:?} has no answer for {capability:?}"
                );
            }
        }
    }

    #[test]
    fn a_capability_a_product_cannot_do_says_why() {
        for product in Product::ALL {
            let report = Report::for_product(product);
            for capability in Capability::ALL {
                match report.state(capability) {
                    State::Available => {}
                    State::Unsupported { reason } | State::Unavailable { reason } => {
                        assert!(
                            !reason.is_empty(),
                            "{product:?}/{capability:?} gives no reason"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn redis_has_no_schema_and_no_transaction_veronica_will_claim() {
        let report = Report::for_product(Product::Redis);
        assert!(!report.supports(Capability::DescribeSchema));
        assert!(!report.supports(Capability::Transactions));
        assert!(report.supports(Capability::BrowseObjects));
        assert!(report.supports(Capability::MutateRecords));
    }

    #[test]
    fn sqlite_has_no_sessions_because_it_is_a_file() {
        let report = Report::for_product(Product::Sqlite);
        assert!(!report.supports(Capability::ListSessions));
        assert!(report.supports(Capability::Transactions));
    }

    #[test]
    fn mongodb_transactions_depend_on_the_version_and_the_topology() {
        let standalone = ProductIdentity {
            version: Some(Version::parse("7.0.5")),
            topology: Topology {
                kind: TopologyKind::Standalone,
                ..Topology::default()
            },
            ..ProductIdentity::new(Product::MongoDb)
        };
        assert!(!Report::for_identity(&standalone).supports(Capability::Transactions));

        let replica_set = ProductIdentity {
            topology: Topology {
                kind: TopologyKind::ReplicaSet,
                ..Topology::default()
            },
            ..standalone
        };
        assert!(Report::for_identity(&replica_set).supports(Capability::Transactions));

        let ancient = ProductIdentity {
            version: Some(Version::parse("3.6.0")),
            ..replica_set
        };
        let report = Report::for_identity(&ancient);
        assert!(!report.supports(Capability::Transactions));
        let State::Unavailable { reason } = report.state(Capability::Transactions) else {
            panic!("expected unavailable");
        };
        assert!(reason.contains("4.0"), "got {reason}");
    }

    #[test]
    fn an_unreached_server_is_given_the_benefit_of_the_doubt() {
        assert!(Report::for_product(Product::MongoDb).supports(Capability::Transactions));
    }

    #[test]
    fn the_hierarchy_matches_what_the_product_actually_nests() {
        assert_eq!(
            hierarchy(Product::Postgresql),
            ["database", "schema", "table"]
        );
        assert_eq!(hierarchy(Product::Mysql), ["database", "table"]);
        assert_eq!(hierarchy(Product::Sqlite), ["table"]);
        assert_eq!(hierarchy(Product::MongoDb), ["database", "collection"]);
        assert_eq!(hierarchy(Product::Elasticsearch), ["index"]);
    }

    #[test]
    fn an_unknown_capability_still_returns_a_state() {
        let mut report = Report::for_product(Product::Sqlite);
        report.states.clear();
        assert!(!report.supports(Capability::ReadRecords));
    }
}
