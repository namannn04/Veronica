//! Reading a bounded slice of something.
//!
//! A port of Edith's `Paging.swift` and `Filters.swift`. The point of both is
//! that no read is unbounded: a page has a size with a hard ceiling, so
//! `SELECT * FROM events` against a billion-row table returns a screenful
//! rather than filling memory and taking the app down with it.

use serde::{Deserialize, Serialize};

use crate::value::Value;

/// Edith's page bounds exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct PageSize(u32);

impl PageSize {
    pub const MIN: u32 = 1;
    pub const MAX: u32 = 2_000;
    pub const DEFAULT: u32 = 200;

    pub fn new(value: u32) -> anyhow::Result<Self> {
        if !(Self::MIN..=Self::MAX).contains(&value) {
            anyhow::bail!(
                "a page size must be between {} and {}, not {value}",
                Self::MIN,
                Self::MAX
            );
        }
        Ok(Self(value))
    }

    /// Clamp rather than refuse. For a `--limit` flag, where "as many as you
    /// can" is a reasonable thing to mean and an error would be pedantic.
    pub fn clamped(value: u32) -> Self {
        Self(value.clamp(Self::MIN, Self::MAX))
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl Default for PageSize {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

impl TryFrom<u32> for PageSize {
    type Error = String;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value).map_err(|error| error.to_string())
    }
}

impl From<PageSize> for u32 {
    fn from(value: PageSize) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sort {
    /// Field path components, outermost first.
    pub path: Vec<String>,
    #[serde(default)]
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectionMode {
    #[default]
    Include,
    Exclude,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Projection {
    pub mode: ProjectionMode,
    pub paths: Vec<Vec<String>>,
}

/// Edith's eighteen operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterOperator {
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Contains,
    StartsWith,
    EndsWith,
    In,
    NotIn,
    Between,
    IsNull,
    IsNotNull,
    IsMissing,
    IsNotMissing,
    RegularExpression,
    FullText,
}

impl FilterOperator {
    /// Whether the operator takes any values at all. `IS NULL` with a value is
    /// a malformed filter, not a filter with an ignored field.
    pub fn arity(self) -> usize {
        match self {
            FilterOperator::IsNull
            | FilterOperator::IsNotNull
            | FilterOperator::IsMissing
            | FilterOperator::IsNotMissing => 0,
            FilterOperator::Between => 2,
            FilterOperator::In | FilterOperator::NotIn => usize::MAX,
            _ => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaseSensitivity {
    #[default]
    ProductDefault,
    Sensitive,
    Insensitive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterPredicate {
    pub path: Vec<String>,
    pub operator: FilterOperator,
    #[serde(default)]
    pub values: Vec<Value>,
    #[serde(default)]
    pub case_sensitivity: CaseSensitivity,
}

impl FilterPredicate {
    /// Whether the values match what the operator takes.
    pub fn is_well_formed(&self) -> bool {
        if self.path.is_empty() {
            return false;
        }
        match self.operator.arity() {
            usize::MAX => !self.values.is_empty(),
            expected => self.values.len() == expected,
        }
    }
}

/// A tree, so `a AND (b OR c)` is expressible without a parser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Filter {
    Predicate { predicate: FilterPredicate },
    All { children: Vec<Filter> },
    Any { children: Vec<Filter> },
    Not { child: Box<Filter> },
}

impl Filter {
    /// How deep the tree goes. A filter nested a thousand levels down would
    /// recurse an adapter to death, so the caller checks before building.
    pub fn depth(&self) -> usize {
        match self {
            Filter::Predicate { .. } => 1,
            Filter::All { children } | Filter::Any { children } => {
                1 + children.iter().map(Filter::depth).max().unwrap_or(0)
            }
            Filter::Not { child } => 1 + child.depth(),
        }
    }

    /// Every predicate in the tree, for validating one filter in one pass.
    pub fn predicates(&self) -> Vec<&FilterPredicate> {
        match self {
            Filter::Predicate { predicate } => vec![predicate],
            Filter::All { children } | Filter::Any { children } => {
                children.iter().flat_map(Filter::predicates).collect()
            }
            Filter::Not { child } => child.predicates(),
        }
    }

    pub fn is_well_formed(&self) -> bool {
        const MAX_DEPTH: usize = 12;
        self.depth() <= MAX_DEPTH
            && !self.predicates().is_empty()
            && self
                .predicates()
                .iter()
                .all(|predicate| predicate.is_well_formed())
    }
}

/// One bounded read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRequest {
    #[serde(default)]
    pub page_size: PageSize,
    /// How many records to skip. Offset paging is what every product supports;
    /// a keyset continuation is better and is what an adapter uses when it can.
    #[serde(default)]
    pub offset: u64,
    pub projection: Option<Projection>,
    pub filter: Option<Filter>,
    #[serde(default)]
    pub sorts: Vec<Sort>,
}

/// One column of a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDescriptor {
    pub name: String,
    /// What the product calls the type, where it says.
    pub type_name: Option<String>,
}

/// A page of records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub columns: Vec<ColumnDescriptor>,
    pub rows: Vec<Vec<Value>>,
    /// True when the server had more to give. Distinguishes "that is all of it"
    /// from "that is the first two hundred", which changes what a total means.
    pub has_more: bool,
    /// The offset to ask for next, when there is more.
    pub next_offset: Option<u64>,
    /// How long the read took, which is the number that tells you a query needs
    /// an index.
    pub elapsed_millis: u64,
}

impl Page {
    pub fn empty() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            has_more: false,
            next_offset: None,
            elapsed_millis: 0,
        }
    }

    /// Build a page from rows the adapter fetched, which it fetches one over
    /// the page size so "is there more" is known rather than guessed.
    pub fn from_rows(
        columns: Vec<ColumnDescriptor>,
        mut rows: Vec<Vec<Value>>,
        request: &PageRequest,
        elapsed_millis: u64,
    ) -> Self {
        let size = request.page_size.get() as usize;
        let has_more = rows.len() > size;
        rows.truncate(size);
        Self {
            columns,
            next_offset: has_more.then(|| request.offset + rows.len() as u64),
            rows,
            has_more,
            elapsed_millis,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ediths_page_bounds_are_kept_exactly() {
        assert_eq!(
            (PageSize::MIN, PageSize::MAX, PageSize::DEFAULT),
            (1, 2_000, 200)
        );
        assert_eq!(PageSize::default().get(), 200);
    }

    #[test]
    fn a_page_size_outside_the_range_is_refused_when_stored() {
        // So no read is ever unbounded.
        assert!(PageSize::new(0).is_err());
        assert!(PageSize::new(2_001).is_err());
        assert!(serde_json::from_str::<PageSize>("100000").is_err());
    }

    #[test]
    fn a_limit_flag_clamps_rather_than_erroring() {
        // "As many as you can" is a reasonable thing to type.
        assert_eq!(PageSize::clamped(1_000_000).get(), 2_000);
        assert_eq!(PageSize::clamped(0).get(), 1);
        assert_eq!(PageSize::clamped(50).get(), 50);
    }

    #[test]
    fn an_operator_that_takes_no_value_is_malformed_with_one() {
        let predicate = FilterPredicate {
            path: vec!["deleted_at".into()],
            operator: FilterOperator::IsNull,
            values: vec![Value::Null],
            case_sensitivity: CaseSensitivity::default(),
        };
        assert!(!predicate.is_well_formed());

        let bare = FilterPredicate {
            values: vec![],
            ..predicate
        };
        assert!(bare.is_well_formed());
    }

    #[test]
    fn between_needs_exactly_two_values_and_in_needs_at_least_one() {
        let build = |operator, count| FilterPredicate {
            path: vec!["n".into()],
            operator,
            values: vec![Value::SignedInteger(1); count],
            case_sensitivity: CaseSensitivity::default(),
        };
        assert!(!build(FilterOperator::Between, 1).is_well_formed());
        assert!(build(FilterOperator::Between, 2).is_well_formed());
        assert!(!build(FilterOperator::Between, 3).is_well_formed());
        assert!(!build(FilterOperator::In, 0).is_well_formed());
        assert!(build(FilterOperator::In, 5).is_well_formed());
    }

    #[test]
    fn a_predicate_with_no_path_targets_nothing_and_is_malformed() {
        let predicate = FilterPredicate {
            path: vec![],
            operator: FilterOperator::Equal,
            values: vec![Value::Null],
            case_sensitivity: CaseSensitivity::default(),
        };
        assert!(!predicate.is_well_formed());
    }

    fn leaf(name: &str) -> Filter {
        Filter::Predicate {
            predicate: FilterPredicate {
                path: vec![name.into()],
                operator: FilterOperator::IsNull,
                values: vec![],
                case_sensitivity: CaseSensitivity::default(),
            },
        }
    }

    #[test]
    fn a_filter_tree_reports_its_depth_and_every_predicate() {
        let filter = Filter::All {
            children: vec![
                leaf("a"),
                Filter::Any {
                    children: vec![
                        leaf("b"),
                        Filter::Not {
                            child: Box::new(leaf("c")),
                        },
                    ],
                },
            ],
        };
        assert_eq!(filter.depth(), 4);
        assert_eq!(filter.predicates().len(), 3);
        assert!(filter.is_well_formed());
    }

    #[test]
    fn a_filter_nested_past_the_limit_is_refused() {
        // A thousand levels would recurse an adapter to death.
        let mut filter = leaf("a");
        for _ in 0..40 {
            filter = Filter::Not {
                child: Box::new(filter),
            };
        }
        assert!(!filter.is_well_formed());
    }

    #[test]
    fn an_empty_filter_group_is_not_a_filter() {
        assert!(!Filter::All { children: vec![] }.is_well_formed());
    }

    #[test]
    fn a_page_knows_there_is_more_because_it_asked_for_one_extra() {
        let request = PageRequest {
            page_size: PageSize::new(2).unwrap(),
            ..PageRequest::default()
        };
        let columns = vec![ColumnDescriptor {
            name: "id".into(),
            type_name: None,
        }];
        let rows = vec![
            vec![Value::SignedInteger(1)],
            vec![Value::SignedInteger(2)],
            vec![Value::SignedInteger(3)],
        ];
        let page = Page::from_rows(columns.clone(), rows, &request, 5);
        assert_eq!(page.rows.len(), 2, "the extra row is not shown");
        assert!(page.has_more);
        assert_eq!(page.next_offset, Some(2));

        let exact = Page::from_rows(columns, vec![vec![Value::SignedInteger(1)]], &request, 5);
        assert!(!exact.has_more, "one short of the page is the end");
        assert_eq!(exact.next_offset, None);
    }

    #[test]
    fn a_page_request_round_trips_with_its_filter() {
        let request = PageRequest {
            page_size: PageSize::new(10).unwrap(),
            offset: 20,
            projection: Some(Projection {
                mode: ProjectionMode::Include,
                paths: vec![vec!["id".into()]],
            }),
            filter: Some(leaf("deleted_at")),
            sorts: vec![Sort {
                path: vec!["created_at".into()],
                direction: SortDirection::Descending,
            }],
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(serde_json::from_str::<PageRequest>(&json).unwrap(), request);
    }
}
