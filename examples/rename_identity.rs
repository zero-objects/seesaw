//! Rename identity demo: A8 in action.
//!
//! Create a UML `Class("Foo")` on the L side, run the cascade so the
//! engine derives `JavaClass("Foo")` plus the `CorrClass` anchor on the
//! R side. Then rename the L-side `Class` to `"Bar"` and run the cascade
//! again. Expectation:
//!
//! - The `JavaClass` count stays at **1** (no duplicate).
//! - Its name updates to `"Bar"` via propagation.
//! - Its `GhostId` is **identical** to the one before the rename —
//!   the id is parent-rooted and structural, not derived from the
//!   propagated `name`.
//!
//! Run with: `cargo run --example rename_identity`

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

fn count_nodes(g: &TypedGraph, kind: &str) -> usize {
    g.iter_nodes().filter(|n| n.type_id == kind).count()
}

fn name_of_only(g: &TypedGraph, kind: &str) -> String {
    let nodes: Vec<_> = g.iter_nodes().filter(|n| n.type_id == kind).collect();
    assert_eq!(
        nodes.len(),
        1,
        "expected exactly one {kind} node, found {}",
        nodes.len()
    );
    nodes[0].attrs.get("name").cloned().unwrap_or_default()
}

fn id_of_only(g: &TypedGraph, kind: &str) -> seesaw_tgg::graph::GhostId {
    let nodes: Vec<_> = g.iter_nodes().filter(|n| n.type_id == kind).collect();
    assert_eq!(
        nodes.len(),
        1,
        "expected exactly one {kind} node, found {}",
        nodes.len()
    );
    nodes[0].id
}

fn main() {
    // ── Initial UML: Model("Demo") --classes--> Class("Foo")
    let mut g = TypedGraph::new();
    let root = g.add_baseline_node("Unknown", "root", BTreeMap::new());
    let model = g.add_baseline_node("Model", "mDemo", attrs(&[("name", "Demo")]));
    let class = g.add_baseline_node("Class", "cFoo", attrs(&[("name", "Foo")]));

    g.add_edge(root, model, "contains", BTreeMap::new(), Status::Solid)
        .unwrap();
    g.add_edge(model, class, "classes", BTreeMap::new(), Status::Solid)
        .unwrap();

    let r_class = demo_rule_instantiated("R_Class").expect("R_Class");
    let rules: Vec<&dyn Rule> = vec![r_class.as_ref()];

    // ── Forward cascade: derives JavaClass("Foo") + CorrClass
    let mut cascade = Cascade::new();
    let state1 = cascade_step(&mut cascade, &mut g, &rules).expect("step 1");
    println!("── After forward cascade ──");
    println!("  termination: {state1:?}");
    println!("  JavaClass count: {}", count_nodes(&g, "JavaClass"));
    println!("  JavaClass name : '{}'", name_of_only(&g, "JavaClass"));
    assert_eq!(state1, TerminationState::Running);

    let id_before = id_of_only(&g, "JavaClass");
    println!("  JavaClass id   : {}", id_before.short());
    println!();

    // ── Rename on the L side: Foo -> Bar.
    //    For example simplicity we mutate the graph directly. A real caller
    //    would submit an Op::SetAttr against the cascade; the behaviour is
    //    the same — A8 is a property of the rule's identity calculation,
    //    not of the op delivery.
    g.set_node_attr(&class, "name", "Bar");

    println!("── After rename Foo → Bar on the L side ──");
    println!("  Class name      : '{}'", name_of_only(&g, "Class"));
    println!("  JavaClass count : {}", count_nodes(&g, "JavaClass"));
    println!(
        "  JavaClass name  : '{}'  ← stale; not yet propagated",
        name_of_only(&g, "JavaClass")
    );
    println!();

    // ── Cascade again. R_Class→ re-fires. A8: the JavaClass's id is
    //    parent-rooted + structural, so it stays the same. The AddNode
    //    is a duplicate (no-op); the bound `name` propagates via SetAttr
    //    onto the existing node.
    let mut cascade2 = Cascade::new();
    let state2 = cascade_step(&mut cascade2, &mut g, &rules).expect("step 2");

    println!("── After re-cascade ──");
    println!("  termination     : {state2:?}");
    println!("  JavaClass count : {}", count_nodes(&g, "JavaClass"));
    println!("  JavaClass name  : '{}'", name_of_only(&g, "JavaClass"));

    let id_after = id_of_only(&g, "JavaClass");
    println!("  JavaClass id    : {}", id_after.short());
    println!("  id stable       : {}", id_before == id_after);

    // ── A8 contract checks
    assert_eq!(
        count_nodes(&g, "JavaClass"),
        1,
        "A8: no duplicate JavaClass after rename"
    );
    assert_eq!(
        name_of_only(&g, "JavaClass"),
        "Bar",
        "rename propagates via SetAttr"
    );
    assert_eq!(
        id_before, id_after,
        "GhostId is structural; rename of a bound attr does not move identity"
    );
    println!();
    println!("✓ A8 invariants hold: identity stable, value propagated, no duplicate.");
}
