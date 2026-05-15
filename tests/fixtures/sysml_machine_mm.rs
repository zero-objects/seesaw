//! Shared Pre-Graph für Case 3 (JOT2022 SysML dangling transition).
//!
//! L-Seite (SysML-Welt nach JOT2022 Fig. 3) — minimaler Pre-Graph:
//!
//! ```text
//!   sm: Statemachine
//!   sm --states--> s1(kind=START)
//!   sm --states--> s2(kind=STOP)
//!   sm --variables--> v
//!   sm --transitions--> t1(source=s1, target=s2)   ← valide
//!   sm --transitions--> t2(source=s1, target=⊥)    ← DANGLING
//! ```
//!
//! Die R-Seite (Event-B Machine + EventBlock + EventVariable +
//! MachineEdge) entsteht via TGG-Sync. t2 hat **keine** target-Edge,
//! daher matcht TransToEdge nicht für t2 — bleibt un-übersetzt.

use seesaw_tgg::graph::{GhostId, Status, TypedGraph};
use std::collections::{BTreeMap, HashMap};

pub struct SysMlSnapshot {
    pub ids: HashMap<&'static str, GhostId>,
}

/// Baut den L-seitigen Pre-Graph mit einer **dangling Transition**.
pub fn build_dangling_graph() -> (TypedGraph, SysMlSnapshot) {
    let mut g = TypedGraph::new();
    let sm = g.add_baseline_node("Statemachine", "sm", attrs(&[("name", "sm")]));
    let s1 = g.add_baseline_node("State", "s1", attrs(&[("name", "s1"), ("kind", "START")]));
    let s2 = g.add_baseline_node("State", "s2", attrs(&[("name", "s2"), ("kind", "STOP")]));
    let v = g.add_baseline_node("Variable", "v", attrs(&[("name", "finish")]));
    let t1 = g.add_baseline_node("Transition", "t1", attrs(&[("name", "t1")]));
    let t2 = g.add_baseline_node("Transition", "t2", attrs(&[("name", "t2")]));

    // Statemachine-Containment
    g.add_edge(sm, s1, "states", BTreeMap::new(), Status::Solid);
    g.add_edge(sm, s2, "states", BTreeMap::new(), Status::Solid);
    g.add_edge(sm, v, "variables", BTreeMap::new(), Status::Solid);
    g.add_edge(sm, t1, "transitions", BTreeMap::new(), Status::Solid);
    g.add_edge(sm, t2, "transitions", BTreeMap::new(), Status::Solid);

    // t1 ist valide: source=s1, target=s2
    g.add_edge(t1, s1, "source", BTreeMap::new(), Status::Solid);
    g.add_edge(t1, s2, "target", BTreeMap::new(), Status::Solid);

    // t2 ist DANGLING: source=s1, KEINE target-Edge
    g.add_edge(t2, s1, "source", BTreeMap::new(), Status::Solid);
    // bewusst kein target

    let mut ids = HashMap::new();
    ids.insert("sm", sm);
    ids.insert("s1", s1);
    ids.insert("s2", s2);
    ids.insert("v", v);
    ids.insert("t1", t1);
    ids.insert("t2", t2);
    (g, SysMlSnapshot { ids })
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
    fn dangling_graph_has_six_nodes_and_eight_edges() {
        let (g, snap) = build_dangling_graph();
        assert_eq!(snap.ids.len(), 6);
        assert_eq!(g.iter_nodes().count(), 6);
        // 5 Containment + 2 source + 1 target = 8 Edges
        assert_eq!(g.iter_edges().len(), 8);
    }

    #[test]
    fn t2_has_no_target_edge() {
        let (g, snap) = build_dangling_graph();
        let t2 = snap.ids["t2"];
        let target_edges: Vec<_> = g
            .iter_edges()
            .into_iter()
            .filter(|(s, _, e)| *s == t2 && e.type_id == "target")
            .collect();
        assert_eq!(target_edges.len(), 0, "t2 hat keine target-Edge (dangling)");
    }
}
