//! Visualisierungs-Hilfen für Debugging und Paper-Figuren.
//!
//! Das `dot`-Submodul rendert `TypedGraph` und `Cascade` in GraphViz-DOT.
//! File-IO-Helfer sind hinter `cfg(feature = "regen_graphs")` gestellt,
//! damit reguläre `cargo test`-Läufe nichts überschreiben.

pub mod dot;
