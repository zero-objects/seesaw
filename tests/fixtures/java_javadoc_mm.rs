//! Shared Pre-Graph für Case 7a/b/c (FAoC2021 Schema-Violations).
//!
//! L-Seite (Java-AST, vereinfacht):
//!   - Doc mit GlossaryEntries
//!   - Class mit Methods und Fields
//!
//! Verschiedene `build_*`-Helper für die drei Sub-Cases.

use seesaw_tgg::graph::{GhostId, Status, TypedGraph};
use std::collections::{BTreeMap, HashMap};

#[allow(dead_code)]
pub struct JavaSnap {
    pub ids: HashMap<&'static str, GhostId>,
}

/// 7a: Doc mit 2 GlossaryEntries — naiver TGG würde 2 Glossaries
/// erzeugen, NAC verhindert das.
pub fn build_doc_two_entries() -> (TypedGraph, JavaSnap) {
    let mut g = TypedGraph::new();
    let d = g.add_baseline_node("Doc", "D", attrs(&[("name", "MyDoc")]));
    let e1 = g.add_baseline_node("GlossaryEntry", "E1", attrs(&[("name", "foo")]));
    let e2 = g.add_baseline_node("GlossaryEntry", "E2", attrs(&[("name", "bar")]));
    g.add_edge(d, e1, "entries", BTreeMap::new(), Status::Solid);
    g.add_edge(d, e2, "entries", BTreeMap::new(), Status::Solid);
    let mut ids = HashMap::new();
    ids.insert("D", d);
    ids.insert("E1", e1);
    ids.insert("E2", e2);
    (g, JavaSnap { ids })
}

/// 7b: zwei Classes, eine mit Method, eine leer — NAC verhindert
/// Translation der leeren Class.
pub fn build_two_classes_one_empty() -> (TypedGraph, JavaSnap) {
    let mut g = TypedGraph::new();
    let c1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
    let c2 = g.add_baseline_node("Class", "C2", attrs(&[("name", "C2")]));
    let m1 = g.add_baseline_node("Method", "M1", attrs(&[("name", "m1")]));
    g.add_edge(c1, m1, "methods", BTreeMap::new(), Status::Solid);
    let mut ids = HashMap::new();
    ids.insert("C1", c1);
    ids.insert("C2", c2);
    ids.insert("M1", m1);
    (g, JavaSnap { ids })
}

/// 7c: zwei Methods mit gleichem Namen "foo" — beide sollten auf
/// denselben GlossaryEntry verlinken.
pub fn build_two_methods_same_name() -> (TypedGraph, JavaSnap) {
    let mut g = TypedGraph::new();
    let c = g.add_baseline_node("Class", "C", attrs(&[("name", "C")]));
    let m1 = g.add_baseline_node("Method", "M1", attrs(&[("name", "foo")]));
    let m2 = g.add_baseline_node("Method", "M2", attrs(&[("name", "foo")]));
    g.add_edge(c, m1, "methods", BTreeMap::new(), Status::Solid);
    g.add_edge(c, m2, "methods", BTreeMap::new(), Status::Solid);
    let mut ids = HashMap::new();
    ids.insert("C", c);
    ids.insert("M1", m1);
    ids.insert("M2", m2);
    (g, JavaSnap { ids })
}

fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}
