//! Case 6 — ICGT2020 CRA empty-class fixpoint (Kosiol et al. 2020).
//!
//! Pathology (paper §2): mutation rules on a Class/Feature model
//! accumulate violations of the c₂ constraint ("every Class has at
//! least one Feature"). Naive count-based repair drifts into the
//! trivial fixpoint: tombstone everything → 0 violations.
//!
//! Seesaw's mechanism: **Tombstone(Class) and Solid(Class) with
//! tombstoned features are distinguishable states**. An empty
//! Class is *structurally different* from a deleted Class — the
//! engine status expresses that.

#[path = "fixtures/cra_class_feature_mm.rs"]
mod cra_class_feature_mm;

use cra_class_feature_mm::{build_pre_graph, CraSnapshot};
use seesaw_tgg::graph::{GhostId, Status, TypedGraph};
use seesaw_tgg::ops::{DeltaEntry, Op};

fn delete_both_features() -> (TypedGraph, CraSnapshot) {
    let (mut graph, snap) = build_pre_graph();
    // User delta: delete both features (user-driven "moveFeature
    // out of C without a new target" — simplified as plain DelNode).
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
    assert_eq!(c.status, Status::Solid, "C stays Solid (not deleted)");
    assert_eq!(f1.status, Status::Tombstone);
    assert_eq!(f2.status, Status::Tombstone);
}

#[test]
fn case06_empty_state_is_distinguishable_from_deleted() {
    // Variant A: empty Class (features Tombstone, C Solid)
    let (graph_empty, snap_empty) = delete_both_features();

    // Variant B: deleted Class (C Tombstone, features Tombstone via cascade)
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

    // Status inventory differs clearly
    assert_eq!(
        graph_empty.get_node(&snap_empty.ids["C"]).unwrap().status,
        Status::Solid,
        "empty Class: Solid"
    );
    assert_eq!(
        graph_deleted
            .get_node(&snap_deleted.ids["C"])
            .unwrap()
            .status,
        Status::Tombstone,
        "deleted Class: Tombstone"
    );
}

#[test]
fn case06_violation_count_signal_via_status_inventar() {
    // External repair logic can count c₂ violations via the status
    // inventory, without ad-hoc conflict detection: for every Solid
    // Class, how many active features does it have?
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
        "C has 0 active features → c₂ violation"
    );
}

#[test]
fn case06_gamma_class_id_stable_under_feature_purge() {
    // γ evidence: C's GhostId stays stable, even when all features
    // are tombstoned. The structural identity of the Class does
    // not depend on its feature population.
    let (graph_pre, snap) = build_pre_graph();
    let c_id_pre = graph_pre.get_node(&snap.ids["C"]).unwrap().id;

    let (graph_post, _) = delete_both_features();
    let c_id_post = graph_post.get_node(&snap.ids["C"]).unwrap().id;
    assert_eq!(c_id_pre, c_id_post, "γ: C's GhostId stable");
}
