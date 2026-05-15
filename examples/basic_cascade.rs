//! Basic cascade demo: build a small UML-style source graph, register the
//! embedded `R_Class` and `R_Attr` rules, and step the cascade twice to
//! see the engine derive the Java-side correspondence.
//!
//! Run with: `cargo run --example basic_cascade`

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

fn main() {
    // ── Build source graph: root → Model("Demo") → Class("Widget") → Attribute("label")
    let mut g = TypedGraph::new();
    let root = g.add_baseline_node("Unknown", "root", BTreeMap::new());
    let model = g.add_baseline_node("Model", "mDemo", attrs(&[("name", "Demo")]));
    let class = g.add_baseline_node("Class", "cWidget", attrs(&[("name", "Widget")]));
    let attr = g.add_baseline_node(
        "Attribute",
        "aLabel",
        attrs(&[("name", "label"), ("type", "String")]),
    );

    g.add_edge(root, model, "contains", BTreeMap::new(), Status::Solid)
        .unwrap();
    g.add_edge(model, class, "classes", BTreeMap::new(), Status::Solid)
        .unwrap();
    g.add_edge(class, attr, "attributes", BTreeMap::new(), Status::Solid)
        .unwrap();

    println!("── Initial source graph ──");
    println!("  nodes: {}, edges: {}", g.node_count(), g.edge_count());
    println!();

    // ── Load demo rules
    let r_class = demo_rule_instantiated("R_Class").expect("R_Class");
    let r_attr = demo_rule_instantiated("R_Attr").expect("R_Attr");
    let rules: Vec<&dyn Rule> = vec![r_class.as_ref(), r_attr.as_ref()];

    let mut cascade = Cascade::new();

    // ── Step 1: R_Class should fire (Class → JavaClass + CorrClass)
    let state1 = cascade_step(&mut cascade, &mut g, &rules).expect("step 1");
    println!("── After step 1 (R_Class applied) ──");
    println!("  state: {state1:?}");
    println!("  cascade length: {}", cascade.len());
    println!("  nodes: {}, edges: {}", g.node_count(), g.edge_count());
    assert_eq!(state1, TerminationState::Running);

    // ── Step 2: R_Attr should fire (Attribute → JavaField + CorrAttr)
    let state2 = cascade_step(&mut cascade, &mut g, &rules).expect("step 2");
    println!();
    println!("── After step 2 (R_Attr applied) ──");
    println!("  state: {state2:?}");
    println!("  cascade length: {}", cascade.len());
    println!("  nodes: {}, edges: {}", g.node_count(), g.edge_count());
    assert_eq!(state2, TerminationState::Running);

    // ── Show the derived target side
    println!();
    println!("── Final node kinds in graph ──");
    let mut kinds: Vec<_> = g
        .iter_nodes()
        .map(|n| (n.type_id.clone(), format!("{:?}", n.status)))
        .collect();
    kinds.sort();
    for (k, s) in kinds {
        println!("  {k} ({s})");
    }
}
