//! # seesaw-tgg
//!
//! Core engine for delta-extended Triple Graph Grammars with ghost
//! overlays.
//!
//! See the accompanying paper (chapters T₁–T₈) for the formal
//! foundation.
//!
//! ## Modules
//!
//! - [`graph`]: typed attributed graphs, status annotation, parent-rooted
//!   ghost ids (T₂, T₅).
//! - [`ops`]: atomic operations, delta entries, overlay application,
//!   rollup index κ (T₁, T₂, T₃).
//! - [`engine`]: pattern matching, the `Rule` trait, rank-based selection,
//!   cascade, backtracking (T₃, T₄, T₆).
//! - [`fold`]: nullification, consolidation, materialization, net delta,
//!   transition graph (T₅, T₇).

pub mod engine;
pub mod fold;
pub mod graph;
pub mod hash;
pub mod ops;
#[cfg(feature = "perf_trace")]
pub mod perf_trace;
pub mod rule;
pub mod viz;
pub mod xmi;
