//! Backward cascade demo: start from a Java-side graph
//! (`Model -- javaClasses --> JavaClass("Foo")`), lower the demo `R_Class`
//! rule with `compile_bidirectional`, register only the backward
//! direction (`R_Class←`), and step the cascade once to see the engine
//! derive the UML-side `Class("Foo")` together with the `CorrClass`
//! anchor.
//!
//! The point: the *same* declarative `R_Class` rule serves both
//! directions. The engine picks the direction by which side already
//! exists in the graph; backward propagation is not a separate rule
//! set.
//!
//! Run with: `cargo run --example backward_cascade`

use std::collections::BTreeMap;

use seesaw_tgg::engine::{cascade_step, Cascade, Rule, TerminationState};
use seesaw_tgg::graph::{Status, TypedGraph};
use seesaw_tgg::rule::compile::compile_bidirectional;
use seesaw_tgg::rule::demo::demo_ruleset_spec;
use seesaw_tgg::rule::instantiate::instantiate;

fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn main() {
    // ── Java side only: Model("Demo") --javaClasses--> JavaClass("Foo")
    let mut g = TypedGraph::new();
    let root = g.add_baseline_node("Unknown", "root", BTreeMap::new());
    let model = g.add_baseline_node("Model", "mDemo", attrs(&[("name", "Demo")]));
    let java_class = g.add_baseline_node("JavaClass", "jcFoo", attrs(&[("name", "Foo")]));

    g.add_edge(root, model, "contains", BTreeMap::new(), Status::Solid)
        .unwrap();
    g.add_edge(
        model,
        java_class,
        "javaClasses",
        BTreeMap::new(),
        Status::Solid,
    )
    .unwrap();

    println!("── Initial Java-side graph ──");
    println!("  nodes: {}, edges: {}", g.node_count(), g.edge_count());
    println!();

    // ── Lower R_Class to both directions; instantiate only the backward
    //    direction so the example output stays focused on R_Class←.
    let spec = demo_ruleset_spec();
    let r_class_spec = spec
        .rules
        .iter()
        .find(|r| r.name == "R_Class")
        .expect("R_Class is in the demo set");

    let directed = compile_bidirectional(r_class_spec).expect("compile bidirectional");
    assert_eq!(
        directed.len(),
        2,
        "compile_bidirectional emits one rule per direction"
    );
    println!(
        "── Lowered to {} directed rules: {} + {}",
        directed.len(),
        directed[0].name,
        directed[1].name,
    );
    println!();

    let r_class_bwd = instantiate(&directed[1]); // index 1 = backward ("R_Class←")
    let rules: Vec<&dyn Rule> = vec![r_class_bwd.as_ref()];

    // ── Step the cascade: R_Class← matches the JavaClass + Model anchor
    //    and creates the UML-side Class + CorrClass.
    let mut cascade = Cascade::new();
    let state = cascade_step(&mut cascade, &mut g, &rules).expect("cascade step");
    println!("── After cascade step ──");
    println!("  termination: {state:?}");
    println!("  cascade length: {}", cascade.len());
    println!("  nodes: {}, edges: {}", g.node_count(), g.edge_count());
    assert_eq!(state, TerminationState::Running);
    println!();

    // ── Show the derived UML side. Sort for deterministic output.
    println!("── Final node kinds (sorted) ──");
    let mut rows: Vec<_> = g
        .iter_nodes()
        .map(|n| {
            (
                n.type_id.clone(),
                format!("{:?}", n.status),
                n.attrs.get("name").cloned().unwrap_or_default(),
            )
        })
        .collect();
    rows.sort();
    for (kind, status, name) in rows {
        if name.is_empty() {
            println!("  {kind} ({status})");
        } else {
            println!("  {kind}('{name}') ({status})");
        }
    }
}
