//! # seesaw-core
//!
//! Kern-Engine für delta-extended Triple Graph Grammars mit Ghost-Overlays.
//!
//! Siehe die Kapitel T₁–T₈ im Arbeitsstand-PDF (`seesaw_rev2/main.tex`) für
//! die formale Grundlage.
//!
//! ## Module
//!
//! - [`graph`]: typisierte attributierte Graphen, Status-Annotation,
//!   parent-rooted Ghost-IDs (T₂, T₅).
//! - [`ops`]: atomare Operationen, Delta-Einträge, Overlay-Anwendung,
//!   Rollup-Index κ (T₁, T₂, T₃).
//! - [`engine`]: Pattern-Matching, Rule-Trait, Rang-Selektion, Cascade,
//!   Backtracking (T₃, T₄, T₆).
//! - [`fold`]: Nullifikation, Konsolidierung, Materialisierung,
//!   Netto-Delta, Transitions-Graph (T₅, T₇).

pub mod engine;
pub mod fold;
pub mod graph;
pub mod ops;
pub mod rule;
pub mod viz;
pub mod xmi;
