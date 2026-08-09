//! Case 5 — Hildebrandt et al. 2013, Backtracking-„Trilemma", FRISCH
//! und aus dem Problem geschnitten neu bewertet (2026-07-16, nach Sandras Analyse).
//!
//! ── Kern-Einsicht (Sandra) ─────────────────────────────────────────
//! In der Engine gibt es KEINE spekulative Regelwahl und daher kein
//! Backtracking: der Prozess ist total geordnet (Rang, dann μ) und
//! deterministisch — matchen bis Sättigung, dann nächste Regel. Das
//! „Trilemma" ist ein Artefakt spekulativer Ausführung; es entsteht
//! hier gar nicht.
//!
//! Der EINZIGE Fall, in dem die Rang-Ordnung das Ergebnis wirklich
//! bestimmt, ist ein **echtes Superset-Paar**: eine frühere (höher
//! priorisierte) Regel R_g ist ein echtes Superset einer späteren
//! R_s (alles, was R_s matcht, matcht auch R_g). Dann wird R_s
//! KOMPLETT überschattet — sie kommt nie zum Zug.
//!
//! ── Auflösung (Sandra) = Meta-Level, nicht Engine ─────────────────
//! Nicht die Engine backtrackt, sondern der Implementierer/User:
//! Rang als Hook umdefinieren → kompletter (deterministischer) Run →
//! passt nicht → Ruleset REINDEXEN (spezifischere Regel zuerst,
//! „most specific first") → wieder testen. Jeder Run reproduzierbar.
//! Und das Superset-Paar ist STATISCH erkennbar (Pattern-Subsumption).
//!
//! Dieser Test zeigt beides: die Überschattung unter Ordnung A, das
//! intendierte Ergebnis nach Reindex (Ordnung B), und einen statischen
//! Superset-Check.

mod common;

use seesaw_tgg::engine::Engine;
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::Status;
use seesaw_tgg::plan::DirectedRule;
use seesaw_tgg::rules::format::RuleDecl;

type Id = seesaw_tgg::ident::GhostId;

// ═══════════════ Das Superset-Paar (Rang parametrisiert) ════════════════════

/// Allgemein: JEDES Node ↔ Target(content="generic"). Kein Constraint
/// ⇒ Superset. `rank` ist parametrisiert (der „Hook").
fn general_rule(rank: u64) -> serde_json::Value {
    serde_json::json!({
            "name": "Node_2_Generic", "rank": rank,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Node"}
                ]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Target"},
                    {"name": "r1", "type": "content",
                     "predicate": {"kind": "equals", "value": "generic"}, "constant": "generic"}
                ],
                "links": [["r0", "r1"]]
            },
            "corrs": [
                {"type": "TargetCorr", "left": "l0", "right": "r0", "role": "establishes"}
            ]
    })
}

/// Spezifisch: Node MIT special=true ↔ Target(content="special").
/// Zusätzlicher Constraint ⇒ echtes Subset von `general_rule`.
fn special_rule(rank: u64) -> serde_json::Value {
    serde_json::json!({
            "name": "Node_2_Special", "rank": rank,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Node"},
                    {"name": "l1", "type": "special",
                     "predicate": {"kind": "equals", "value": "true"}, "constant": "true"}
                ],
                "links": [["l0", "l1"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Target"},
                    {"name": "r1", "type": "content",
                     "predicate": {"kind": "equals", "value": "special"}, "constant": "special"}
                ],
                "links": [["r0", "r1"]]
            },
            "corrs": [
                {"type": "TargetCorr", "left": "l0", "right": "r0", "role": "establishes"}
            ]
    })
}

fn ruleset(g: &mut Graph, rank_general: u64, rank_special: u64) -> Vec<DirectedRule> {
    let specs = [general_rule(rank_general), special_rule(rank_special)];
    common::load_forward("case05_superset_order", specs.to_vec(), g)
}

// ═══════════════ World (Rang-Ordnung als Parameter) ════════════════

struct World {
    g: Graph,
    vs: ValueStore,
    engine: Engine<'static>,
    n1: Id,
    n2: Id,
}

impl World {
    /// n1 (special=false), n2 (special=true). Rang-Ordnung frei.
    fn new(rank_general: u64, rank_special: u64) -> Self {
        let mut g = Graph::new();
        let mk = |g: &mut Graph, ext: &str, special: bool| -> (Id, Id) {
            let n = g.add_baseline(ext, "Node");
            let s = g.add_baseline(&format!("{ext}/special"), "special");
            g.connect(n, s, Status::Solid);
            let _ = special;
            (n, s)
        };
        let (n1, s1) = mk(&mut g, "n1", false);
        let (n2, s2) = mk(&mut g, "n2", true);
        let rules: &'static [DirectedRule] =
            Box::leak(ruleset(&mut g, rank_general, rank_special).into_boxed_slice());
        let mut w = World {
            g,
            vs: ValueStore::default(),
            engine: Engine::new(rules),
            n1,
            n2,
        };
        w.vs.insert(s1, "false");
        w.vs.insert(s2, "true");
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

    /// content des Target, das ein Node über TargetCorr etabliert hat.
    fn target_content(&self, node: &Id) -> Option<String> {
        let ct = self.g.types.lookup("TargetCorr")?;
        let tt = self.g.types.lookup("Target")?;
        for p in self.g.parts_by_other_type(node, ct) {
            if !self.conn_alive(&p.connection) || !self.node_alive(&p.other) {
                continue;
            }
            for q in self.g.parts(&p.other) {
                if q.other != *node && q.other_typ == tt && self.node_alive(&q.other) {
                    return Some(self.leaf_value(&q.other, "content"));
                }
            }
        }
        None
    }

    fn leaf_value(&self, node: &Id, leaf: &str) -> String {
        self.g
            .child_leaf_of_type(node, leaf)
            .and_then(|l| self.g.resolve_value(&l, &self.vs))
            .unwrap_or_default()
    }
}

// ═══════════════ Statische Superset-Erkennung (Pattern-Subsumption) ═════════

/// Ist `sub`-Regel ein echtes Subset von `sup`? Minimal-Subsumption:
/// jeder Knoten von `sup`s Eingangsseite hat in `sub` einen Knoten
/// gleichen Typs (Positions-treu über den Anker), und `sub` legt
/// mindestens einen ZUSÄTZLICHEN Constraint/Knoten drauf. Dann matcht
/// `sub` überall höchstens dort, wo `sup` matcht — echtes Subset.
fn is_proper_subset(sub: &RuleDecl, sup: &RuleDecl) -> bool {
    // Anker (Position 0) muss typgleich sein.
    if sub.left.nodes.first().map(|n| &n.typ) != sup.left.nodes.first().map(|n| &n.typ) {
        return false;
    }
    // Jeder sup-Knoten hat in sub einen typgleichen (sup ⊆ sub als
    // Multiset der Typen).
    let mut sub_types: Vec<&String> = sub.left.nodes.iter().map(|n| &n.typ).collect();
    for sn in &sup.left.nodes {
        if let Some(pos) = sub_types.iter().position(|t| **t == sn.typ) {
            sub_types.remove(pos);
        } else {
            return false; // sup verlangt etwas, das sub nicht hat ⇒ nicht Subset
        }
    }
    // sub ist ECHT restriktiver: hat mehr Knoten ODER mehr value-Constraints.
    let sub_constraints = sub
        .left
        .nodes
        .iter()
        .filter(|n| n.predicate.is_some())
        .count();
    let sup_constraints = sup
        .left
        .nodes
        .iter()
        .filter(|n| n.predicate.is_some())
        .count();
    sub.left.nodes.len() > sup.left.nodes.len() || sub_constraints > sup_constraints
}

// ═══════════════ Tests ═══════════════

/// Ordnung A: general höher priorisiert (rank 100 > 90). general ist
/// Superset ⇒ es ÜBERSCHATTET special. n2 (special=true) bekommt
/// „generic" statt „special" — deterministisch, aber unintendiert.
#[test]
fn superset_shadows_specific_under_order_a() {
    let mut w = World::new(/*general*/ 100, /*special*/ 90);
    w.sync();
    assert_eq!(
        w.target_content(&w.n1).as_deref(),
        Some("generic"),
        "n1 generic"
    );
    assert_eq!(
        w.target_content(&w.n2).as_deref(),
        Some("generic"),
        "n2 ÜBERSCHATTET: general (Superset) hat special verdrängt"
    );
    // Nur 2 Targets — special hat nie gefeuert.
    assert_eq!(
        w.g.types.lookup("Target").map(|t| w
            .g
            .nodes_of_type(t)
            .filter(|n| n.status.is_matchable())
            .count()),
        Some(2)
    );
}

/// Ordnung B (REINDEX, „most specific first"): special höher (rank 100
/// > 90). special feuert zuerst für special-Nodes, general für den
/// > Rest. n2 → „special", n1 → „generic". Das intendierte Ergebnis —
/// > ohne Backtracking, nur durch Umsortieren des Rulesets.
#[test]
fn reindex_specific_first_gives_intended_result() {
    let mut w = World::new(/*general*/ 90, /*special*/ 100);
    w.sync();
    assert_eq!(
        w.target_content(&w.n1).as_deref(),
        Some("generic"),
        "n1 generic"
    );
    assert_eq!(
        w.target_content(&w.n2).as_deref(),
        Some("special"),
        "n2 special (intendiert)"
    );
}

/// Beide Runs sind deterministisch und reproduzierbar (kein Raten):
/// derselbe Rang liefert bit-identisch dasselbe Ergebnis.
#[test]
fn each_run_is_deterministic() {
    let run = |rg: u64, rs: u64| -> (String, String) {
        let mut w = World::new(rg, rs);
        w.sync();
        (
            w.target_content(&w.n1).unwrap_or_default(),
            w.target_content(&w.n2).unwrap_or_default(),
        )
    };
    assert_eq!(run(100, 90), run(100, 90), "Ordnung A reproduzierbar");
    assert_eq!(run(90, 100), run(90, 100), "Ordnung B reproduzierbar");
    // Und die beiden Ordnungen unterscheiden sich WIRKLICH nur in n2.
    assert_ne!(run(100, 90).1, run(90, 100).1, "n2 hängt an der Ordnung");
}

/// Statische Analyse: das Superset-Paar ist ohne Ausführung erkennbar.
/// Genau das macht den Fall LOKAL entscheidbar (kein Backtracking):
/// erkenne special ⊂ general, verlange „specific first" (oder warne).
#[test]
fn superset_pair_is_statically_detectable() {
    let decl =
        |v: serde_json::Value| -> RuleDecl { serde_json::from_value(v).expect("Regel parst") };
    let general = decl(general_rule(100));
    let special = decl(special_rule(90));
    assert!(
        is_proper_subset(&special, &general),
        "special ist echtes Subset von general (statisch erkannt)"
    );
    // Umgekehrt NICHT.
    assert!(
        !is_proper_subset(&general, &special),
        "general ist kein Subset von special"
    );
    // Reflexiv/disjunkt-Kontrolle: eine unabhängige Regel ist kein Subset.
    let unrelated: RuleDecl = serde_json::from_value(serde_json::json!({
        "name": "Other", "rank": 50,
        "left": {
            "anchor": "l0",
            "nodes": [
                {"name": "l0", "type": "Other"}
            ]
        },
        "right": {
            "anchor": "r0",
            "nodes": [
                {"name": "r0", "type": "X"}
            ]
        },
        "corrs": [
            {"type": "XCorr", "left": "l0", "right": "r0", "role": "establishes"}
        ]
    }))
    .unwrap();
    assert!(
        !is_proper_subset(&unrelated, &general),
        "disjunkte Regel: kein Subset"
    );
}
