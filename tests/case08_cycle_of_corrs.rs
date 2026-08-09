//! Case 8 — Cycle of Corrs (eigener Case, Termination-Pathologie).
//! FRISCH aus dem Problem, frisch geschnitten, ohne Vorkenntnisse der ersten Generation (2026-07-17).
//!
//! ── Das Problem ────────────────────────────────────────────────────
//! Zyklische L-Topologien (A -> B -> C -> A) sind ein Stress-Test fuer
//! TGG-Engines: naive Forward-Translate kann endlos laufen, weil Match-
//! Enumeration und Rule-Anwendung sich gegenseitig speisen (jedes neue
//! Element erzeugt einen neuen Match-Kandidaten). Etablierte Tools
//! (eMoflon u.a.) schliessen zyklische Schemata teils aus oder setzen
//! Step-Limits.
//!
//! ── Die frische These ───────────────────────────────────────────
//! Die Engine terminiert auf zyklischen Inputs OHNE Cycle-spezifische Logik.
//! Zwei strukturell vorhandene Mechanismen tragen das:
//!  1. Duplikations-Saturation: hat jeder L-Knoten seinen R-Partner,
//!     traefe eine erneute Anwendung dieselbe Ghost-Id (strukturelle
//!     Identitaet, Weg C) -> Duplikat -> verworfen.
//!  2. Kanonische mu-Enumeration: Matches in fester Index-Reihenfolge
//!     -> deterministische Cascade-Laenge, unabhaengig vom Pre-Graph.
//!     Die Engine verhaelt sich auf zyklischen wie auf azyklischen Inputs
//!     gleich; Worst-Case linear in der L-Knoten-Zahl, nicht in Cycle-Laenge.
//!
//! ── Schnitt aus dem Problem ─────────────────────────────────────────────
//! Die delta-tragende `next`-Beziehung ist als KNOTEN reifiziert (Typ
//! `Next`), nicht als anonyme Kante. Zwei Regeln erzeugen einen ECHTEN
//! Corr-Zyklus: `Node_2_R` (pro L-Knoten ein R-Knoten + NodeCorr) und
//! `Next_2_RNext` (pro next-Knoten ein RNext + NextCorr, mit den beiden
//! Enden als references-Kontext). Die entstehenden Corrs bilden selbst
//! den Ring NodeCorr_A -> NextCorr_AB -> NodeCorr_B -> ... -> NodeCorr_A.

mod common;

use seesaw_tgg::engine::Engine;
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::Status;
use seesaw_tgg::plan::DirectedRule;

type Id = seesaw_tgg::ident::GhostId;

// ═══════════════ TGG aus dem Problem ═══════════════

/// Pro L-Knoten genau ein R-Knoten + NodeCorr. Das L-Pattern hat NUR
/// den L-Knoten -> genau ein Match pro Knoten; ein zweiter Versuch
/// traefe dieselbe Ghost-Id (Duplikat). Rank hoch: laeuft zuerst.
fn node_to_r() -> serde_json::Value {
    serde_json::json!({
            "name": "Node_2_R", "rank": 20,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "LNode"}
                ]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "RNode"}
                ]
            },
            "corrs": [
                {"type": "NodeCorr", "left": "l0", "right": "r0", "role": "establishes"}
            ]
    })
}

/// Pro reifiziertem next-Knoten ein RNext, mit beiden L-Enden als
/// references-Kontext (ihre NodeCorr muss also schon stehen -> NodeToR
/// zuerst). So wird der Ring auch auf der R-Seite geschlossen.
fn next_to_rnext() -> serde_json::Value {
    serde_json::json!({
            "name": "Next_2_RNext", "rank": 10,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "LNode"},
                    {"name": "l1", "type": "Next"},
                    {"name": "l2", "type": "LNode"}
                ],
                "links": [["l0", "l1"], ["l1", "l2"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "RNode"},
                    {"name": "r1", "type": "RNext"},
                    {"name": "r2", "type": "RNode"}
                ],
                "links": [["r0", "r1"], ["r1", "r2"]]
            },
            "corrs": [
                {"type": "NodeCorr", "left": "l0", "right": "r0", "role": "references"},
                {"type": "NodeCorr", "left": "l2", "right": "r2", "role": "references"},
                {"type": "NextCorr", "left": "l1", "right": "r1", "role": "establishes"}
            ]
    })
}

fn ruleset(g: &mut Graph) -> Vec<DirectedRule> {
    // Forward-only: die Pathologie ist die Forward-Translate-Richtung.
    common::load_forward(
        "case08_cycle_of_corrs",
        vec![node_to_r(), next_to_rnext()],
        g,
    )
}

// ═══════════════ World ═══════════════

struct World {
    g: Graph,
    vs: ValueStore,
    engine: Engine<'static>,
    nodes: Vec<Id>,
}

impl World {
    /// Baut `n` L-Knoten und reifizierte next-Kanten laut `edges`
    /// (Paare von L-Knoten-Indizes). Zyklus A->B->C->A: n=3,
    /// edges=[(0,1),(1,2),(2,0)]. Self-Loop: n=1, edges=[(0,0)].
    fn build(n: usize, edges: &[(usize, usize)]) -> Self {
        let mut g = Graph::new();
        let names = ["A", "B", "C", "D", "E", "F"];
        let nodes: Vec<Id> = (0..n).map(|i| g.add_baseline(names[i], "LNode")).collect();
        for &(from, to) in edges {
            let nx = g.add_baseline(&format!("next/{from}-{to}"), "Next");
            g.connect(nodes[from], nx, Status::Solid);
            g.connect(nx, nodes[to], Status::Solid);
        }
        let rules: &'static [DirectedRule] = Box::leak(ruleset(&mut g).into_boxed_slice());
        World {
            g,
            vs: ValueStore::default(),
            engine: Engine::new(rules),
            nodes,
        }
    }

    /// Fixpunkt-Sync. Gibt die GESAMT-Zahl der Rule-Anwendungen zurueck
    /// (Cascade-Laenge) — der Wert, der bei Termination endlich bleibt
    /// und bei Determinismus reproduzierbar ist. Der guard < 100_000
    /// ist der Endlosschleifen-Faenger: bei Nicht-Termination panict er.
    fn sync(&mut self) -> u64 {
        let mut rounds = 0;
        let mut total = 0u64;
        loop {
            self.engine.seed(&self.g, &self.vs);
            let mut applied = 0u64;
            let mut guard = 0;
            while self.engine.step(&mut self.g, &self.vs).is_some() {
                applied += 1;
                guard += 1;
                assert!(guard < 100_000, "sync loop (Nicht-Termination!)");
            }
            total += applied;
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
        total
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

    /// R-Knoten, den ein L-Knoten ueber NodeCorr etabliert hat.
    fn r_of(&self, l: &Id) -> Option<Id> {
        let ct = self.g.types.lookup("NodeCorr")?;
        let rt = self.g.types.lookup("RNode")?;
        for p in self.g.parts_by_other_type(l, ct) {
            if !self.conn_alive(&p.connection) || !self.node_alive(&p.other) {
                continue;
            }
            for q in self.g.parts(&p.other) {
                if q.other != *l && q.other_typ == rt && self.node_alive(&q.other) {
                    return Some(q.other);
                }
            }
        }
        None
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

fn cycle3() -> World {
    World::build(3, &[(0, 1), (1, 2), (2, 0)])
}

// ═══════════════ Tests ═══════════════

/// Termination: der zyklische A->B->C->A-Input laeuft NICHT endlos.
/// Der guard im Sync-Loop wuerde bei Nicht-Termination paniken; dass
/// sync() zurueckkehrt, IST der Beleg. Die Cascade-Laenge ist endlich.
#[test]
fn cyclic_graph_terminates() {
    let mut w = cycle3();
    let cascade = w.sync();
    assert!(cascade > 0, "es wurde etwas uebersetzt");
    assert!(
        cascade < 100,
        "endliche, kleine Cascade (keine Endlosschleife)"
    );
}

/// 3 L-Knoten -> 3 R-Knoten + 3 NodeCorrs, und der Ring wird auf der
/// R-Seite geschlossen: 3 next-Knoten -> 3 RNext + 3 NextCorrs. Keine
/// Duplikate trotz Zyklus — Duplikations-Saturation greift.
#[test]
fn three_l_nodes_give_three_r_nodes_and_corrs() {
    let mut w = cycle3();
    w.sync();
    assert_eq!(w.live_count("RNode"), 3, "3 R-Knoten (keine Duplikate)");
    assert_eq!(w.live_count("NodeCorr"), 3, "3 NodeCorrs");
    assert_eq!(w.live_count("RNext"), 3, "3 RNext (R-Ring geschlossen)");
    assert_eq!(w.live_count("NextCorr"), 3, "3 NextCorrs");
    // Jeder L-Knoten hat genau seinen R-Partner.
    for l in w.nodes.clone() {
        assert!(w.r_of(&l).is_some(), "L-Knoten hat R-Partner");
    }
}

/// Determinismus: wiederholter Lauf liefert identische Cascade-Laenge
/// UND identische Element-Zahlen. Die kanonische mu-Enumeration macht
/// das Ergebnis unabhaengig von Wiederholung.
#[test]
fn cascade_is_deterministic() {
    let run = || -> (u64, usize, usize) {
        let mut w = cycle3();
        let cascade = w.sync();
        (cascade, w.live_count("RNode"), w.live_count("RNext"))
    };
    let a = run();
    let b = run();
    assert_eq!(
        a, b,
        "identische Cascade-Laenge + Element-Zahlen ueber Laeufe"
    );
}

/// Self-Loop A->A: auch die entartete 1-Knoten-Schleife terminiert und
/// liefert genau 1 R-Knoten + 1 NodeCorr (Paper-Erwartung). Kein
/// Sonderfall-Code — dieselbe Saturation.
#[test]
fn self_loop_terminates() {
    let mut w = World::build(1, &[(0, 0)]);
    let cascade = w.sync();
    assert!(cascade < 100, "Self-Loop terminiert, endliche Cascade");
    assert_eq!(w.live_count("RNode"), 1, "1 L-Knoten -> 1 R-Knoten");
    assert_eq!(w.live_count("NodeCorr"), 1, "1 NodeCorr");
    assert!(w.r_of(&w.nodes[0]).is_some(), "A hat seinen R-Partner");
}

/// Informationserhalt: eine manuelle Ziel-Ergaenzung (Note an einem
/// R-Knoten) ueberlebt einen erneuten Sync auf dem Zyklus — und der
/// zweite Sync erzeugt KEINE Duplikate (Saturation ist stabil).
#[test]
fn manual_annotation_survives_resync() {
    let mut w = cycle3();
    w.sync();
    let ra = w.r_of(&w.nodes[0]).expect("R(A)");
    w.annotate(ra, "kept");
    assert_eq!(w.note_of(&ra).as_deref(), Some("kept"));

    // Zweiter Sync: nichts Neues, keine Duplikate, Notiz bleibt.
    let cascade2 = w.sync();
    assert_eq!(cascade2, 0, "saturiert: zweiter Sync wendet nichts an");
    assert_eq!(
        w.live_count("RNode"),
        3,
        "weiterhin 3 R-Knoten (kein Duplikat)"
    );
    assert_eq!(
        w.note_of(&ra).as_deref(),
        Some("kept"),
        "manuelle Notiz ueberlebt"
    );
}
