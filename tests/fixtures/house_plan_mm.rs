//! Shared Pre-Graph-Builder für Case 2 (LMCS2024 Terrace-House).
//!
//! L-Seite nach LMCS2024 Fig. 3(a):
//!
//!   h₁(Nook) --next--> h₂(Villa) --next--> h₃(Cube)
//!
//! Die R-Seite (Construction + Cellar/Floor/SaddleRoof) wird durch
//! die Rules im Initial-Sync erzeugt.

use seesaw_tgg::graph::{GhostId, Status, TypedGraph};
use std::collections::{BTreeMap, HashMap};

pub struct HouseSnapshot {
    pub ids: HashMap<&'static str, GhostId>,
}

/// Baut den L-seitigen Pre-Graph: 3 Houses + 2 next-Kanten.
/// Jedes House trägt `type` und `name`-Attribute.
pub fn build_fig3a_graph() -> (TypedGraph, HouseSnapshot) {
    let mut g = TypedGraph::new();
    let h1 = g.add_baseline_node("House", "h1", attrs(&[("name", "h1"), ("type", "Nook")]));
    let h2 = g.add_baseline_node("House", "h2", attrs(&[("name", "h2"), ("type", "Villa")]));
    let h3 = g.add_baseline_node("House", "h3", attrs(&[("name", "h3"), ("type", "Cube")]));
    g.add_edge(h1, h2, "next", BTreeMap::new(), Status::Solid);
    g.add_edge(h2, h3, "next", BTreeMap::new(), Status::Solid);
    let mut ids = HashMap::new();
    ids.insert("h1", h1);
    ids.insert("h2", h2);
    ids.insert("h3", h3);
    (g, HouseSnapshot { ids })
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
    fn fig3a_has_three_houses_and_two_next_edges() {
        let (g, snap) = build_fig3a_graph();
        assert_eq!(snap.ids.len(), 3);
        assert_eq!(g.iter_nodes().count(), 3);
        assert_eq!(g.iter_edges().len(), 2);
    }

    #[test]
    fn house_types_are_set_correctly() {
        let (g, snap) = build_fig3a_graph();
        assert_eq!(g.get_node(&snap.ids["h1"]).unwrap().attrs["type"], "Nook");
        assert_eq!(g.get_node(&snap.ids["h2"]).unwrap().attrs["type"], "Villa");
        assert_eq!(g.get_node(&snap.ids["h3"]).unwrap().attrs["type"], "Cube");
    }
}
