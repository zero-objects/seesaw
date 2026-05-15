//! Shared Pre-Graph für Case 4 (SLE2020 Concurrent delete+modify).
//!
//! L-Seite (Java-AST-Welt nach SLE2020 Fig. 2, vereinfacht):
//!
//! ```text
//!   C1: Class, M1: Method (unter C1, methods-Edge)
//!   C2: Class, M2: Method (unter C2)
//! ```
//!
//! Die R-Seite (Doc + Entry) wird durch TGG-Rules erzeugt.

use seesaw_tgg::graph::{GhostId, Status, TypedGraph};
use std::collections::{BTreeMap, HashMap};

pub struct ClassDocSnapshot {
    pub ids: HashMap<&'static str, GhostId>,
}

pub fn build_pre_graph() -> (TypedGraph, ClassDocSnapshot) {
    let mut g = TypedGraph::new();
    let c1 = g.add_baseline_node("Class", "C1", attrs(&[("name", "C1")]));
    let c2 = g.add_baseline_node("Class", "C2", attrs(&[("name", "C2")]));
    let m1 = g.add_baseline_node("Method", "M1", attrs(&[("name", "M1")]));
    let m2 = g.add_baseline_node("Method", "M2", attrs(&[("name", "M2")]));
    g.add_edge(c1, m1, "methods", BTreeMap::new(), Status::Solid);
    g.add_edge(c2, m2, "methods", BTreeMap::new(), Status::Solid);
    let mut ids = HashMap::new();
    ids.insert("C1", c1);
    ids.insert("C2", c2);
    ids.insert("M1", m1);
    ids.insert("M2", m2);
    (g, ClassDocSnapshot { ids })
}

fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_graph_has_2_classes_2_methods() {
        let (g, snap) = build_pre_graph();
        assert_eq!(snap.ids.len(), 4);
        assert_eq!(g.iter_nodes().count(), 4);
        assert_eq!(g.iter_edges().len(), 2);
    }
}
