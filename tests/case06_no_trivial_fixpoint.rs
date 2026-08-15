//! Case 6 — ICGT 2020 (Kosiol, Strüber, Taentzer, Zschaler): das
//! Infinite-Folding-Trivial-Fixpunkt der CRA-Pathologie. FRISCH aus
//! dem Paper-Problem, ohne Vorkenntnisse der ersten Generation (2026-07-17).
//!
//! ── Das Paper-Problem ──────────────────────────────────────────────
//! Constraint c₂: jede Class hat ≥1 Feature. Eine Mutation kann Classes
//! leeren → c₂-Violation. Eine count-basierte Repair-Strategie
//! (Violations minimieren) findet den TRIVIALEN Fixpunkt: „lösche
//! alles" → 0 Violations. Etablierte Tools haben kein eingebautes
//! Gegenargument.
//!
//! ── Die frische These ───────────────────────────────────────────
//! Die Engine ist KEINE Violation-minimierende Repair-Engine — es propagiert
//! nur das, was das Delta sagt, und unterscheidet strukturell zwischen
//! „leer" (Solid, aber verletzt c₂) und „gelöscht" (Tombstone). Der
//! triviale Fixpunkt entsteht gar nicht: eine leer gewordene Struktur
//! wird NICHT automatisch weggeräumt, die Violation bleibt SICHTBAR.
//!
//! Schnitt aus dem Problem: Class→Table, Feature→Column (Kontext ClassCorr).
//! Delta = DelNode der Features. Das Ziel (Table) überlebt leer.

mod common;

use seesaw_tgg::engine::{DeltaDomain, Engine};
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::Status;
use seesaw_tgg::plan::DirectedRule;

type Id = seesaw_tgg::ident::GhostId;

fn class_to_table() -> serde_json::Value {
    serde_json::json!({
            "name": "Class_2_Table", "rank": 20,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Class"},
                    {"name": "l1", "type": "className"}
                ],
                "links": [["l0", "l1"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Table"},
                    {"name": "r1", "type": "tblName"}
                ],
                "links": [["r0", "r1"]]
            },
            "corrs": [
                {"type": "ClassCorr", "left": "l0", "right": "r0", "role": "establishes", "bindings": [{"left": "l1", "right": "r1"}]}
            ]
    })
}

fn feature_to_column() -> serde_json::Value {
    serde_json::json!({
            "name": "Feature_2_Column", "rank": 10,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Class"},
                    {"name": "l1", "type": "Feature"},
                    {"name": "l2", "type": "featName"}
                ],
                "links": [["l0", "l1"], ["l1", "l2"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Table"},
                    {"name": "r1", "type": "Column"},
                    {"name": "r2", "type": "colName"}
                ],
                "links": [["r0", "r1"], ["r1", "r2"]]
            },
            "corrs": [
                {"type": "ClassCorr", "left": "l0", "right": "r0", "role": "references"},
                {"type": "FeatCorr", "left": "l1", "right": "r1", "role": "establishes", "bindings": [{"left": "l2", "right": "r2"}]}
            ]
    })
}

fn ruleset(g: &mut Graph) -> Vec<DirectedRule> {
    common::load(
        "case06_no_trivial_fixpoint",
        vec![class_to_table(), feature_to_column()],
        g,
    )
}

struct World {
    g: Graph,
    vs: ValueStore,
    engine: Engine<'static>,
    c: Id,
    f1: Id,
    f2: Id,
}

impl World {
    fn new() -> Self {
        let mut g = Graph::new();
        let c = g.add_baseline("C", "Class");
        let cn = g.add_baseline("C/name", "className");
        g.connect(c, cn, Status::Solid);
        let mk_feat = |g: &mut Graph, ext: &str| -> Id {
            let f = g.add_baseline(ext, "Feature");
            let n = g.add_baseline(&format!("{ext}/name"), "featName");
            g.connect(f, n, Status::Solid);
            g.connect(c, f, Status::Solid);
            f
        };
        let f1 = mk_feat(&mut g, "F1");
        let f2 = mk_feat(&mut g, "F2");
        let f1n = g.child_leaf_of_type(&f1, "featName").unwrap();
        let f2n = g.child_leaf_of_type(&f2, "featName").unwrap();
        let rules: &'static [DirectedRule] = Box::leak(ruleset(&mut g).into_boxed_slice());
        let mut w = World {
            g,
            vs: ValueStore::default(),
            engine: Engine::new(rules),
            c,
            f1,
            f2,
        };
        w.vs.insert(cn, "C");
        w.vs.insert(f1n, "F1");
        w.vs.insert(f2n, "F2");
        w.engine.admit_delta(&[DeltaDomain::Source]);
        w
    }

    fn sync(&mut self) {
        let mut rounds = 0;
        loop {
            self.engine.seed(&self.g, &self.vs);
            let mut applied = 0u64;
            let mut guard = 0;
            while self.engine.step(&mut self.g, &self.vs).is_some() {
                applied += 1;
                guard += 1;
                assert!(guard < 100_000, "sync loop");
            }
            let had_tt = self
                .g
                .iter_nodes()
                .any(|n| n.status == Status::TentativeTombstone);
            self.engine.consolidate(&mut self.g);
            rounds += 1;
            if (applied == 0 && !had_tt) || rounds > 20 {
                break;
            }
        }
    }

    fn conn_alive(&self, conn: &Id) -> bool {
        self.g
            .connection(conn)
            .is_some_and(|c| c.status.is_matchable())
    }
    fn node_alive(&self, id: &Id) -> bool {
        self.g.node(id).is_some_and(|n| n.status.is_matchable())
    }
    fn live_count(&self, typ: &str) -> usize {
        self.g
            .types
            .lookup(typ)
            .map(|t| {
                self.g
                    .nodes_of_type(t)
                    .filter(|n| n.status.is_matchable())
                    .count()
            })
            .unwrap_or(0)
    }

    fn table_of(&self, class: &Id) -> Option<Id> {
        let ct = self.g.types.lookup("ClassCorr")?;
        let tt = self.g.types.lookup("Table")?;
        for p in self.g.parts_by_other_type(class, ct) {
            if !self.conn_alive(&p.connection) || !self.node_alive(&p.other) {
                continue;
            }
            for q in self.g.parts(&p.other) {
                if q.other != *class && q.other_typ == tt && self.node_alive(&q.other) {
                    return Some(q.other);
                }
            }
        }
        None
    }

    /// Lebendige Columns unter einer Table.
    fn live_columns(&self, table: &Id) -> usize {
        let ct = self.g.types.lookup("Column");
        ct.map(|t| {
            self.g
                .parts_by_other_type(table, t)
                .filter(|p| {
                    p.outgoing && self.conn_alive(&p.connection) && self.node_alive(&p.other)
                })
                .count()
        })
        .unwrap_or(0)
    }

    fn remove_node(&mut self, id: Id) {
        self.g.set_node_status(&id, Status::Tombstone);
        self.engine.element_removed(&id);
        self.engine.element_deleted(&mut self.g, &id);
    }

    fn annotate(&mut self, node: Id, note: &str) {
        let a = self.g.add_baseline(&format!("note/{note}"), "Note");
        self.g.connect(node, a, Status::Solid);
        self.vs.insert(a, note);
    }
    fn note_of(&self, node: &Id) -> Option<String> {
        let nt = self.g.types.lookup("Note")?;
        self.g
            .parts_by_other_type(node, nt)
            .filter(|p| p.outgoing && self.conn_alive(&p.connection) && self.node_alive(&p.other))
            .find_map(|p| self.g.resolve_value(&p.other, &self.vs))
    }
}

#[test]
fn initial_sync() {
    let mut w = World::new();
    w.sync();
    assert_eq!(w.live_count("Table"), 1);
    assert_eq!(w.live_count("Column"), 2);
}

/// DER Paper-Test: beide Features löschen → Class/Table werden NICHT
/// automatisch mit weggeräumt (kein trivialer Fixpunkt). Die Table
/// überlebt LEER — die c₂-Violation ist strukturell sichtbar, nicht
/// durch „lösche alles" versteckt.
#[test]
fn emptying_does_not_collapse_to_trivial_fixpoint() {
    let mut w = World::new();
    w.sync();
    let table = w.table_of(&w.c).expect("Table");
    // manuelle Ziel-Ergänzung an der Table — muss überleben.
    w.annotate(table, "kept");

    w.engine.admit_delta(&[DeltaDomain::Source]);
    w.remove_node(w.f1);
    w.remove_node(w.f2);
    w.sync();

    // Features + ihre Columns sind Tombstone (User-Op).
    assert!(
        !w.node_alive(&w.f1) && !w.node_alive(&w.f2),
        "Features tombstone"
    );
    assert_eq!(w.live_count("Column"), 0, "Columns tombstone");
    // ABER: Class bleibt Solid, Table überlebt LEER — kein automatisches
    // DeleteEmptyClass, kein trivialer Fixpunkt.
    assert!(w.node_alive(&w.c), "Class bleibt (kein trivial-Fixpunkt)");
    let table_after = w.table_of(&w.c).expect("Table überlebt leer");
    assert_eq!(table_after, table, "Table ID-stabil (nicht weggeräumt)");
    assert_eq!(w.live_count("Table"), 1, "Table überlebt");
    assert_eq!(
        w.live_columns(&table_after),
        0,
        "Table ist leer — c₂-Violation SICHTBAR"
    );
    // Informationserhalt: manuelle Ergänzung überlebt.
    assert_eq!(
        w.note_of(&table_after).as_deref(),
        Some("kept"),
        "manuelle Ergänzung überlebt"
    );
}

/// Kontrast-Beleg: die Engine räumt eine leere Struktur nicht ab, aber sobald
/// das Delta die Class SELBST löscht, verschwindet auch das Ziel
/// (ehrliche Propagation, keine Heuristik).
#[test]
fn deleting_the_class_itself_does_remove_target() {
    let mut w = World::new();
    w.sync();
    w.engine.admit_delta(&[DeltaDomain::Source]);
    w.remove_node(w.f1);
    w.remove_node(w.f2);
    w.remove_node(w.c); // jetzt explizit die Class löschen
    w.sync();
    assert!(!w.node_alive(&w.c), "Class tombstone");
    assert_eq!(
        w.live_count("Table"),
        0,
        "Table folgt der expliziten Class-Löschung"
    );
}
