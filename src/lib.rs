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

pub mod engine;
pub mod graph;
pub mod hash;
pub mod ident;
#[cfg(feature = "perf_trace")]
pub mod perf_trace;
pub mod plan;
pub mod rules;
