//! Edge-case tests for the `kind_index` (F15 mitigation,
//! match indexing). Ensure that `matchable_nodes_by_kind`
//! handles status transitions, resurrection, and empty inventory
//! correctly — and that the order is deterministic (BTreeSet-ord),
//! which grounds the canonical-μ match enumeration from F11.

use std::collections::BTreeMap;

use seesaw_tgg::graph::{GhostId, Status, TypedGraph};

fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn index_returns_only_requested_kind() {
    let mut g = TypedGraph::new();
    g.add_baseline_node("Person", "alice", attrs(&[("name", "alice")]));
    g.add_baseline_node("Person", "bob", attrs(&[("name", "bob")]));
    g.add_baseline_node("Car", "vehicle1", attrs(&[("name", "v1")]));

    let persons: Vec<&str> = g
        .matchable_nodes_by_kind("Person")
        .map(|n| n.attrs.get("name").unwrap().as_str())
        .collect();
    assert_eq!(persons.len(), 2, "2 Persons in the index");
    let cars: Vec<&str> = g
        .matchable_nodes_by_kind("Car")
        .map(|n| n.attrs.get("name").unwrap().as_str())
        .collect();
    assert_eq!(cars.len(), 1, "1 Car in the index");
}

#[test]
fn index_returns_empty_for_unknown_kind() {
    let mut g = TypedGraph::new();
    g.add_baseline_node("Person", "alice", BTreeMap::new());

    let count = g.matchable_nodes_by_kind("UnknownType").count();
    assert_eq!(count, 0, "kind without nodes returns empty iterator");
}

#[test]
fn index_excludes_tombstone() {
    let mut g = TypedGraph::new();
    let p = g.add_baseline_node("Person", "alice", BTreeMap::new());

    assert_eq!(g.matchable_nodes_by_kind("Person").count(), 1);

    g.set_node_status(&p, Status::Tombstone);

    assert_eq!(
        g.matchable_nodes_by_kind("Person").count(),
        0,
        "tombstone is excluded by the lookup status filter"
    );
}

#[test]
fn index_includes_tentative_tombstone_for_resurrection() {
    // M5/F13: TentativeTombstone stays matchable so resurrection
    // works (a new rule application with an identical ghost id
    // can revive the node).
    let mut g = TypedGraph::new();
    let p = g.add_baseline_node("Person", "alice", BTreeMap::new());

    g.set_node_status(&p, Status::TentativeTombstone);

    let count = g.matchable_nodes_by_kind("Person").count();
    assert_eq!(
        count, 1,
        "TentativeTombstone stays visible in the match index (F13)"
    );
}

#[test]
fn index_deterministic_iteration_order_via_btreeset() {
    // The index uses BTreeSet, so iteration is sorted by
    // GhostId lex order. Two graphs with identical insertions
    // must produce identical match order.
    let mut g1 = TypedGraph::new();
    let mut g2 = TypedGraph::new();
    for n in ["alice", "bob", "carol", "dave"] {
        g1.add_baseline_node("Person", n, attrs(&[("name", n)]));
        g2.add_baseline_node("Person", n, attrs(&[("name", n)]));
    }
    let order1: Vec<&GhostId> = g1
        .matchable_nodes_by_kind("Person")
        .map(|n| &n.id)
        .collect();
    let order2: Vec<&GhostId> = g2
        .matchable_nodes_by_kind("Person")
        .map(|n| &n.id)
        .collect();
    assert_eq!(
        order1, order2,
        "deterministic order under identical insertion"
    );
}

#[test]
fn index_iteration_order_independent_of_insertion_order() {
    // Stronger determinism test: two graphs with different
    // insertion order must also yield identical match order,
    // because BTreeSet sorts by GhostId (not by insertion order).
    // That is the point of canonical-μ.
    let mut g1 = TypedGraph::new();
    let mut g2 = TypedGraph::new();
    g1.add_baseline_node("Person", "alice", BTreeMap::new());
    g1.add_baseline_node("Person", "bob", BTreeMap::new());
    g1.add_baseline_node("Person", "carol", BTreeMap::new());
    // Different insertion order in g2
    g2.add_baseline_node("Person", "carol", BTreeMap::new());
    g2.add_baseline_node("Person", "alice", BTreeMap::new());
    g2.add_baseline_node("Person", "bob", BTreeMap::new());
    let order1: Vec<&GhostId> = g1
        .matchable_nodes_by_kind("Person")
        .map(|n| &n.id)
        .collect();
    let order2: Vec<&GhostId> = g2
        .matchable_nodes_by_kind("Person")
        .map(|n| &n.id)
        .collect();
    assert_eq!(
        order1, order2,
        "BTreeSet order is insertion-independent (canonical μ)"
    );
}

#[test]
fn index_handles_repeated_insertion_idempotently() {
    // Repeated inserts with the same ghost-id hash must not bloat
    // the index (insert_node early-returns for known ids).
    let mut g = TypedGraph::new();
    let id_a = g.add_baseline_node("Person", "alice", attrs(&[("name", "alice")]));
    let id_b = g.add_baseline_node("Person", "alice", attrs(&[("name", "alice")]));
    assert_eq!(id_a, id_b, "identical baseline names yield identical id");
    assert_eq!(
        g.matchable_nodes_by_kind("Person").count(),
        1,
        "duplicate insertion with the same id produces only one index entry"
    );
}

#[test]
fn index_consistent_with_matchable_nodes_total() {
    // Sanity: sum of per-kind counts == total matchable count.
    // If this does not hold, the index loses nodes or
    // duplicates them.
    let mut g = TypedGraph::new();
    g.add_baseline_node("Person", "alice", BTreeMap::new());
    g.add_baseline_node("Person", "bob", BTreeMap::new());
    g.add_baseline_node("Car", "v1", BTreeMap::new());
    g.add_baseline_node("Car", "v2", BTreeMap::new());
    g.add_baseline_node("House", "h1", BTreeMap::new());

    let total_via_iter = g.matchable_nodes().count();
    let total_via_index: usize = ["Person", "Car", "House"]
        .iter()
        .map(|k| g.matchable_nodes_by_kind(k).count())
        .sum();
    assert_eq!(
        total_via_iter, total_via_index,
        "index sum equals iter total"
    );
}

#[test]
fn index_after_status_round_trip_solid_to_tombstone_to_solid_via_resurrection() {
    // Resurrection round-trip: Solid → TentativeTombstone → Tombstone
    // → Solid (via insert_node with the same id on TT). The index
    // must stay correct at every step.
    let mut g = TypedGraph::new();
    let p = g.add_baseline_node("Person", "alice", attrs(&[("name", "alice")]));
    assert_eq!(
        g.matchable_nodes_by_kind("Person").count(),
        1,
        "Solid: matchable"
    );

    g.set_node_status(&p, Status::TentativeTombstone);
    assert_eq!(
        g.matchable_nodes_by_kind("Person").count(),
        1,
        "TT: still matchable"
    );

    g.set_node_status(&p, Status::Tombstone);
    assert_eq!(
        g.matchable_nodes_by_kind("Person").count(),
        0,
        "Tombstone: not matchable"
    );

    // Resurrection via re-insert with an identical baseline id
    // only works through the TentativeTombstone path in insert_node;
    // direct Tombstone → resurrection is not implemented in the
    // insert path. We test: TT → Solid via insert.
    g.set_node_status(&p, Status::TentativeTombstone);
    g.add_baseline_node("Person", "alice", attrs(&[("name", "alice")]));
    let resurrected = g.get_node(&p).unwrap();
    assert_eq!(resurrected.status, Status::Solid, "resurrection sets Solid");
    assert_eq!(
        g.matchable_nodes_by_kind("Person").count(),
        1,
        "matchable again after resurrection"
    );
}
