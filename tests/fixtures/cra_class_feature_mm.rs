//! Shared Pre-Graph für Case 6 (ICGT2020 CRA empty-class).
//!
//! L-Seite (CRA-Welt nach Kosiol et al. 2020 Fig. 1, vereinfacht):
//!   1 Class `C` mit 2 Features `F1`, `F2`, je via `contains`-Edge.

use seesaw_tgg::graph::{GhostId, Status, TypedGraph};
use std::collections::{BTreeMap, HashMap};

pub struct CraSnapshot {
    pub ids: HashMap<&'static str, GhostId>,
}

pub fn build_pre_graph() -> (TypedGraph, CraSnapshot) {
    let mut g = TypedGraph::new();
    let c = g.add_baseline_node("Class", "C", attrs(&[("name", "C")]));
    let f1 = g.add_baseline_node("Feature", "F1", attrs(&[("name", "F1")]));
    let f2 = g.add_baseline_node("Feature", "F2", attrs(&[("name", "F2")]));
    g.add_edge(c, f1, "contains", BTreeMap::new(), Status::Solid);
    g.add_edge(c, f2, "contains", BTreeMap::new(), Status::Solid);
    let mut ids = HashMap::new();
    ids.insert("C", c);
    ids.insert("F1", f1);
    ids.insert("F2", f2);
    (g, CraSnapshot { ids })
}

fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}
