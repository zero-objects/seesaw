//! # seesaw-core
//!
//! Core engine for delta-extended Triple Graph Grammars with ghost
//! overlays.
//!
//! See the accompanying paper (chapters T₁–T₈) for the formal
//! foundation.
//!
//! ## Modules
//!
//! - [`ident`]: the two shared types. `GhostId` is the structurally
//!   derived identity of every node, `Status` its lifecycle state.
//! - [`rules`]: the rule format. Declarative bidirectional rules,
//!   loading and validation, lowering into two directed creation plans.
//!   [`rules::load`] is the one way from a rule file to plans.
//! - [`graph`]: the model. Map-based, anonymous connections, value-free
//!   identity.
//! - [`plan`]: what a lowered rule is — a creation plan the engine
//!   executes.
//! - [`engine`]: the delta-local cascade with retraction.
//!
//! Users write rules against [`rules`]. The layers below become visible
//! only where a cascade is driven or a graph is read.

/// Counts structural work under the `perf_trace` feature; expands to
/// nothing without it. `trace!(field)` adds one, `trace!(field, n)`
/// adds `n`.
#[cfg(feature = "perf_trace")]
macro_rules! trace {
    ($f:ident) => {
        $crate::perf_trace::add(|t| t.$f += 1)
    };
    ($f:ident, $n:expr) => {
        $crate::perf_trace::add(|t| t.$f += ($n) as u64)
    };
}
#[cfg(not(feature = "perf_trace"))]
macro_rules! trace {
    ($($t:tt)*) => {};
}

pub mod engine;
pub mod graph;
pub mod hash;
pub mod ident;
#[cfg(feature = "perf_trace")]
pub mod perf_trace;
pub mod plan;
pub mod rules;
mod values;

#[cfg(feature = "opt_f")]
pub use values::ValueNumbers;
