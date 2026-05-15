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

// ══ Strategien ═══════════════════════════════════════════════════════════

/// Generiert einen opaken String-Identifier aus einem beschränkten Alphabet.
fn arb_opaque_id() -> impl Strategy<Value = String> {
    "[a-z]{3,6}[0-9]{0,2}".prop_map(|s| s.to_string())
}

/// Kleiner Attribute-Bag (0-3 Einträge).
fn arb_attrs() -> impl Strategy<Value = BTreeMap<String, String>> {
    prop::collection::btree_map("[a-z]{2,4}", "[a-zA-Z]{1,8}", 0..3)
}

/// Baseline-Graph mit n zufälligen SOLID-Knoten.
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

/// Strategie für eine Op, die einen beliebigen bekannten Knoten aus `ids`
/// als Parent/Target wählt.
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
        // AddNode: neues Ghost-Kind von einem beliebigen existierenden Knoten
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
        // AddEdge zwischen zwei bekannten Knoten
        (0..ids_edge.len(), 0..ids_edge.len(), "[a-z]{2,5}").prop_map(move |(s, t, etype)| {
            Op::AddEdge {
                source: ids_edge[s],
                target: ids_edge[t],
                type_id: etype,
                attrs: BTreeMap::new(),
            }
        }),
        // SetAttr auf einem bekannten Knoten
        (0..ids_set.len(), "[a-z]{2,4}", "[a-z0-9]{1,6}").prop_map(move |(i, k, v)| {
            Op::SetAttr {
                target: ids_set[i],
                key: k,
                value: v,
            }
        }),
        // DelNode auf einem bekannten Knoten (niedriges Gewicht, TOMB rare)
        (0..ids_del.len()).prop_map(move |i| Op::DelNode { target: ids_del[i] }),
    ]
    .boxed()
}

/// Generiert eine Kaskade aus bis zu `max_entries` Delta-Einträgen mit je
/// 1-3 Ops, wobei Ops auf die Baseline-Knoten referenzieren.
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

// ══ V₆: Kaskaden-Längen-Monotonie ═══════════════════════════════════════

proptest! {
    /// V₆: append ist streng monoton und append-only; |D_x| = x+1.
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

// ══ GhostId-Determinismus ════════════════════════════════════════════════

proptest! {
    /// `GhostId::from_opaque` ist deterministisch.
    #[test]
    fn ghost_id_from_opaque_deterministic(s in "[a-zA-Z0-9_-]{1,32}") {
        let a = GhostId::from_opaque(&s);
        let b = GhostId::from_opaque(&s);
        prop_assert_eq!(a, b);
    }

    /// `GhostId::from_baseline` ist deterministisch.
    #[test]
    fn ghost_id_from_baseline_deterministic(s in "[a-zA-Z0-9]{1,16}") {
        let a = GhostId::from_baseline(&s);
        let b = GhostId::from_baseline(&s);
        prop_assert_eq!(a, b);
    }

    /// Verschiedene Opaque-Strings erzeugen verschiedene IDs (SHA-256
    /// Kollisionsresistenz).
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

// ══ V₄: Projektions-Determinismus ═══════════════════════════════════════

proptest! {
    /// V₄: Wiederholte Anwendung einer Op-Sequenz auf den gleichen
    /// Anfangsgraphen liefert strukturell gleichen Ergebnisgraphen.
    #[test]
    fn v4_projection_deterministic(
        (base, ids) in arb_baseline_graph(3),
        ops in prop::collection::vec(arb_op_over_ids(vec![GhostId::from_baseline("seed_0_0")]), 0..8),
    ) {
        let _ = ids; // Nur zur Baseline-Generierung
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

// ══ V₁₆: Fold-Terminierung ══════════════════════════════════════════════

proptest! {
    /// V₁₆: `consolidate` terminiert auf jeder endlichen Kaskade ohne
    /// Deadlock.
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
        // consolidate() darf Err zurückgeben (z.\ B.\ bei
        // Referenz auf nicht-existenten Knoten), muss aber _terminieren_.
        let result = consolidate(&base, &cascade);
        // Einziges Prädikat: es liefert ein Ergebnis und hängt nicht.
        let _ = result;
    }
}

// ══ V₁₇: Fold erhält Materialisierung (vereinfacht) ══════════════════════

proptest! {
    /// V₁₇-Eingeschränkt: wenn `consolidate` erfolgreich ist, dann hat
    /// das Ergebnis weniger oder gleich viele Knoten/Kanten wie direkte
    /// Anwendung der ganzen Kaskade (Upper-Bound durch Nullifikation).
    #[test]
    fn v17_fold_is_reduction(
        (base, _ids) in arb_baseline_graph(2),
        cascade in arb_cascade(4, vec![
            GhostId::from_baseline("seed_0_0"),
            GhostId::from_baseline("seed_1_1"),
        ]),
    ) {
        // Voll-Anwendung
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

        // fold-Weg
        let fold_result = consolidate(&base, &cascade);
        if let Ok(result) = fold_result {
            let net = diff(&base, &result.new_baseline);
            // Null-Delta ist möglich; Prüfung auf formale Summary-Funktionalität.
            let _ = net.summary();
            // Upper-Bound: fold-Baseline hat ≤ so viele Knoten wie
            // Full-Apply (weil fold Nullifikationen + Tombstones entfernt).
            if direct_ok {
                let direct_mat = direct.materialize();
                prop_assert!(
                    result.new_baseline.node_count() <= direct_mat.node_count(),
                    "fold reduziert oder erhält node_count: fold={}, direct={}",
                    result.new_baseline.node_count(),
                    direct_mat.node_count(),
                );
            }
        }
    }
}

// ══ GhostId::from_parent Verwendung ═════════════════════════════════════

proptest! {
    /// Parent-rooted Hash ist strukturell determiniert.
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
