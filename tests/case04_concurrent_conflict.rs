//! Case 4 — SLE 2020 (Fritsche, Kosiol, Möller, Schürr, Taentzer):
//! CONCURRENT konfligierende Edits. FRISCH aus dem Paper-Problem, ohne
//! Vorkenntnisse der ersten Generation (2026-07-16).
//!
//! ── Das Paper-Problem ──────────────────────────────────────────────
//! Zwei unabhängige, gleichzeitige Edits im SELBEN Bereich: der
//! Source-Nutzer LÖSCHT eine Class C1, der Target-Nutzer FÜGT
//! gleichzeitig einen Eintrag unter dem korrespondierenden Doc D1
//! HINZU. Das Paper zeigt: KEIN TGG-Tool kann diese Konflikt-Klasse
//! deterministisch AUFLÖSEN („propagation-based conflict detection is
//! not deterministic in general"). Der Beitrag ist Konflikt-ERKENNUNG.
//!
//! ── Die frische These (bewusst NICHT „glatte Lösung") ───────────
//! die Engine soll den Konflikt NICHT silent (und damit womöglich falsch)
//! auflösen, sondern ihn DETERMINISTISCH SICHTBAR machen: die
//! Source-Löschung zieht das regel-erzeugte Ziel (D1, E1) zurück; die
//! manuelle Target-Ergänzung (E_extra) wird NICHT mitgerissen (sie ist
//! kein Regel-Erzeugnis) — sie überlebt und hängt nun an einem
//! Tombstone-Parent. Diese „aktives Element an totem Kontext"-Signatur
//! IST die ehrliche, detektierbare Konflikt-Darstellung.
//!
//! Schnitt aus dem Problem: Class→Doc, Method→Entry (Kontext CorrClass).
//! `methods` anonyme Containment-Kante (kein Delta darauf); das Delta
//! ist ein Knoten-Delta (DelNode C1) plus eine manuelle Target-Add.

mod common;

use seesaw_tgg::engine::{DeltaDomain, Engine};
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::Status;
use seesaw_tgg::plan::DirectedRule;

type Id = seesaw_tgg::ident::GhostId;

// ═══════════════ TGG aus dem Problem ═══════════════

fn class_to_doc() -> serde_json::Value {
    serde_json::json!({
            "name": "Class_2_Doc", "rank": 30,
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
                    {"name": "r0", "type": "Doc"},
                    {"name": "r1", "type": "docName"}
                ],
                "links": [["r0", "r1"]]
            },
            "corrs": [
                {"type": "CorrClass", "left": "l0", "right": "r0", "role": "establishes", "bindings": [{"left": "l1", "right": "r1"}]}
            ]
    })
}

fn method_to_entry() -> serde_json::Value {
    serde_json::json!({
            "name": "Method_2_Entry", "rank": 20,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Class"},
                    {"name": "l1", "type": "Method"},
                    {"name": "l2", "type": "methodName"}
                ],
                "links": [["l0", "l1"], ["l1", "l2"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Doc"},
                    {"name": "r1", "type": "Entry"},
                    {"name": "r2", "type": "entryName"}
                ],
                "links": [["r0", "r1"], ["r1", "r2"]]
            },
            "corrs": [
                {"type": "CorrClass", "left": "l0", "right": "r0", "role": "references"},
                {"type": "CorrMethod", "left": "l1", "right": "r1", "role": "establishes", "bindings": [{"left": "l2", "right": "r2"}]}
            ]
    })
}

fn ruleset(g: &mut Graph) -> Vec<DirectedRule> {
    common::load(
        "case04_concurrent_conflict",
        vec![class_to_doc(), method_to_entry()],
        g,
    )
}

// ═══════════════ World ═══════════════

struct World {
    g: Graph,
    vs: ValueStore,
    engine: Engine<'static>,
    c1: Id,
    c2: Id,
    m2: Id,
}

impl World {
    /// C1 ⊇ M1, C2 ⊇ M2.
    fn new() -> Self {
        let mut g = Graph::new();
        let mk =
            |g: &mut Graph, cext: &str, cname: &str, mext: &str, mname: &str| -> (Id, Id, Id, Id) {
                let c = g.add_baseline(cext, "Class");
                let cn = g.add_baseline(&format!("{cext}/name"), "className");
                g.connect(c, cn, Status::Solid);
                let m = g.add_baseline(mext, "Method");
                let mn = g.add_baseline(&format!("{mext}/name"), "methodName");
                g.connect(m, mn, Status::Solid);
                g.connect(c, m, Status::Solid);
                let _ = (cname, mname);
                (c, cn, m, mn)
            };
        let (c1, c1n, _m1, m1n) = mk(&mut g, "C1", "C1", "M1", "M1");
        let (c2, c2n, m2, m2n) = mk(&mut g, "C2", "C2", "M2", "M2");

        let rules: &'static [DirectedRule] = Box::leak(ruleset(&mut g).into_boxed_slice());
        let mut w = World {
            g,
            vs: ValueStore::default(),
            engine: Engine::new(rules),
            c1,
            c2,
            m2,
        };
        w.vs.insert(c1n, "C1");
        w.vs.insert(m1n, "M1");
        w.vs.insert(c2n, "C2");
        w.vs.insert(m2n, "M2");
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

    fn doc_of(&self, class: &Id) -> Option<Id> {
        self.corr_partner(class, "CorrClass", "Doc")
    }
    fn entry_of(&self, method: &Id) -> Option<Id> {
        self.corr_partner(method, "CorrMethod", "Entry")
    }

    fn corr_partner(&self, node: &Id, corr_typ: &str, want: &str) -> Option<Id> {
        let ct = self.g.types.lookup(corr_typ)?;
        let wt = self.g.types.lookup(want)?;
        for p in self.g.parts_by_other_type(node, ct) {
            if !self.conn_alive(&p.connection) || !self.node_alive(&p.other) {
                continue;
            }
            for q in self.g.parts(&p.other) {
                if q.other != *node
                    && q.other_typ == wt
                    && self.conn_alive(&q.connection)
                    && self.node_alive(&q.other)
                {
                    return Some(q.other);
                }
            }
        }
        None
    }

    fn method_of(&self, class: &Id) -> Option<Id> {
        let mt = self.g.types.lookup("Method")?;
        self.g
            .parts_by_other_type(class, mt)
            .find(|p| p.outgoing && self.conn_alive(&p.connection))
            .map(|p| p.other)
    }

    /// Parent (eingehende Doc-Beteiligung) eines Entry — auch wenn er
    /// Tombstone ist (für die Konflikt-Detektion).
    fn parent_doc_any(&self, entry: &Id) -> Option<(Id, Status)> {
        let dt = self.g.types.lookup("Doc")?;
        for p in self.g.parts_by_other_type(entry, dt) {
            if p.outgoing {
                continue; // Doc → Entry ist eingehend am Entry
            }
            if let Some(n) = self.g.node(&p.other) {
                return Some((p.other, n.status));
            }
        }
        None
    }

    fn remove_node(&mut self, id: Id) {
        self.g.set_node_status(&id, Status::Tombstone);
        self.engine.element_removed(&id);
        self.engine.element_deleted(&mut self.g, &id);
    }
}

// ═══════════════ Tests ═══════════════

#[test]
fn initial_sync() {
    let mut w = World::new();
    w.sync();
    assert_eq!(w.live_count("Doc"), 2);
    assert_eq!(w.live_count("Entry"), 2);
}

/// DER Paper-Test: concurrent conflicting edit — Source löscht C1,
/// Target fügt gleichzeitig E_extra unter D1 hinzu. These: KEINE
/// silent (falsche) Auflösung, sondern deterministisch SICHTBARER
/// Konflikt — E_extra überlebt an einem Tombstone-Parent.
#[test]
fn concurrent_conflict_is_made_visible() {
    let mut w = World::new();
    w.sync();
    let d1 = w.doc_of(&w.c1).expect("D1");
    let m1 = w.method_of(&w.c1).expect("M1");
    let e1 = w.entry_of(&m1).expect("E1");
    let d2 = w.doc_of(&w.c2).expect("D2");

    // Concurrent-Delta (EIN logischer Edit-Schritt):
    //  - Source: DelNode(C1)
    //  - Target: AddNode(E_extra unter D1)  ← manuelle Ziel-Ergänzung
    w.engine
        .admit_delta(&[DeltaDomain::Source, DeltaDomain::Target]);
    let e_extra = w.g.add_baseline("E_extra", "Entry");
    let ex_n = w.g.add_baseline("E_extra/name", "entryName");
    w.g.connect(e_extra, ex_n, Status::Solid);
    w.g.connect(d1, e_extra, Status::Solid);
    w.vs.insert(ex_n, "E_extra");
    w.remove_node(w.c1);
    w.sync();

    // Source-Seite: C1 weg → D1, E1 (regel-erzeugt) zurückgezogen.
    assert!(!w.node_alive(&w.c1), "C1 tombstone");
    assert!(!w.node_alive(&d1), "D1 tombstone (C1-Erzeugnis)");
    assert!(!w.node_alive(&e1), "E1 tombstone (M1-Erzeugnis)");

    // KONFLIKT SICHTBAR: E_extra (manuell, kein Regel-Erzeugnis)
    // überlebt — wird NICHT silent mitgerissen — und hängt jetzt an
    // einem Tombstone-Doc. Genau diese Signatur macht den Konflikt
    // deterministisch detektierbar.
    assert!(
        w.node_alive(&e_extra),
        "E_extra überlebt (nicht silent gelöscht)"
    );
    let (parent, pstatus) = w.parent_doc_any(&e_extra).expect("E_extra hat Doc-Parent");
    assert_eq!(parent, d1, "E_extra hängt an D1");
    assert_eq!(
        pstatus,
        Status::Tombstone,
        "Konflikt-Signatur: aktives Element an totem Parent"
    );

    // C2-Zweig völlig unberührt (Konflikt ist lokal).
    assert!(w.node_alive(&d2), "D2 unberührt");
    assert!(w.node_alive(&w.c2), "C2 unberührt");
    assert!(w.entry_of(&w.m2).is_some(), "E2 unberührt");
}

/// Determinismus: derselbe concurrent Edit liefert bit-identisch
/// dieselbe Konflikt-Signatur (E_extra-Id + Parent-Status) über
/// mehrere Läufe — die „nicht deterministisch auflösbare" Klasse ist
/// hier deterministisch REPRÄSENTIERT.
#[test]
fn conflict_representation_is_deterministic() {
    let run = || -> (Id, Status) {
        let mut w = World::new();
        w.sync();
        let d1 = w.doc_of(&w.c1).unwrap();
        w.engine
            .admit_delta(&[DeltaDomain::Source, DeltaDomain::Target]);
        let e_extra = w.g.add_baseline("E_extra", "Entry");
        w.g.connect(d1, e_extra, Status::Solid);
        w.remove_node(w.c1);
        w.sync();
        let (_p, st) = w.parent_doc_any(&e_extra).unwrap();
        (e_extra, st)
    };
    assert_eq!(
        run(),
        run(),
        "Konflikt-Signatur deterministisch reproduzierbar"
    );
}
