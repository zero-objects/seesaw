//! Case 8 — Cycle of Corrs (eigener Case, kein Paper-Beleg).
//!
//! Pathologie: zyklische L-Topologie (A → B → C → A). TGG-Tools
//! mit naivem Forward-Translate können auf solchen Inputs in
//! Endlosschleifen laufen, weil Match-Enumeration und Rule-
//! Anwendung sich gegenseitig speisen.
//!
//! Seesaw-Claim: deterministische Termination via:
//! - **Duplikations-Saturation** (F11 Säule 1): jede Rule-Anwendung
//!   produziert eindeutige Ghost-IDs; bei Wiederholung greift der
//!   Filter.
//! - **Canonical μ Match-Enumeration** (Def. 4.2 im Paper):
//!   `find_matches` gibt eine kanonische Reihenfolge ohne
//!   Endlosschleifen.

use seesaw_tgg::engine::{run_cascade, Cascade, Rule, TerminationState};
use seesaw_tgg::graph::{Status, TypedGraph};
use seesaw_tgg::rule::spec::parse_ruleset;
use seesaw_tgg::rule::{compile, instantiate};
use std::collections::BTreeMap;

const FIXTURE: &str = r#"{
  "name": "cycle-of-corrs",
  "rules": [
    {
      "name": "NodeToR",
      "rank": 30,
      "documentation": "Bidirektionale Triple-Rule: L-Node n → R-Node r mit Corr.",
      "l_pattern": {
        "nodes": [{ "id": "n", "kind": "LNode", "constraints": [] }],
        "edges": []
      },
      "r_pattern": {
        "nodes": [
          { "id": "n", "kind": "LNode", "constraints": [] },
          { "id": "r", "kind": "RNode", "constraints": [] }
        ],
        "edges": []
      },
      "correspondence_links": [
        {
          "l_node_id": "n",
          "r_node_id": "r",
          "kind": "CorrNode",
          "attribute_bindings": [
            { "l_attr_name": "name", "r_attr_name": "name", "transformation": "identity" }
          ]
        }
      ]
    }
  ]
}"#;

fn load_rules() -> Vec<Box<dyn Rule>> {
    let rs = parse_ruleset(FIXTURE).unwrap();
    rs.rules
        .iter()
        .map(|r| instantiate(&compile(r).unwrap()))
        .collect()
}

fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Baut einen zyklischen L-Pre-Graph: A → B → C → A.
fn build_cyclic_graph() -> TypedGraph {
    let mut g = TypedGraph::new();
    let a = g.add_baseline_node("LNode", "A", attrs(&[("name", "A")]));
    let b = g.add_baseline_node("LNode", "B", attrs(&[("name", "B")]));
    let c = g.add_baseline_node("LNode", "C", attrs(&[("name", "C")]));
    g.add_edge(a, b, "next", BTreeMap::new(), Status::Solid);
    g.add_edge(b, c, "next", BTreeMap::new(), Status::Solid);
    g.add_edge(c, a, "next", BTreeMap::new(), Status::Solid); // Zyklus zu A
    g
}

#[test]
fn case08_cyclic_graph_terminates() {
    // Engine darf auf zyklischer L-Topologie nicht in Endlosschleifen
    // laufen. Mit max_steps=20 als Safeguard erwarten wir terminale
    // Konvergenz/Duplication, kein StepLimitExceeded.
    let mut g = build_cyclic_graph();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let mut cas = Cascade::new();
    let term = run_cascade(&mut cas, &mut g, &refs, 20).expect("Termination ohne Step-Limit");
    eprintln!(
        "Case 8 Termination: {term:?}, entries: {}",
        cas.entries.len()
    );
    assert!(
        matches!(
            term,
            TerminationState::Convergence | TerminationState::Duplication
        ),
        "Engine terminiert deterministisch auf zyklischer Topologie"
    );
}

#[test]
fn case08_each_l_node_produces_one_r_node() {
    let mut g = build_cyclic_graph();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let mut cas = Cascade::new();
    let _ = run_cascade(&mut cas, &mut g, &refs, 20).unwrap();

    // Drei L-Knoten → drei R-Knoten, drei Corrs.
    let r_nodes = g.iter_nodes().filter(|n| n.type_id == "RNode").count();
    let corrs = g.iter_nodes().filter(|n| n.type_id == "CorrNode").count();
    assert_eq!(r_nodes, 3, "3 L-Nodes → 3 R-Nodes");
    assert_eq!(corrs, 3, "3 Corrs (eine pro L-R-Paar)");
}

#[test]
fn case08_canonical_enumeration_deterministic() {
    // Wiederholter Lauf liefert deterministische Cascade-Length.
    // Das belegt die kanonische Match-Enumeration μ.
    let mut len_first = 0;
    for run in 0..3 {
        let mut g = build_cyclic_graph();
        let rules = load_rules();
        let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
        let mut cas = Cascade::new();
        let _ = run_cascade(&mut cas, &mut g, &refs, 20).unwrap();
        if run == 0 {
            len_first = cas.entries.len();
        } else {
            assert_eq!(
                cas.entries.len(),
                len_first,
                "Determinismus: Cascade-Länge identisch über Runs"
            );
        }
    }
}

#[test]
fn case08_self_loop_terminates() {
    // Edge-Case: Self-Loop (A → A). Rule sollte 1× feuern, dann
    // Termination.
    let mut g = TypedGraph::new();
    let a = g.add_baseline_node("LNode", "A", attrs(&[("name", "A")]));
    g.add_edge(a, a, "next", BTreeMap::new(), Status::Solid);

    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let mut cas = Cascade::new();
    let term = run_cascade(&mut cas, &mut g, &refs, 20).expect("Termination");
    eprintln!("Case 8 Self-Loop Termination: {term:?}");
    let r_count = g.iter_nodes().filter(|n| n.type_id == "RNode").count();
    assert_eq!(r_count, 1, "Self-Loop: 1 R-Node");
}
