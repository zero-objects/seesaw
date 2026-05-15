//! Edge-Case-Tests für den `kind_index` (F15-Mitigation,
//! Match-Indexing). Stellen sicher, dass `matchable_nodes_by_kind`
//! korrekt mit Status-Übergängen, Resurrection und leerem Inventar
//! umgeht — und dass die Reihenfolge deterministisch (BTreeSet-Ord)
//! ist, was die Canonical-μ-Match-Enumeration aus F11 fundiert.

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
    assert_eq!(persons.len(), 2, "2 Persons im Index");
    let cars: Vec<&str> = g
        .matchable_nodes_by_kind("Car")
        .map(|n| n.attrs.get("name").unwrap().as_str())
        .collect();
    assert_eq!(cars.len(), 1, "1 Car im Index");
}

#[test]
fn index_returns_empty_for_unknown_kind() {
    let mut g = TypedGraph::new();
    g.add_baseline_node("Person", "alice", BTreeMap::new());

    let count = g.matchable_nodes_by_kind("UnknownType").count();
    assert_eq!(count, 0, "kind ohne Knoten liefert leeren Iterator");
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
        "Tombstone wird vom Lookup-Status-Filter ausgeschlossen"
    );
}

#[test]
fn index_includes_tentative_tombstone_for_resurrection() {
    // M5/F13: TentativeTombstone bleibt matchbar, damit Resurrection
    // funktioniert (eine neue Rule-Anwendung mit identischer Ghost-ID
    // kann den Knoten wiederbeleben).
    let mut g = TypedGraph::new();
    let p = g.add_baseline_node("Person", "alice", BTreeMap::new());

    g.set_node_status(&p, Status::TentativeTombstone);

    let count = g.matchable_nodes_by_kind("Person").count();
    assert_eq!(
        count, 1,
        "TentativeTombstone bleibt im Match-Index sichtbar (F13)"
    );
}

#[test]
fn index_deterministic_iteration_order_via_btreeset() {
    // Der Index nutzt BTreeSet → Iteration sortiert nach
    // GhostId-Lex-Order. Zwei Graphen mit identischen Insertions
    // müssen identische Match-Reihenfolge produzieren.
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
        "deterministische Reihenfolge bei identischer Insertion"
    );
}

#[test]
fn index_iteration_order_independent_of_insertion_order() {
    // Stärkerer Determinismus-Test: zwei Graphen mit unterschiedlicher
    // Insertion-Reihenfolge müssen ebenfalls identische Match-
    // Reihenfolge liefern, weil BTreeSet by GhostId sortiert (nicht
    // by Insertion-Order). Das ist der Punkt von canonical-μ.
    let mut g1 = TypedGraph::new();
    let mut g2 = TypedGraph::new();
    g1.add_baseline_node("Person", "alice", BTreeMap::new());
    g1.add_baseline_node("Person", "bob", BTreeMap::new());
    g1.add_baseline_node("Person", "carol", BTreeMap::new());
    // Andere Insertion-Reihenfolge in g2
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
        "BTreeSet-Reihenfolge ist insertion-unabhängig (canonical μ)"
    );
}

#[test]
fn index_handles_repeated_insertion_idempotently() {
    // Mehrfache Inserts mit gleichem Ghost-ID-Hash sollen den Index
    // nicht aufblähen (insert_node hat early-return für bekannte IDs).
    let mut g = TypedGraph::new();
    let id_a = g.add_baseline_node("Person", "alice", attrs(&[("name", "alice")]));
    let id_b = g.add_baseline_node("Person", "alice", attrs(&[("name", "alice")]));
    assert_eq!(id_a, id_b, "identische Baseline-Namen → identische ID");
    assert_eq!(
        g.matchable_nodes_by_kind("Person").count(),
        1,
        "doppelte Insertion mit gleicher ID erzeugt nur einen Index-Eintrag"
    );
}

#[test]
fn index_consistent_with_matchable_nodes_total() {
    // Sanity: Summe der Per-Kind-Counts == Total-Matchable-Count.
    // Wenn das nicht passt, verliert der Index Knoten oder
    // dupliziert sie.
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
        "Index-Sum gleicht Iter-Total"
    );
}

#[test]
fn index_after_status_round_trip_solid_to_tombstone_to_solid_via_resurrection() {
    // Resurrection-Round-Trip: Solid → TentativeTombstone → Tombstone
    // → Solid (via insert_node mit gleicher ID auf TT). Der Index
    // muss in jedem Schritt korrekt sein.
    let mut g = TypedGraph::new();
    let p = g.add_baseline_node("Person", "alice", attrs(&[("name", "alice")]));
    assert_eq!(
        g.matchable_nodes_by_kind("Person").count(),
        1,
        "Solid: matchbar"
    );

    g.set_node_status(&p, Status::TentativeTombstone);
    assert_eq!(
        g.matchable_nodes_by_kind("Person").count(),
        1,
        "TT: weiterhin matchbar"
    );

    g.set_node_status(&p, Status::Tombstone);
    assert_eq!(
        g.matchable_nodes_by_kind("Person").count(),
        0,
        "Tombstone: nicht matchbar"
    );

    // Resurrection durch erneutes insert mit identischer Baseline-ID
    // funktioniert nur via TentativeTombstone-Pfad in insert_node;
    // direkter Tombstone → Resurrection ist nicht im Insert-Pfad
    // implementiert. Wir testen: TT → Solid via insert.
    g.set_node_status(&p, Status::TentativeTombstone);
    g.add_baseline_node("Person", "alice", attrs(&[("name", "alice")]));
    let resurrected = g.get_node(&p).unwrap();
    assert_eq!(
        resurrected.status,
        Status::Solid,
        "Resurrection setzt Solid"
    );
    assert_eq!(
        g.matchable_nodes_by_kind("Person").count(),
        1,
        "nach Resurrection wieder matchbar"
    );
}
