//! Reproduces the emf_adapter `RuleEngineTest.cascadeStepByStepCountsDeltas`
//! failure on the pure Rust side (without the JNI bridge). If this
//! test fails locally → bug is in seesaw_tgg (rule compile/matching).
//! If it passes → bug sits in the JNI glue (apply_add_node, session
//! init, etc.).
//!
//! Reproduced Java test sequence:
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

/// Constructs the graph from the Java test as a pure Solid baseline:
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
        "R_Class should match on Model→Class; state={:?}, cascadeLength={}",
        state,
        cascade.len()
    );
    assert_eq!(cascade.len(), 1, "exactly 1 step should have been applied");
}

/// Bug B (see open-points.md): R_Getter/R_Setter do not fire
/// because `is_duplicate` wrongly classifies context ops as duplicates.
/// Test is currently `#[ignore]` — will be re-enabled with the Bug B fix.
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

    assert!(kinds.contains("Getter"), "Getter expected");
    assert!(kinds.contains("Setter"), "Setter expected");
}

#[test]
fn demo_r_class_then_r_attr_should_chain() {
    let mut g = build_baseline_graph();
    let mut cascade = Cascade::new();

    let r_class = demo_rule_instantiated("R_Class").expect("R_Class");
    let r_attr = demo_rule_instantiated("R_Attr").expect("R_Attr");
    let rules: Vec<&dyn Rule> = vec![r_class.as_ref(), r_attr.as_ref()];

    // Step 1: R_Class (rank 40)
    let s1 = cascade_step(&mut cascade, &mut g, &rules).expect("cascade_step 1");
    assert_eq!(s1, TerminationState::Running, "Step 1 state");
    assert_eq!(cascade.len(), 1, "Step 1 cascadeLength");

    // Step 2: R_Attr (rank 30, after R_Class)
    let s2 = cascade_step(&mut cascade, &mut g, &rules).expect("cascade_step 2");
    assert_eq!(s2, TerminationState::Running, "Step 2 state");
    assert_eq!(cascade.len(), 2, "Step 2 cascadeLength");
}
