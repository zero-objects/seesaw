//! Reproduktion der emf_adapter `RuleEngineTest.cascadeStepByStepCountsDeltas`-
//! Failure auf reiner Rust-Seite (ohne JNI-Bridge). Wenn dieser Test
//! lokal failt → Bug ist in seesaw_tgg (Rule-Compile/Matching).
//! Wenn er passes → Bug sitzt im JNI-Glue (apply_add_node, Session-
//! Init etc.).
//!
//! Java-Test-Sequenz die reproduziert wird:
//!
//! ```text
//! submitDelta({
//!   AddNode m1: Model parent=root edge=contains attrs={name:"Demo"}
//!   AddNode c1: Class parent=m1   edge=classes  attrs={name:"Widget"}
//!   AddNode a1: Attribute parent=c1 edge=attributes attrs={name:"label", type:"String"}
//! })
//! registerRule("R_Class")
//! registerRule("R_Attr")
//! cascadeStep() → expect state=Running, cascadeLength=1
//! ```

use std::collections::BTreeMap;

use seesaw_tgg::engine::{cascade_step, Cascade, Rule, TerminationState};
use seesaw_tgg::graph::{Status, TypedGraph};
use seesaw_tgg::rule::demo::demo_rule_instantiated;

fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Konstruiert den Graph aus dem Java-Test als reine Solid-Baseline:
/// root (Unknown) ── contains ──▶ m1 (Model) ── classes ──▶ c1 (Class)
///                                                ── attributes ──▶ a1 (Attribute)
fn build_baseline_graph() -> TypedGraph {
    let mut g = TypedGraph::new();
    let root = g.add_baseline_node("Unknown", "root", BTreeMap::new());
    let m1 = g.add_baseline_node("Model", "mModel", attrs(&[("name", "Demo")]));
    let c1 = g.add_baseline_node("Class", "cWidget", attrs(&[("name", "Widget")]));
    let a1 = g.add_baseline_node(
        "Attribute",
        "aLabel",
        attrs(&[("name", "label"), ("type", "String")]),
    );

    g.add_edge(root, m1, "contains", BTreeMap::new(), Status::Solid)
        .expect("contains edge");
    g.add_edge(m1, c1, "classes", BTreeMap::new(), Status::Solid)
        .expect("classes edge");
    g.add_edge(c1, a1, "attributes", BTreeMap::new(), Status::Solid)
        .expect("attributes edge");

    g
}

#[test]
fn demo_r_class_should_match_baseline_graph() {
    let mut g = build_baseline_graph();
    let mut cascade = Cascade::new();

    let r_class = demo_rule_instantiated("R_Class").expect("R_Class instantiated");
    let rules: Vec<&dyn Rule> = vec![r_class.as_ref()];

    println!(
        "graph before: {} nodes, {} edges",
        g.node_count(),
        g.edge_count()
    );

    let state = cascade_step(&mut cascade, &mut g, &rules).expect("cascade_step succeeds");

    println!("state: {:?}, cascadeLength: {}", state, cascade.len());
    println!(
        "graph after: {} nodes, {} edges",
        g.node_count(),
        g.edge_count()
    );

    assert_eq!(
        state,
        TerminationState::Running,
        "R_Class sollte auf Model→Class matchen; state={:?}, cascadeLength={}",
        state,
        cascade.len()
    );
    assert_eq!(cascade.len(), 1, "Genau 1 Schritt sollte angewandt sein");
}

/// Bug B (siehe open-points.md): R_Getter/R_Setter feuern nicht,
/// weil `is_duplicate` Context-Ops fälschlich als Duplikat wertet.
/// Test ist aktuell `#[ignore]` — wird mit Bug-B-Fix re-aktiviert.
#[test]
fn demo_all_four_rules_should_produce_getter_and_setter() {
    use seesaw_tgg::engine::run_cascade;
    let mut g = build_baseline_graph();
    let mut cascade = Cascade::new();

    let rules_owned: Vec<Box<dyn Rule>> = ["R_Class", "R_Attr", "R_Getter", "R_Setter"]
        .iter()
        .map(|n| demo_rule_instantiated(n).expect(n))
        .collect();
    let rules: Vec<&dyn Rule> = rules_owned.iter().map(|b| b.as_ref()).collect();

    let result = run_cascade(&mut cascade, &mut g, &rules, 20).expect("run_cascade");
    println!(
        "state={:?}, cascadeLength={}, nodes={}, edges={}",
        result,
        cascade.len(),
        g.node_count(),
        g.edge_count()
    );

    let kinds: std::collections::HashSet<String> =
        g.iter_nodes().map(|n| n.type_id.clone()).collect();
    println!("kinds={kinds:?}");

    assert!(kinds.contains("Getter"), "Getter erwartet");
    assert!(kinds.contains("Setter"), "Setter erwartet");
}

#[test]
fn demo_r_class_then_r_attr_should_chain() {
    let mut g = build_baseline_graph();
    let mut cascade = Cascade::new();

    let r_class = demo_rule_instantiated("R_Class").expect("R_Class");
    let r_attr = demo_rule_instantiated("R_Attr").expect("R_Attr");
    let rules: Vec<&dyn Rule> = vec![r_class.as_ref(), r_attr.as_ref()];

    // Schritt 1: R_Class (rank 40)
    let s1 = cascade_step(&mut cascade, &mut g, &rules).expect("cascade_step 1");
    assert_eq!(s1, TerminationState::Running, "Step 1 state");
    assert_eq!(cascade.len(), 1, "Step 1 cascadeLength");

    // Schritt 2: R_Attr (rank 30, nach R_Class)
    let s2 = cascade_step(&mut cascade, &mut g, &rules).expect("cascade_step 2");
    assert_eq!(s2, TerminationState::Running, "Step 2 state");
    assert_eq!(cascade.len(), 2, "Step 2 cascadeLength");
}
