//! Bounded catalog lists. A list past the cap is `Partial` and names the cap.

use crate::ids::NodeId;
use crate::scope::ScopeId;

/// Maximum values kept in one `NodeIdList`, `ScopeIdList`, or `IntList`.
pub const MAX_CATALOG_LIST_VALUES: usize = 64;

/// Which catalog list a bound applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListKind {
    /// `List<NodeId>`.
    NodeIdList,
    /// `List<ScopeId>`.
    ScopeIdList,
    /// `List<Int>`.
    IntList,
}

impl ListKind {
    /// Stable cap name recorded on a partial list.
    #[must_use]
    pub const fn cap_name(self) -> &'static str {
        match self {
            Self::NodeIdList => "NodeIdList",
            Self::ScopeIdList => "ScopeIdList",
            Self::IntList => "IntList",
        }
    }
}

/// Whether a list was stored whole.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ListCoverage {
    /// Every value fit.
    Complete,
    /// The list was cut. `dropped` is the number of values not kept.
    Partial {
        /// Cap name from [`ListKind::cap_name`].
        cap: &'static str,
        /// Values retained.
        kept: usize,
        /// Values discarded after the cap.
        dropped: usize,
    },
}

/// A sorted, deduplicated list that stops at [`MAX_CATALOG_LIST_VALUES`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedList<T> {
    values: Box<[T]>,
    coverage: ListCoverage,
}

impl<T: Ord> BoundedList<T> {
    /// Sorts, deduplicates, and truncates `values`.
    ///
    /// Truncation keeps the lowest values so the kept slice is deterministic.
    #[must_use]
    pub fn from_sorted_dedup(kind: ListKind, mut values: Vec<T>) -> Self {
        values.sort();
        values.dedup();
        let total = values.len();
        if total > MAX_CATALOG_LIST_VALUES {
            values.truncate(MAX_CATALOG_LIST_VALUES);
            Self {
                values: values.into_boxed_slice(),
                coverage: ListCoverage::Partial {
                    cap: kind.cap_name(),
                    kept: MAX_CATALOG_LIST_VALUES,
                    dropped: total - MAX_CATALOG_LIST_VALUES,
                },
            }
        } else {
            Self {
                values: values.into_boxed_slice(),
                coverage: ListCoverage::Complete,
            }
        }
    }

    /// Values that were kept.
    #[must_use]
    pub fn values(&self) -> &[T] {
        &self.values
    }

    /// Complete, or partial with the cap name.
    #[must_use]
    pub fn coverage(&self) -> &ListCoverage {
        &self.coverage
    }

    /// Splits the list into the kept values and the coverage record.
    #[must_use]
    pub fn into_parts(self) -> (Box<[T]>, ListCoverage) {
        (self.values, self.coverage)
    }
}

/// Builds a bounded `IntList`.
#[must_use]
pub fn bound_int_list(values: Vec<i64>) -> BoundedList<i64> {
    BoundedList::from_sorted_dedup(ListKind::IntList, values)
}

/// Builds a bounded `NodeIdList`.
#[must_use]
pub fn bound_node_id_list(values: Vec<NodeId>) -> BoundedList<NodeId> {
    BoundedList::from_sorted_dedup(ListKind::NodeIdList, values)
}

/// Builds a bounded `ScopeIdList`.
#[must_use]
pub fn bound_scope_id_list(values: Vec<ScopeId>) -> BoundedList<ScopeId> {
    BoundedList::from_sorted_dedup(ListKind::ScopeIdList, values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_past_the_cap_is_partial_and_names_the_cap() {
        let ints = bound_int_list((0..65).collect());
        assert_eq!(ints.values().len(), MAX_CATALOG_LIST_VALUES);
        assert_eq!(
            ints.coverage(),
            &ListCoverage::Partial {
                cap: "IntList",
                kept: MAX_CATALOG_LIST_VALUES,
                dropped: 1,
            }
        );

        let nodes = bound_node_id_list((0..65).map(NodeId::new).collect());
        assert!(matches!(
            nodes.coverage(),
            ListCoverage::Partial {
                cap: "NodeIdList",
                dropped: 1,
                ..
            }
        ));

        let scopes = bound_scope_id_list((0..65).map(ScopeId::vlan).collect());
        assert!(matches!(
            scopes.coverage(),
            ListCoverage::Partial {
                cap: "ScopeIdList",
                dropped: 1,
                ..
            }
        ));
    }

    #[test]
    fn a_short_list_is_complete() {
        let ints = bound_int_list(vec![3, 1, 3, 2]);
        assert_eq!(ints.values(), &[1, 2, 3]);
        assert_eq!(ints.coverage(), &ListCoverage::Complete);
    }
}
