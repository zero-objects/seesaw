//! Shared Pre-Graph-Builder für Case 1/1b/1c — **saubere bidirektionale
//! TGG-Fassung** (Überarbeitung nach Opt.1-Entscheidung 2026-04-24).
//!
//! Vorher: Pre-Graph enthielt L- und R-Seite vermischt (Packages mit
//! nested Folders), was Rules und Engine-Semantik inkonsistent machte.
//!
//! Jetzt: Pre-Graph hat **nur die L-Seite** (Source-Modell):
//!
//!   rootP --subPackages--> p --classes--> c
//!
//! Die R-Seite (Folder/DocFile + Corrs) wird von den TGG-Rules
//! während der Initial-Synchronisation erzeugt.
//!
//! STTT2021-Fig-3a-Äquivalent: das Source-Modell vor dem User-Delta.

use seesaw_tgg::graph::{GhostId, TypedGraph};
use std::collections::{BTreeMap, HashMap};

pub struct SubtreeSnapshot {
    pub ids: HashMap<&'static str, GhostId>,
}

/// Baut den L-seitigen Pre-Graph: 3 Baseline-Package/Class-Knoten,
/// 2 Edges. Kein CorrPackage, kein Folder, kein DocFile — die werden
/// durch Initial-Sync erzeugt.
///
/// Rückgabe: `(graph, snapshot)`. `snapshot.ids` enthält die
/// L-Seiten-IDs für γ-Assertions.
pub fn build_fig3a_graph() -> (TypedGraph, SubtreeSnapshot) {
    use seesaw_tgg::graph::Status;
    let mut g = TypedGraph::new();

    let root_p = g.add_baseline_node("Package", "rootP", attrs(&[("name", "rootP")]));
    let p = g.add_baseline_node("Package", "p", attrs(&[("name", "p")]));
    let c = g.add_baseline_node("Class", "c", attrs(&[("name", "c")]));

    g.add_edge(root_p, p, "subPackages", BTreeMap::new(), Status::Solid);
    g.add_edge(p, c, "classes", BTreeMap::new(), Status::Solid);

    let mut ids = HashMap::new();
    ids.insert("rootP", root_p);
    ids.insert("p", p);
    ids.insert("c", c);

    (g, SubtreeSnapshot { ids })
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
    fn fig3a_l_side_has_expected_structure() {
        let (g, snap) = build_fig3a_graph();
        assert_eq!(snap.ids.len(), 3, "3 Baseline-L-Knoten");
        let node_count = g.iter_nodes().count();
        assert_eq!(node_count, 3);
        let edge_count = g.iter_edges().len();
        assert_eq!(edge_count, 2, "subPackages + classes");
    }

    #[test]
    fn class_has_name_attribute() {
        let (g, snap) = build_fig3a_graph();
        let c = g.get_node(&snap.ids["c"]).unwrap();
        assert_eq!(c.attrs.get("name").map(|s| s.as_str()), Some("c"));
    }
}
