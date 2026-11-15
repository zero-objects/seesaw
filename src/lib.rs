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
//!
//! ## Features
//!
//! - `low_memory`: less memory for more work. The graph keeps nodes
//!   and connections in dense vectors behind a map of numbers, the
//!   engine its lists of indices per element in 16 bytes instead of
//!   32. Measured on the performance suite: 17 to 24 % less peak
//!   memory in four of five large cells (1 % in the fifth), up to 4 %
//!   more instructions (6 to 8 % where a run deletes). Time on a
//!   machine under load: 7 to 9 % more on the two recorded host runs,
//!   no difference beyond the noise on the generated workloads. Graph,
//!   cascade and verdicts are the same with and without it.
//! - `parallel_seed`: `Engine::seed_with_threads`, the searches of a
//!   seed on several threads with the matches recorded in the order of
//!   `Engine::seed`. Not on wasm32.
//! - `perf_trace`: structural counters of the cascade, for
//!   diagnostics.

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
