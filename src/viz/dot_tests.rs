#![allow(unused_imports)]

use super::*;
use crate::graph::{GhostId, Status, TypedGraph};
use std::collections::BTreeMap;

fn build_test_graph() -> TypedGraph {
    let mut g = TypedGraph::new();
    let a = g.add_baseline_node("Package", "root", BTreeMap::new());
    let b = g.add_baseline_node("Package", "p", BTreeMap::new());
    g.add_edge(a, b, "subPackages", BTreeMap::new(), Status::Solid);
    g
}

#[test]
fn empty_graph_dot_has_header_and_annotation() {
    let opts = DotOpts::default();
    let out = empty_graph_dot("n/a — Mono-Graph-Case", &opts);
    assert!(out.starts_with("digraph "), "starts with digraph keyword");
    assert!(
        out.contains("label = \"n/a — Mono-Graph-Case\""),
        "has annotation as top-label"
    );
    assert!(out.contains("rankdir = \"TB\""), "has rankdir");
    assert!(out.trim_end().ends_with('}'), "closes with }}");
}

#[test]
fn empty_graph_dot_respects_rankdir_lr() {
    let opts = DotOpts {
        rankdir: Rankdir::LR,
        ..Default::default()
    };
    let out = empty_graph_dot("x", &opts);
    assert!(out.contains("rankdir = \"LR\""));
}

#[test]
fn typed_graph_to_dot_contains_nodes() {
    let g = build_test_graph();
    let out = typed_graph_to_dot(&g, &DotOpts::default());
    assert!(out.contains("Package"), "contains node type_id");
    let node_count = out.matches(" [shape=").count();
    assert_eq!(node_count, 2, "two node definitions");
}

#[test]
fn typed_graph_to_dot_contains_edge() {
    let g = build_test_graph();
    let out = typed_graph_to_dot(&g, &DotOpts::default());
    assert!(out.contains("subPackages"), "edge type_id im Output");
    let edge_count = out.matches(" -> ").count();
    assert_eq!(edge_count, 1, "one edge");
}

#[test]
fn typed_graph_to_dot_status_colors() {
    let mut g = TypedGraph::new();
    let a = g.add_baseline_node("A", "a", BTreeMap::new());
    let b = g.add_ghost_node(a, "rel", "B", BTreeMap::new());
    g.set_node_status(&b, Status::Tombstone);
    let out = typed_graph_to_dot(&g, &DotOpts::default());
    assert!(
        out.contains("fillcolor=\"mistyrose\"") || out.contains("color=\"red\""),
        "tombstone gets red/mistyrose styling"
    );
}

#[cfg(feature = "regen_graphs")]
#[test]
fn write_snapshot_triple_writes_three_files() {
    let tmp = std::env::temp_dir().join("seesaw_viz_test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let g = build_test_graph();
    write_snapshot_triple(&tmp, &g, &DotOpts::default()).unwrap();
    assert!(tmp.join("source.dot").exists());
    assert!(tmp.join("target.dot").exists());
    assert!(tmp.join("corr.dot").exists());
    let target_content = std::fs::read_to_string(tmp.join("target.dot")).unwrap();
    assert!(
        target_content.contains("Mono-Graph-Case"),
        "target is empty-annotated"
    );
}

#[test]
fn cascade_trace_dot_has_user_and_rule_steps() {
    use crate::engine::Cascade;
    use crate::ops::{DeltaEntry, Op, Origin};

    let pre = build_test_graph();
    let mut cas = Cascade::new();
    let user = DeltaEntry::new_user(
        vec![Op::DelEdge {
            target: GhostId::from_baseline("dummy-edge"),
        }],
        vec![],
    );
    cas.append(user);
    cas.append(DeltaEntry {
        origin: Origin::Rule {
            rule_id: "Sub-Rule".into(),
        },
        rank: 20,
        op_star: vec![],
        anchor: vec![],
        induces: vec![],
        bindings: std::collections::BTreeMap::new(),
    });
    let out = cascade_trace_to_dot(&pre, &cas);
    assert!(out.contains("step_0"), "step 0 node");
    assert!(out.contains("step_1"), "step 1 node");
    assert!(out.contains("User"), "user origin visible");
    assert!(out.contains("Sub-Rule"), "rule id visible");
    assert!(out.contains("step_0 -> step_1"), "timeline chain");
}
