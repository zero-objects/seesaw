//! Diagnostik-Test: welche Graph-Elemente produziert die Engine nach
//! Root-Rule-Anwendung? Bestätigt die corrL/corrR-Topology-Konvention
//! aus `instantiate.rs` und hilft Sub-Rule/Leaf-Rule-Pattern richtig
//! zu formulieren.

use seesaw_tgg::engine::{run_cascade, Cascade, Rule};
use seesaw_tgg::graph::{Status, TypedGraph};
use seesaw_tgg::rule::spec::parse_ruleset;
use seesaw_tgg::rule::{compile, instantiate};
use std::collections::BTreeMap;

const FIXTURE: &str = include_str!("fixtures/rules_fase2019_3rule.json");

fn load_rules() -> Vec<Box<dyn Rule>> {
    let rs = parse_ruleset(FIXTURE).unwrap();
    rs.rules
        .iter()
        .map(|r| instantiate(&compile(r).unwrap()))
        .collect()
}

#[test]
fn probe_root_rule_output_topology() {
    let mut g = TypedGraph::new();
    let _root = g.add_baseline_node(
        "Package",
        "rootP",
        [("name".to_string(), "rootP".to_string())]
            .iter()
            .cloned()
            .collect::<BTreeMap<_, _>>(),
    );

    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();

    let mut cascade = Cascade::new();
    let term = run_cascade(&mut cascade, &mut g, &refs, 10).unwrap();
    eprintln!("Termination: {term:?}");
    eprintln!("Cascade entries: {}", cascade.entries.len());
    for (i, e) in cascade.entries.iter().enumerate() {
        eprintln!("  step {i}: origin={:?}, ops={}", e.origin, e.op_star.len());
    }

    eprintln!("\n== Nodes ==");
    for n in g.iter_nodes() {
        eprintln!(
            "  id={} type={} status={:?} attrs={:?}",
            n.id.short(),
            n.type_id,
            n.status,
            n.attrs
        );
    }
    eprintln!("\n== Edges ==");
    for (src, tgt, e) in g.iter_edges() {
        eprintln!(
            "  {} --{}--> {}  status={:?}",
            src.short(),
            e.type_id,
            tgt.short(),
            e.status
        );
    }

    // Basis-Erwartung: Root-Rule feuert mindestens 1×.
    assert!(
        !cascade.entries.is_empty(),
        "Root-Rule sollte auf rootP matchen"
    );
    // Nach Root-Rule: ein CorrPackage-Node und ein Folder-Node im Graph.
    let has_corr = g.iter_nodes().any(|n| n.type_id == "CorrPackage");
    let has_folder = g.iter_nodes().any(|n| n.type_id == "Folder");
    assert!(has_corr, "CorrPackage-Node existiert nach Root-Rule");
    assert!(has_folder, "Folder-Node existiert nach Root-Rule");
    // Kanten-Typen
    let edge_types: std::collections::HashSet<String> = g
        .iter_edges()
        .iter()
        .map(|(_, _, e)| e.type_id.clone())
        .collect();
    eprintln!("\nEdge-Typen: {edge_types:?}");
    assert!(edge_types.contains("corrL"), "corrL existiert");
    assert!(edge_types.contains("corrR"), "corrR existiert");

    // Kein Delta-Solid vs Ghost-Test hier, nur Existenz.
    let _ = Status::Solid; // suppress unused warning
}
