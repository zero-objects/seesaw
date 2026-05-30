//! Case 8 — Cycle of Corrs (custom case, no paper reference).
//!
//! Pathology: cyclic L-topology (A → B → C → A). TGG tools
//! with naive forward-translate can loop forever on such inputs,
//! because match enumeration and rule application feed each
//! other.
//!
//! Seesaw claim: deterministic termination via:
//! - **Duplication saturation** (F11 pillar 1): every rule
//!   application produces unique ghost ids; on repetition the
//!   filter kicks in.
//! - **Canonical μ match enumeration** (Def. 4.2 in the paper):
//!   `find_matches` returns a canonical order without infinite
//!   loops.

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
      "documentation": "Bidirectional triple rule: L-node n → R-node r with corr.",
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

/// Builds a cyclic L-pre-graph: A → B → C → A.
fn build_cyclic_graph() -> TypedGraph {
    let mut g = TypedGraph::new();
    let a = g.add_baseline_node("LNode", "A", attrs(&[("name", "A")]));
    let b = g.add_baseline_node("LNode", "B", attrs(&[("name", "B")]));
    let c = g.add_baseline_node("LNode", "C", attrs(&[("name", "C")]));
    g.add_edge(a, b, "next", BTreeMap::new(), Status::Solid);
    g.add_edge(b, c, "next", BTreeMap::new(), Status::Solid);
    g.add_edge(c, a, "next", BTreeMap::new(), Status::Solid); // cycle back to A
    g
}

#[test]
fn case08_cyclic_graph_terminates() {
    // Engine must not loop forever on a cyclic L-topology.
    // With max_steps=20 as a safeguard, we expect terminal
    // convergence/duplication, not StepLimitExceeded.
    let mut g = build_cyclic_graph();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let mut cas = Cascade::new();
    let term = run_cascade(&mut cas, &mut g, &refs, 20).expect("termination without step limit");
    eprintln!(
        "Case 8 termination: {term:?}, entries: {}",
        cas.entries.len()
    );
    assert!(
        matches!(
            term,
            TerminationState::Convergence | TerminationState::Duplication
        ),
        "engine terminates deterministically on cyclic topology"
    );
}

#[test]
fn case08_each_l_node_produces_one_r_node() {
    let mut g = build_cyclic_graph();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let mut cas = Cascade::new();
    let _ = run_cascade(&mut cas, &mut g, &refs, 20).unwrap();

    // Three L-nodes → three R-nodes, three corrs.
    let r_nodes = g.iter_nodes().filter(|n| n.type_id == "RNode").count();
    let corrs = g.iter_nodes().filter(|n| n.type_id == "CorrNode").count();
    assert_eq!(r_nodes, 3, "3 L-nodes → 3 R-nodes");
    assert_eq!(corrs, 3, "3 corrs (one per L-R pair)");
}

#[test]
fn case08_canonical_enumeration_deterministic() {
    // Repeated runs yield a deterministic cascade length.
    // This evidences canonical match enumeration μ.
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
                "determinism: cascade length identical across runs"
            );
        }
    }
}

#[test]
fn case08_self_loop_terminates() {
    // Edge case: self-loop (A → A). Rule should fire once, then
    // terminate.
    let mut g = TypedGraph::new();
    let a = g.add_baseline_node("LNode", "A", attrs(&[("name", "A")]));
    g.add_edge(a, a, "next", BTreeMap::new(), Status::Solid);

    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let mut cas = Cascade::new();
    let term = run_cascade(&mut cas, &mut g, &refs, 20).expect("termination");
    eprintln!("Case 8 self-loop termination: {term:?}");
    let r_count = g.iter_nodes().filter(|n| n.type_id == "RNode").count();
    assert_eq!(r_count, 1, "self-loop: 1 R-node");
}
