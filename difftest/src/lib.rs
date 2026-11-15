//! Differential test of the engine in this repository against the
//! published 2.0.2: the same model and the same sequence of deltas go
//! through both, and after every wave the graphs and the cascades have
//! to be equal. The tests are in `tests/`.
//!
//! The two engines are two crates with types of their own, so the code
//! that drives them is written once as a macro and expanded per crate.

pub use new;
pub use old;
