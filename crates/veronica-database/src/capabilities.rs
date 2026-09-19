//! What a given database can actually be asked to do.
//!
//! A port of Edith's capability report. The point is that the interface never
//! offers an operation a product cannot perform: Redis has no schemas, SQLite
//! has no roles, and a search index is not something you run a transaction
//! against. Rather than scattering `if product == …` through every screen, one
//! report answers it, derived from the product and — where it matters — the
//! version the server actually reported.

use serde::{Deserialize, Serialize};

use crate::product::{Family, Product, ProductIdentity};

/// One thing a client might want to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    /// List catalogs, schemas, tables — whatever the product's hierarchy is.
    BrowseObjects,
    /// Read a bounded page of records.
    ReadRecords,
    /// Run a statement the user wrote.
    RunQuery,
    /// Insert, update or delete a record.
    MutateRecords,
    /// Wrap a mutation in a transaction that can be rolled back.
    Transactions,
    /// Count matching records before mutating them.
    CountBeforeMutation,
    /// Describe an object's columns or fields.
    DescribeSchema,
    /// Read the server's query plan.
    ExplainQuery,
    /// List the sessions connected to the server.
    ListSessions,
    /// A server-side notion of read-only for a session.
    SessionReadOnly,
}

impl Capability {
    pub const ALL: [Capability; 10] = [
        Capability::BrowseObjects,
        Capability::ReadRecords,
        Capability::RunQuery,
        Capability::MutateRecords,
        Capability::Transactions,
        Capability::CountBeforeMutation,
        Capability::DescribeSchema,
        Capability::ExplainQuery,
        Capability::ListSessions,
        Capability::SessionReadOnly,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Capability::BrowseObjects => "Browse objects",
            Capability::ReadRecords => "Read records",
            Capability::RunQuery => "Run a query",
            Capability::MutateRecords => "Change records",
            Capability::Transactions => "Transactions",
            Capability::CountBeforeMutation => "Count before changing",
            Capability::DescribeSchema => "Describe schema",
            Capability::ExplainQuery => "Explain a query",
            Capability::ListSessions => "List sessions",
            Capability::SessionReadOnly => "Read-only sessions",
        }
    }
}

/// Whether a capability is there, and if not, why not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum State {
    Available,
    /// The product cannot do this at all.
    Unsupported {
        reason: String,
    },
    /// The product could, but this server or this version cannot.
    Unavailable {
        reason: String,
    },
}

impl State {
    pub fn is_available(&self) -> bool {
        matches!(self, State::Available)
    }

    fn unsupported(reason: &str) -> Self {
        State::Unsupported {
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
    /// The hierarchy this product's objects live in, outermost first. What the
    /// browser's breadcrumb is built from.
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

    /// Derive the report from a product, before a server has been reached.
    pub fn for_product(product: Product) -> Self {
        Self::resolve(product, None)
    }

    /// Derive it from what the server turned out to be, which can narrow it —
    /// an old server may not have a feature its product generally has.
    pub fn for_identity(identity: &ProductIdentity) -> Self {
        Self::resolve(identity.product, Some(identity))
    }

    fn resolve(product: Product, identity: Option<&ProductIdentity>) -> Self {
        use Capability::*;
        let family = product.family();
        let mut states = std::collections::BTreeMap::new();
        let mut set = |capability: Capability, state: State| {
            states.insert(capability, state);
        };

        // Every product can be browsed and read; that is what makes it a
        // database rather than a socket.
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
                    // MongoDB gained multi-document transactions in 4.0, and
                    // only on a replica set — a standalone server refuses them
                    // however new it is.
                    Some(major) if major >= 4 => {
                        match identity.map(|identity| identity.topology.kind) {
                            Some(crate::product::TopologyKind::ReplicaSet)
                            | Some(crate::product::TopologyKind::ShardedCluster) => {
                                State::Available
                            }
                            Some(_) => State::Unavailable {
                                reason: "MongoDB needs a replica set for transactions; this \
                                         server is standalone."
                                    .into(),
                            },
                            None => State::Available,
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
                Family::Relational | Family::Document | Family::Search => State::Available,
                Family::KeyValue => State::unsupported(
                    "Counting the keys a pattern matches means scanning the keyspace, which \
                     would block the server.",
                ),
                Family::Analytical => State::Available,
            },
        );

        set(
            DescribeSchema,
            match family {
                Family::Relational | Family::Analytical => State::Available,
                Family::Document => State::Unavailable {
                    reason: "A document collection has no declared schema; Veronica infers \
                             the fields from a sample."
                        .into(),
                },
                Family::Search => State::Available,
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
                | Product::MongoDb => State::Available,
                Product::Elasticsearch | Product::OpenSearch => State::Available,
                Product::Redis | Product::Valkey => {
                    State::unsupported("A Redis command has no query plan.")
                }
            },
        );

        set(
            ListSessions,
            match product {
                Product::Postgresql | Product::Mysql | Product::MariaDb | Product::ClickHouse => {
                    State::Available
                }
                Product::Redis | Product::Valkey | Product::MongoDb => State::Available,
                Product::Sqlite => {
                    State::unsupported("SQLite is a file; there are no sessions to list.")
                }
                Product::Elasticsearch | Product::OpenSearch => State::Available,
            },
        );

        set(
            SessionReadOnly,
            match product {
                Product::Postgresql => State::Available,
                Product::Mysql | Product::MariaDb => State::Available,
                Product::Sqlite => State::Available,
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

/// What an object path means for this product, outermost first.
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
        // "Unavailable" with no reason leaves the user hunting.
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
        // But it can still be browsed, read and changed.
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
        // 4.0 brought them, and only on a replica set.
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
            ..standalone.clone()
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
        // Before connecting, the product's general answer is the honest one.
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
