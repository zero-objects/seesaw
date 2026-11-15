//! Values as the anchor index and the search compare them with the
//! constants of `Predicate::Equals`.
//!
//! One private interface, two implementations, chosen by the feature
//! `opt_f`:
//!
//! - Without it ([`plain`]) a value is its text. A lookup by value
//!   hashes the text and compares it with the constant.
//! - With it ([`numbered`]) every constant an `Equals` predicate of the
//!   rule set compares with has a number in the [`ValueNumbers`] of the
//!   anchor index, and a stored value keeps, beside its text, the number
//!   that text has there (a [`Cell`]). Index and search then compare
//!   numbers. Stored values are the rule constants of the graph
//!   ([`KonstTable`](crate::graph::KonstTable)) and the values of a
//!   resolver that answers `ValueResolver::baseline_number`
//!   ([`ValueStore`](crate::graph::ValueStore) does). A value without a
//!   number (a derived leaf, a resolver that lends only text) is
//!   compared by its text as before.
//!
//! Both answer the same: the numbering maps the constants one to one,
//! so two numbers are equal exactly when the texts are, and a text
//! without a number equals none of the constants.

#[cfg(feature = "opt_f")]
mod numbered;
#[cfg(not(feature = "opt_f"))]
mod plain;

#[cfg(feature = "opt_f")]
pub use numbered::ValueNumbers;
#[cfg(feature = "opt_f")]
pub(crate) use numbered::{
    holds, see, wanted, ByValue, Cell, Hint, Numbering, Numbers, NumbersMut, Seen, Shared, Wanted,
};
#[cfg(not(feature = "opt_f"))]
pub(crate) use plain::{
    holds, see, wanted, ByValue, Cell, Hint, Numbering, Numbers, NumbersMut, Seen, Shared, Wanted,
};

/// A value a store keeps: its text and the [`Cell`] beside it.
#[derive(Clone, Default)]
pub(crate) struct Kept {
    pub(crate) text: String,
    #[cfg_attr(not(feature = "opt_f"), allow(dead_code))]
    pub(crate) cell: Cell,
}

impl Kept {
    pub(crate) fn new(text: String) -> Self {
        Self {
            text,
            cell: Cell::default(),
        }
    }
}

impl std::fmt::Debug for Kept {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.text, f)
    }
}
