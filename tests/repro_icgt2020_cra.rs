//! Case 6 — ICGT2020 CRA empty-class fixpoint (Kosiol et al. 2020).
//!
//! Pathologie (Paper §2): Mutation-Regeln auf Class/Feature-Modell
//! akkumulieren Violations gegen den c₂-Constraint („jede Class
//! mindestens ein Feature"). Naive Count-Based-Repair läuft ins
//! triviale Fixpunkt: alles tombstone → 0 Violations.
//!
//! Seesaw's Mechanismus: **Tombstone(Class) und Solid(Class) mit
//! tombstoned-Features sind unterscheidbare Zustände**. Eine leere
//! Class ist *strukturell anders* als eine gelöschte Class — der
//! Engine-Status drückt das aus.

#[path = "fixtures/cra_class_feature_mm.rs"]
mod cra_class_feature_mm;

use cra_class_feature_mm::{build_pre_graph, CraSnapshot};
use seesaw_tgg::graph::{GhostId, Status, TypedGraph};
use seesaw_tgg::ops::{DeltaEntry, Op};

fn delete_both_features() -> (TypedGraph, CraSnapshot) {
    let (mut graph, snap) = build_pre_graph();
    // User-Delta: lösche beide Features (User-getriebene "moveFeature
    // raus aus C, ohne neues Ziel" — vereinfacht als reines DelNode).
    let user = DeltaEntry::new_user(
        vec![
            Op::DelNode {
                target: snap.ids["F1"],
            },
            Op::DelNode {
                target: snap.ids["F2"],
            },
        ],
        vec![snap.ids["C"], snap.ids["F1"], snap.ids["F2"]],
    );
    user.apply(&mut graph).expect("user-delta applies");
    (graph, snap)
}

#[test]
fn case06_empty_class_is_solid_features_tombstone() {
    let (graph, snap) = delete_both_features();
    let c = graph.get_node(&snap.ids["C"]).unwrap();
    let f1 = graph.get_node(&snap.ids["F1"]).unwrap();
    let f2 = graph.get_node(&snap.ids["F2"]).unwrap();
    assert_eq!(c.status, Status::Solid, "C bleibt Solid (nicht gelöscht)");
    assert_eq!(f1.status, Status::Tombstone);
    assert_eq!(f2.status, Status::Tombstone);
}

#[test]
fn case06_empty_state_is_distinguishable_from_deleted() {
    // Variante A: leere Class (Features Tombstone, C Solid)
    let (graph_empty, snap_empty) = delete_both_features();

    // Variante B: gelöschte Class (C Tombstone, Features Tombstone via Cascade)
    let (mut graph_deleted, snap_deleted) = build_pre_graph();
    let user = DeltaEntry::new_user(
        vec![
            Op::DelNode {
                target: snap_deleted.ids["F1"],
            },
            Op::DelNode {
                target: snap_deleted.ids["F2"],
            },
            Op::DelNode {
                target: snap_deleted.ids["C"],
            },
        ],
        vec![snap_deleted.ids["C"]],
    );
    user.apply(&mut graph_deleted).unwrap();

    // Status-Inventar unterscheidet sich klar
    assert_eq!(
        graph_empty.get_node(&snap_empty.ids["C"]).unwrap().status,
        Status::Solid,
        "leere Class: Solid"
    );
    assert_eq!(
        graph_deleted
            .get_node(&snap_deleted.ids["C"])
            .unwrap()
            .status,
        Status::Tombstone,
        "gelöschte Class: Tombstone"
    );
}

#[test]
fn case06_violation_count_signal_via_status_inventar() {
    // Eine externe Repair-Logik kann via Status-Inventar c₂-Violations
    // zählen, ohne ad-hoc Konflikt-Detection: für jede Solid Class,
    // wie viele aktive Features hat sie?
    let (graph, snap) = delete_both_features();
    let c = snap.ids["C"];

    let active_features: Vec<GhostId> = graph
        .iter_edges()
        .into_iter()
        .filter(|(s, _, e)| *s == c && e.type_id == "contains" && e.status != Status::Tombstone)
        .map(|(_, t, _)| t)
        .filter(|t| {
            graph
                .get_node(t)
                .map(|n| n.status != Status::Tombstone)
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(
        active_features.len(),
        0,
        "C hat 0 aktive Features → c₂-Violation"
    );
}

#[test]
fn case06_gamma_class_id_stable_under_feature_purge() {
    // γ-Beleg: C's GhostId bleibt stabil, auch wenn alle Features
    // tombstoned werden. Die strukturelle Identität der Class
    // hängt nicht von ihrer Feature-Population ab.
    let (graph_pre, snap) = build_pre_graph();
    let c_id_pre = graph_pre.get_node(&snap.ids["C"]).unwrap().id;

    let (graph_post, _) = delete_both_features();
    let c_id_post = graph_post.get_node(&snap.ids["C"]).unwrap().id;
    assert_eq!(c_id_pre, c_id_post, "γ: C's GhostId stabil");
}
