//! Visualization helpers for debugging and paper figures.
//!
//! The `dot` submodule renders `TypedGraph` and `Cascade` as GraphViz DOT.
//! File-IO helpers sit behind `cfg(feature = "regen_graphs")` so that
//! regular `cargo test` runs do not overwrite anything.

pub mod dot;
