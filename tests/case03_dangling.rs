//! Case 3 — JOT 2022 (Weidmann, Yigitbas, Anjorin, Srivastava, Jose):
//! SysML→Event-B mit einer DANGLING transition. FRISCH aus dem
//! Paper-Problem, ohne Vorkenntnisse der ersten Generation (2026-07-16).
//!
//! ── Das Paper-Problem ──────────────────────────────────────────────
//! Eine unvollständige (dangling) Struktur — Transition mit `source`,
//! aber OHNE `target` — soll ehrlich repräsentiert werden: NICHT
//! übersetzt (kein partielles/inkonsistentes Ziel, kein Fehler),
//! solange sie unvollständig ist; und automatisch übersetzt, sobald
//! das fehlende Ende additiv ergänzt wird. Die Paper-Autoren brauchen
//! dafür einen interaktiven Debugger (VICToRy, human-in-the-loop).
//!
//! ── Die frische These ───────────────────────────────────────────
//! Das Pattern-Matching repräsentiert dangling ohne Sonderbehandlung:
//! die TransToEdge-Regel VERLANGT beide Enden — fehlt `target`, matcht
//! sie schlicht nicht. Das additive `AddEdge(target)` triggert die
//! Übersetzung deterministisch über den add-Strom; die bereits
//! übersetzten Elemente bleiben ID-stabil (rein additiv).
//!
//! Schnitt aus dem Problem: `source`/`target` als reifizierte Knoten
//! (SourceRel/TargetRel), `machineEdge` ebenso (MEdge). Statemachine→
//! Machine, State→EventBlock, Variable→EventVariable, und
//! Transition(mit beiden Enden)→MachineEdge.

use seesaw_tgg::engine::Engine;
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::Status;
use seesaw_tgg::plan::DirectedRule;
use seesaw_tgg::rules::format::RuleFile;
use seesaw_tgg::rules::lower::lower_all;
use seesaw_tgg::rules::validate::validate;

type Id = seesaw_tgg::ident::GhostId;

// ═══════════════ TGG aus dem Problem ═══════════════

fn sm_to_machine() -> serde_json::Value {
    serde_json::json!({
            "name": "SM_2_Machine", "rank": 40,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Statemachine"}
                ]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Machine"}
                ]
            },
            "corrs": [
                {"type": "CorrSM", "left": "l0", "right": "r0", "role": "establishes"}
            ]
    })
}

/// State im Statemachine ↔ EventBlock in der Machine (name ↦ name).
fn state_to_event() -> serde_json::Value {
    serde_json::json!({
            "name": "State_2_Event", "rank": 30,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Statemachine"},
                    {"name": "l1", "type": "State"},
                    {"name": "l2", "type": "stateName"}
                ],
                "links": [["l0", "l1"], ["l1", "l2"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Machine"},
                    {"name": "r1", "type": "EventBlock"},
                    {"name": "r2", "type": "ebName"}
                ],
                "links": [["r0", "r1"], ["r1", "r2"]]
            },
            "corrs": [
                {"type": "CorrSM", "left": "l0", "right": "r0", "role": "references"},
                {"type": "CorrState", "left": "l1", "right": "r1", "role": "establishes", "bindings": [{"left": "l2", "right": "r2"}]}
            ]
    })
}

fn var_to_var() -> serde_json::Value {
    serde_json::json!({
            "name": "Var_2_Var", "rank": 20,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Statemachine"},
                    {"name": "l1", "type": "Variable"},
                    {"name": "l2", "type": "varName"}
                ],
                "links": [["l0", "l1"], ["l1", "l2"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Machine"},
                    {"name": "r1", "type": "EventVariable"},
                    {"name": "r2", "type": "evName"}
                ],
                "links": [["r0", "r1"], ["r1", "r2"]]
            },
            "corrs": [
                {"type": "CorrSM", "left": "l0", "right": "r0", "role": "references"},
                {"type": "CorrVar", "left": "l1", "right": "r1", "role": "establishes", "bindings": [{"left": "l2", "right": "r2"}]}
            ]
    })
}

/// Transition mit SourceRel UND TargetRel ↔ MachineEdge zwischen den
/// beiden EventBlocks. OHNE TargetRel matcht das Pattern nicht — das
/// ist die dangling-Repräsentation (keine NAC nötig).
fn trans_to_edge() -> serde_json::Value {
    serde_json::json!({
            "name": "Trans_2_Edge", "rank": 10,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Transition"},
                    {"name": "l1", "type": "SourceRel"},
                    {"name": "l2", "type": "State"},
                    {"name": "l3", "type": "TargetRel"},
                    {"name": "l4", "type": "State"}
                ],
                "links": [["l0", "l1"], ["l1", "l2"], ["l0", "l3"], ["l3", "l4"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "EventBlock"},
                    {"name": "r1", "type": "MEdge"},
                    {"name": "r2", "type": "EventBlock"}
                ],
                "links": [["r0", "r1"], ["r1", "r2"]]
            },
            "corrs": [
                {"type": "CorrState", "left": "l2", "right": "r0", "role": "references"},
                {"type": "CorrState", "left": "l4", "right": "r2", "role": "references"},
                {"type": "CorrTrans", "left": "l0", "right": "r1", "role": "establishes"}
            ]
    })
}

fn ruleset(g: &mut Graph) -> Vec<DirectedRule> {
    let file: RuleFile = serde_json::from_value(serde_json::json!({
        "format": 3,
        "name": "case03_dangling",
        "rules": [
            sm_to_machine(),
            state_to_event(),
            var_to_var(),
            trans_to_edge(),
        ]
    }))
    .expect("Regeldatei parst");
    let resolved = validate(&file).expect("Regeldatei validiert");
    // lower_all liefert je Regel Vorwaerts und Rueckwaerts; dieser Fall
    // fuehrt nur den Forward-Sync (SysML → Event-B).
    lower_all(&resolved, g)
        .expect("Regeln lowern")
        .into_iter()
        .step_by(2)
        .collect()
}

// ═══════════════ World ═══════════════

struct World {
    g: Graph,
    vs: ValueStore,
    engine: Engine<'static>,
    s2: Id,
    t1: Id,
    t2: Id,
}

impl World {
    /// sm ⊇ {s1(START), s2(STOP), v(finish), t1, t2}.
    /// t1: source→s1, target→s2 (valide). t2: source→s1, KEIN target
    /// (dangling).
    fn new() -> Self {
        let mut g = Graph::new();
        let sm = g.add_baseline("sm", "Statemachine");
        let mk_state = |g: &mut Graph, ext: &str| -> Id {
            let s = g.add_baseline(ext, "State");
            let n = g.add_baseline(&format!("{ext}/name"), "stateName");
            g.connect(s, n, Status::Solid);
            g.connect(sm, s, Status::Solid);
            s
        };
        let s1 = mk_state(&mut g, "s1");
        let s2 = mk_state(&mut g, "s2");
        // Namen setzen (nach Graph-Bau via vs unten).
        let v = g.add_baseline("v", "Variable");
        let vn = g.add_baseline("v/name", "varName");
        g.connect(v, vn, Status::Solid);
        g.connect(sm, v, Status::Solid);
        let t1 = g.add_baseline("t1", "Transition");
        g.connect(sm, t1, Status::Solid);
        let t2 = g.add_baseline("t2", "Transition");
        g.connect(sm, t2, Status::Solid);
        // source/target reifiziert.
        let src1 = g.add_baseline("src/t1/s1", "SourceRel");
        g.connect(t1, src1, Status::Solid);
        g.connect(src1, s1, Status::Solid);
        let tgt1 = g.add_baseline("tgt/t1/s2", "TargetRel");
        g.connect(t1, tgt1, Status::Solid);
        g.connect(tgt1, s2, Status::Solid);
        let src2 = g.add_baseline("src/t2/s1", "SourceRel");
        g.connect(t2, src2, Status::Solid);
        g.connect(src2, s1, Status::Solid);
        // t2 hat KEIN TargetRel — dangling.

        // Namen-Blätter einsammeln für vs.
        let s1n = g.child_leaf_of_type(&s1, "stateName").unwrap();
        let s2n = g.child_leaf_of_type(&s2, "stateName").unwrap();

        let rules: &'static [DirectedRule] = Box::leak(ruleset(&mut g).into_boxed_slice());
        let mut w = World {
            g,
            vs: ValueStore::default(),
            engine: Engine::new(rules),
            s2,
            t1,
            t2,
        };
        w.vs.insert(s1n, "START");
        w.vs.insert(s2n, "STOP");
        w.vs.insert(vn, "finish");
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

    fn edge_of(&self, trans: &Id) -> Option<Id> {
        let ct = self.g.types.lookup("CorrTrans")?;
        let mt = self.g.types.lookup("MEdge")?;
        for p in self.g.parts_by_other_type(trans, ct) {
            if !self.conn_alive(&p.connection) || !self.node_alive(&p.other) {
                continue;
            }
            for q in self.g.parts(&p.other) {
                if q.other != *trans
                    && q.other_typ == mt
                    && self.conn_alive(&q.connection)
                    && self.node_alive(&q.other)
                {
                    return Some(q.other);
                }
            }
        }
        None
    }

    /// Additiver Edit: das fehlende TargetRel(t2→s2) ergänzen.
    fn complete_t2(&mut self) {
        let tgt = self.g.add_baseline("tgt/t2/s2", "TargetRel");
        self.g.connect(self.t2, tgt, Status::Solid);
        self.g.connect(tgt, self.s2, Status::Solid);
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

// ═══════════════ Tests ═══════════════

#[test]
fn dangling_is_not_translated() {
    let mut w = World::new();
    w.sync();
    // 1 Machine, 2 EventBlocks, 1 EventVariable, 1 MachineEdge (nur t1).
    assert_eq!(w.live_count("Machine"), 1);
    assert_eq!(w.live_count("EventBlock"), 2);
    assert_eq!(w.live_count("EventVariable"), 1);
    assert_eq!(w.live_count("MEdge"), 1, "nur t1 übersetzt");
    assert!(w.edge_of(&w.t1).is_some(), "t1 → MachineEdge");
    // t2 ist dangling → KEINE Übersetzung, ehrlich repräsentiert.
    assert!(w.edge_of(&w.t2).is_none(), "t2 dangling: kein MachineEdge");
}

/// Additive Vervollständigung: TargetRel(t2→s2) ergänzen → t2 wird
/// übersetzt. Der bereits übersetzte MachineEdge von t1 bleibt
/// ID-stabil (inkl. manueller Ergänzung); rein additiv.
#[test]
fn completing_dangling_translates_additively() {
    let mut w = World::new();
    w.sync();
    let edge_t1_before = w.edge_of(&w.t1).expect("t1-Edge");
    w.annotate(edge_t1_before, "reviewed");

    w.complete_t2();
    w.sync();

    // t2 jetzt übersetzt → 2 MachineEdges.
    assert_eq!(w.live_count("MEdge"), 2, "t2 nachträglich übersetzt");
    assert!(w.edge_of(&w.t2).is_some(), "t2 → MachineEdge");
    // t1's MachineEdge ID-stabil, manuelle Ergänzung überlebt (additiv).
    let edge_t1_after = w.edge_of(&w.t1).expect("t1-Edge bleibt");
    assert_eq!(edge_t1_after, edge_t1_before, "t1-Edge ID-stabil");
    assert_eq!(
        w.note_of(&edge_t1_after).as_deref(),
        Some("reviewed"),
        "Notiz überlebt"
    );
    // keine Duplikate.
    assert_eq!(w.live_count("EventBlock"), 2);
    assert_eq!(w.live_count("Machine"), 1);
}
