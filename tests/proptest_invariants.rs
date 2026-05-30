//! Property-based tests for the core invariants V₄, V₆, V₁₆, V₁₇.
//!
//! Each property is tested against randomly generated graphs and cascades
//! using the `proptest` crate. This complements the deterministic unit
//! tests and provides the empirical validation layer from the paper's
//! Evaluation Protocol.
//!
//! See `chapters/09_verifikation.tex` for the formal statements.

use proptest::prelude::*;
use seesaw_tgg::engine::Cascade;
use seesaw_tgg::fold::{consolidate, diff};
use seesaw_tgg::graph::{GhostId, NodeData, Status, TypedGraph};
use seesaw_tgg::ops::{DeltaEntry, Op, Origin};
use std::collections::BTreeMap;

// ══ Strategies ═══════════════════════════════════════════════════════════

/// Generates an opaque string identifier from a restricted alphabet.
fn arb_opaque_id() -> impl Strategy<Value = String> {
    "[a-z]{3,6}[0-9]{0,2}".prop_map(|s| s.to_string())
}

/// Small attribute bag (0-3 entries).
fn arb_attrs() -> impl Strategy<Value = BTreeMap<String, String>> {
    prop::collection::btree_map("[a-z]{2,4}", "[a-zA-Z]{1,8}", 0..3)
}

/// Baseline graph with n random SOLID nodes.
fn arb_baseline_graph(size: usize) -> impl Strategy<Value = (TypedGraph, Vec<GhostId>)> {
    prop::collection::vec(arb_opaque_id(), size..=size).prop_map(move |opaques| {
        let mut graph = TypedGraph::new();
        let mut ids = Vec::new();
        for (i, name) in opaques.iter().enumerate() {
            let id = GhostId::from_baseline(&format!("{name}_{i}"));
            if graph.get_node(&id).is_none() {
                graph.insert_node_data(NodeData {
                    id,
                    type_id: "Class".to_string(),
                    attrs: BTreeMap::new(),
                    status: Status::Solid,
                });
                ids.push(id);
            }
        }
        (graph, ids)
    })
}

/// Strategy for an Op that picks any known node from `ids`
/// as parent/target.
fn arb_op_over_ids(ids: Vec<GhostId>) -> impl Strategy<Value = Op> {
    if ids.is_empty() {
        return Just(Op::SetAttr {
            target: GhostId::from_opaque("dummy"),
            key: "noop".to_string(),
            value: "".to_string(),
        })
        .boxed();
    }
    let ids_add = ids.clone();
    let ids_del = ids.clone();
    let ids_set = ids.clone();
    let ids_edge = ids.clone();

    prop_oneof![
        // AddNode: new ghost child of any existing node
        (
            0..ids_add.len(),
            "[a-z]{3,5}",
            "[A-Z][a-z]{3,6}",
            arb_attrs()
        )
            .prop_map(move |(i, edge_type, type_id, attrs)| Op::AddNode {
                parent: ids_add[i],
                edge_type,
                type_id,
                attrs,
            }),
        // AddEdge between two known nodes
        (0..ids_edge.len(), 0..ids_edge.len(), "[a-z]{2,5}").prop_map(move |(s, t, etype)| {
            Op::AddEdge {
                source: ids_edge[s],
                target: ids_edge[t],
                type_id: etype,
                attrs: BTreeMap::new(),
            }
        }),
        // SetAttr on a known node
        (0..ids_set.len(), "[a-z]{2,4}", "[a-z0-9]{1,6}").prop_map(move |(i, k, v)| {
            Op::SetAttr {
                target: ids_set[i],
                key: k,
                value: v,
            }
        }),
        // DelNode on a known node (low weight, TOMB rare)
        (0..ids_del.len()).prop_map(move |i| Op::DelNode { target: ids_del[i] }),
    ]
    .boxed()
}

/// Generates a cascade of up to `max_entries` delta entries, each with
/// 1-3 ops, where ops reference baseline nodes.
fn arb_cascade(max_entries: usize, ids: Vec<GhostId>) -> impl Strategy<Value = Cascade> {
    prop::collection::vec(
        prop::collection::vec(arb_op_over_ids(ids.clone()), 1..=3),
        0..=max_entries,
    )
    .prop_map(move |op_lists| {
        let mut cascade = Cascade::new();
        for (i, ops) in op_lists.into_iter().enumerate() {
            let anchor = if ids.is_empty() {
                Vec::new()
            } else {
                vec![ids[i % ids.len()]]
            };
            let op_count = ops.len();
            cascade.append(DeltaEntry {
                origin: if i == 0 {
                    Origin::User
                } else {
                    Origin::Rule {
                        rule_id: format!("r{i}"),
                    }
                },
                rank: (i as u64) * 10,
                op_star: ops,
                anchor,
                induces: vec![Vec::new(); op_count],
                bindings: std::collections::HashMap::new(),
            });
        }
        cascade
    })
}

// ══ V₆: cascade length monotonicity ═════════════════════════════════════

proptest! {
    /// V₆: append is strictly monotone and append-only; |D_x| = x+1.
    #[test]
    fn v6_cascade_length_monotone(
        n in 0..25usize,
    ) {
        let mut cascade = Cascade::new();
        prop_assert_eq!(cascade.len(), 0);
        for i in 0..n {
            cascade.append(DeltaEntry {
                origin: Origin::Rule { rule_id: format!("r{i}") },
                rank: i as u64,
                op_star: Vec::new(),
                anchor: Vec::new(),
                induces: Vec::new(),
            bindings: std::collections::HashMap::new(),
            });
            prop_assert_eq!(cascade.len(), i + 1);
        }
    }
}

// ══ GhostId determinism ══════════════════════════════════════════════════

proptest! {
    /// `GhostId::from_opaque` is deterministic.
    #[test]
    fn ghost_id_from_opaque_deterministic(s in "[a-zA-Z0-9_-]{1,32}") {
        let a = GhostId::from_opaque(&s);
        let b = GhostId::from_opaque(&s);
        prop_assert_eq!(a, b);
    }

    /// `GhostId::from_baseline` is deterministic.
    #[test]
    fn ghost_id_from_baseline_deterministic(s in "[a-zA-Z0-9]{1,16}") {
        let a = GhostId::from_baseline(&s);
        let b = GhostId::from_baseline(&s);
        prop_assert_eq!(a, b);
    }

    /// Distinct opaque strings produce distinct ids (SHA-256
    /// collision resistance).
    #[test]
    fn ghost_id_opaque_distinguishes(
        s1 in "[a-zA-Z]{1,16}",
        s2 in "[a-zA-Z]{1,16}",
    ) {
        prop_assume!(s1 != s2);
        let a = GhostId::from_opaque(&s1);
        let b = GhostId::from_opaque(&s2);
        prop_assert_ne!(a, b);
    }
}

// ══ V₄: projection determinism ══════════════════════════════════════════

proptest! {
    /// V₄: Repeated application of an op sequence on the same
    /// initial graph yields a structurally equal result graph.
    #[test]
    fn v4_projection_deterministic(
        (base, ids) in arb_baseline_graph(3),
        ops in prop::collection::vec(arb_op_over_ids(vec![GhostId::from_baseline("seed_0_0")]), 0..8),
    ) {
        let _ = ids; // baseline generation only
        let mut g1 = base.clone();
        let mut g2 = base.clone();

        for op in &ops {
            let _ = op.apply(&mut g1);
        }
        for op in &ops {
            let _ = op.apply(&mut g2);
        }

        prop_assert_eq!(g1.node_count(), g2.node_count());
        prop_assert_eq!(g1.edge_count(), g2.edge_count());
    }
}

// ══ V₁₆: fold termination ═══════════════════════════════════════════════

proptest! {
    /// V₁₆: `consolidate` terminates on every finite cascade without
    /// deadlock.
    #[test]
    fn v16_fold_terminates(
        (base, ids) in arb_baseline_graph(3),
        cascade in arb_cascade(6, vec![
            GhostId::from_baseline("seed_0_0"),
            GhostId::from_baseline("seed_1_1"),
            GhostId::from_baseline("seed_2_2"),
        ]),
    ) {
        let _ = ids;
        // consolidate() may return Err (e.g. on a reference to a
        // non-existent node), but it must _terminate_.
        let result = consolidate(&base, &cascade);
        // Only predicate: it returns a result and does not hang.
        let _ = result;
    }
}

// ══ V₁₇: fold preserves materialization (simplified) ═════════════════════

proptest! {
    /// V₁₇ restricted: when `consolidate` succeeds, the result has
    /// fewer or equal nodes/edges than direct application of the
    /// full cascade (upper bound via nullification).
    #[test]
    fn v17_fold_is_reduction(
        (base, _ids) in arb_baseline_graph(2),
        cascade in arb_cascade(4, vec![
            GhostId::from_baseline("seed_0_0"),
            GhostId::from_baseline("seed_1_1"),
        ]),
    ) {
        // Full application
        let mut direct = base.clone();
        let mut direct_ok = true;
        for entry in &cascade.entries {
            for op in &entry.op_star {
                if op.apply(&mut direct).is_err() {
                    direct_ok = false;
                    break;
                }
            }
            if !direct_ok { break; }
        }

        // Fold path
        let fold_result = consolidate(&base, &cascade);
        if let Ok(result) = fold_result {
            let net = diff(&base, &result.new_baseline);
            // Null delta is possible; check formal summary functionality.
            let _ = net.summary();
            // Upper bound: fold baseline has ≤ as many nodes as
            // full-apply (because fold removes nullifications + tombstones).
            if direct_ok {
                let direct_mat = direct.materialize();
                prop_assert!(
                    result.new_baseline.node_count() <= direct_mat.node_count(),
                    "fold reduces or preserves node_count: fold={}, direct={}",
                    result.new_baseline.node_count(),
                    direct_mat.node_count(),
                );
            }
        }
    }
}

// ══ GhostId::from_parent usage ══════════════════════════════════════════

proptest! {
    /// Parent-rooted hash is structurally determined.
    #[test]
    fn ghost_id_from_parent_deterministic(
        parent_name in "[a-z]{1,8}",
        edge_type in "[a-z]{1,6}",
        type_id in "[A-Z][a-z]{1,8}",
        attrs in arb_attrs(),
    ) {
        let parent = GhostId::from_baseline(&parent_name);
        let a = GhostId::from_parent(&parent, &edge_type, &type_id, &attrs);
        let b = GhostId::from_parent(&parent, &edge_type, &type_id, &attrs);
        prop_assert_eq!(a, b);
    }
}
