//! Case 7 — FAoC 2021 (Weidmann, Anjorin): Schema-Constraints, die TGG-
//! Tools einhalten müssen. FRISCH aus dem Paper, ohne Vorkenntnisse der ersten Generation
//! (2026-07-17). Drei Sub-Cases.
//!
//! 7a NoTwoGlossaries: höchstens ein Glossary pro Doc.
//! 7b NoEmptyClass:    leere Class wird nicht übersetzt.
//! 7c SameNameSameGlossaryEntry: Methods gleichen Namens teilen einen
//!    GlossaryEntry.
//!
//! Paper: klassische TGG-Tools halten diese Constraints NICHT direkt
//! ein; Weidmann/Anjorin brauchen eine ILP-basierte „maximum partial
//! solution".
//!
//! Frische These: 7a/7b fallen aus der Kern-Semantik (strukturelle
//! Identität + Pattern-Strenge) ohne Zusatzmechanik. 7c ist der ehrliche
//! Fall: die Default-Identität ist PER-ELEMENT (2 Methods → 2 Entries)
//! — die Name-basierte Teilung ist eine WERT-Constraint, die die Engine nicht
//! per Default erfüllt (kein Wert im Hash).
//!
//! 7c ohne NAC (Sandras These, 2026-07-18): die frühere Lösung nutzte
//! create-if-absent + NAC (Case-16-Muster). Die NAC kompensiert eine
//! FEHLENDE GRAPHEN-EBENE — die Namens-Gruppe. Im flachen Method-Modell
//! existiert keine Gruppe; die NAC rekonstruiert sie zur Regel-Zeit (das
//! ist der Fehler). Positiv: der Ingest baut die Gruppen-Ebene mit —
//! Methods gleichen Namens hängen an EINER NameGroup (beim ersten
//! Auftreten geschrieben, danach wiederverwendet). Die Wert-Gleichheit
//! wird EINMAL beim Schreiben aufgelöst, die Engine bleibt wert-frei.
//! Danach ist die Regel reines Bauen: NameGroup → GlossaryEntry,
//! Method unter NameGroup → MethCorr auf dieselbe Entry. Kein Join,
//! keine NAC.

mod common;

use std::collections::BTreeMap;

use seesaw_tgg::engine::Engine;
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::Status;
use seesaw_tgg::plan::DirectedRule;

type Id = seesaw_tgg::ident::GhostId;

// ═══════════════ Gemeinsame Sync-World ═══════════════

struct World {
    g: Graph,
    vs: ValueStore,
    engine: Engine<'static>,
}

impl World {
    fn with(g: Graph, rules: Vec<DirectedRule>) -> Self {
        Self::with_vs(g, rules, ValueStore::default())
    }
    fn with_vs(g: Graph, rules: Vec<DirectedRule>, vs: ValueStore) -> Self {
        let leaked: &'static [DirectedRule] = Box::leak(rules.into_boxed_slice());
        World {
            g,
            vs,
            engine: Engine::new(leaked),
        }
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
}

fn lower_fwd(specs: &[serde_json::Value], g: &mut Graph) -> Vec<DirectedRule> {
    common::load_forward("case07_schema_constraints", specs.to_vec(), g)
}

// ═══════════════ 7a — NoTwoGlossaries ═══════════════

/// Doc → Glossary. L-Pattern hat NUR den Doc-Knoten ⇒ genau ein Match
/// pro Doc ⇒ genau ein Glossary. Strukturelle Identität: ein zweiter
/// Anwendungsversuch träfe dieselbe Ghost-Id (Duplikat).
#[test]
fn seven_a_at_most_one_glossary_per_doc() {
    let mut g = Graph::new();
    let doc = g.add_baseline("doc", "Doc");
    // Zwei Entries unter dem Doc (die im Paper mehrfach matchen könnten).
    for e in ["e1", "e2"] {
        let entry = g.add_baseline(e, "Entry");
        g.connect(doc, entry, Status::Solid);
    }
    let rule = serde_json::json!({
            "name": "Doc_2_Glossary", "rank": 10,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Doc"}
                ]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Glossary"}
                ]
            },
            "corrs": [
                {"type": "DocCorr", "left": "l0", "right": "r0", "role": "establishes"}
            ]
    });
    let rules = lower_fwd(&[rule], &mut g);
    let mut w = World::with(g, rules);
    w.sync();
    assert_eq!(
        w.live_count("Glossary"),
        1,
        "genau 1 Glossary (kein Doppel)"
    );
}

// ═══════════════ 7b — NoEmptyClass ═══════════════

/// ClassWithMethod → Doc. Das L-Pattern VERLANGT eine Method (methods-
/// Kante) — Pattern-Strenge statt NAC (PAC). Eine leere Class matcht
/// nicht ⇒ wird nicht übersetzt.
#[test]
fn seven_b_empty_class_is_not_translated() {
    let mut g = Graph::new();
    // Class A mit Method, Class B leer.
    let a = g.add_baseline("A", "Class");
    let m = g.add_baseline("m", "Method");
    g.connect(a, m, Status::Solid);
    let _b = g.add_baseline("B", "Class"); // leer
    let rule = serde_json::json!({
            "name": "ClassWithMethod_2_Doc", "rank": 10,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Class"},
                    {"name": "l1", "type": "Method"}
                ],
                "links": [["l0", "l1"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Doc"}
                ]
            },
            "corrs": [
                {"type": "ClassCorr", "left": "l0", "right": "r0", "role": "establishes"}
            ]
    });
    let rules = lower_fwd(&[rule], &mut g);
    let mut w = World::with(g, rules);
    w.sync();
    assert_eq!(w.live_count("Doc"), 1, "nur die nicht-leere Class → 1 Doc");
}

// ═══════════════ 7c — SameNameSameGlossaryEntry ═══════════════

fn methods_graph() -> (Graph, Vec<Id>) {
    let mut g = Graph::new();
    let mut ms = Vec::new();
    for ext in ["m1", "m2"] {
        let m = g.add_baseline(ext, "Method");
        let n = g.add_baseline(&format!("{ext}/name"), "methName");
        g.connect(m, n, Status::Solid);
        ms.push(m);
    }
    (g, ms)
}

/// 7c NAIV (der ehrliche Befund): Method(name) → GlossaryEntry(name),
/// per-Method. Zwei Methods „foo" ⇒ ZWEI GlossaryEntries — die
/// Default-Identität ist per-Element, nicht per-Name. Das ist die
/// wert-freie Identität (Weg C), kein Bug.
#[test]
fn seven_c_naive_gives_one_entry_per_method() {
    let (mut g, ms) = methods_graph();
    let rule = serde_json::json!({
            "name": "Method_2_Entry", "rank": 10,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Method"},
                    {"name": "l1", "type": "methName"}
                ],
                "links": [["l0", "l1"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "GlossaryEntry"},
                    {"name": "r1", "type": "geName"}
                ],
                "links": [["r0", "r1"]]
            },
            "corrs": [
                {"type": "MethCorr", "left": "l0", "right": "r0", "role": "establishes", "bindings": [{"left": "l1", "right": "r1"}]}
            ]
    });
    let rules = lower_fwd(&[rule], &mut g);
    let n1 = g.child_leaf_of_type(&ms[0], "methName").unwrap();
    let n2 = g.child_leaf_of_type(&ms[1], "methName").unwrap();
    let mut w = World::with(g, rules);
    w.vs.insert(n1, "foo");
    w.vs.insert(n2, "foo");
    w.sync();
    assert_eq!(
        w.live_count("GlossaryEntry"),
        2,
        "naiv: per-Method-Identität ⇒ 2 Entries (Default, wert-frei)"
    );
}

/// Ingest baut die NAMENS-GRUPPEN-EBENE: Methods gleichen Namens hängen
/// an EINER NameGroup (beim ersten Auftreten geschrieben, danach
/// wiederverwendet — positiv, kein Wert im Hash: die Gruppen-Id ist die
/// des Erst-Schreibens). Die NameGroup trägt den Namen (mgName).
fn methods_by_name(entries: &[(&str, &str)]) -> (Graph, ValueStore) {
    let mut g = Graph::new();
    let mut vs = ValueStore::default();
    let reg = g.add_baseline("mreg", "MethodRegister");
    let mut group_of: BTreeMap<&str, Id> = BTreeMap::new();
    for (i, (ext, name)) in entries.iter().enumerate() {
        let grp = match group_of.get(name) {
            Some(gr) => *gr,
            None => {
                let gr = g.add_baseline(&format!("grp/{name}"), "NameGroup");
                g.connect(reg, gr, Status::Solid);
                let gn = g.add_baseline(&format!("grp/{name}/name"), "mgName");
                g.connect(gr, gn, Status::Solid);
                vs.insert(gn, name.to_string());
                group_of.insert(name, gr);
                gr
            }
        };
        let m = g.add_baseline(ext, "Method");
        g.connect(grp, m, Status::Solid); // Method haengt an ihrer NameGroup
        let _ = i;
    }
    (g, vs)
}

/// NameGroup → GlossaryEntry (Name-Identität). Reines Bauen: eine Entry
/// pro Gruppe.
fn namegroup_to_entry() -> serde_json::Value {
    serde_json::json!({
            "name": "NameGroup_2_Entry", "rank": 20,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "NameGroup"},
                    {"name": "l1", "type": "mgName"}
                ],
                "links": [["l0", "l1"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "GlossaryEntry"},
                    {"name": "r1", "type": "geName"}
                ],
                "links": [["r0", "r1"]]
            },
            "corrs": [
                {"type": "NgCorr", "left": "l0", "right": "r0", "role": "establishes", "bindings": [{"left": "l1", "right": "r1"}]}
            ]
    })
}

/// Method unter NameGroup → MethCorr auf die (bestehende) GlossaryEntry.
/// NameGroup/GlossaryEntry sind Kontext. Reines Bauen — keine NAC, kein
/// Join. Zwei gleichnamige Methods erzeugen zwei MethCorr auf DIESELBE
/// Entry.
fn method_to_entry() -> serde_json::Value {
    serde_json::json!({
            "name": "Method_2_Entry", "rank": 10,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "NameGroup"},
                    {"name": "l1", "type": "Method"}
                ],
                "links": [["l0", "l1"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "GlossaryEntry"}
                ]
            },
            "corrs": [
                {"type": "NgCorr", "left": "l0", "right": "r0", "role": "references"},
                {"type": "MethCorr", "left": "l1", "right": "r0", "role": "establishes"}
            ]
    })
}

#[test]
fn seven_c_name_group_shares_one_entry() {
    // Zwei Methods "foo" (m1, m2), teilen die NameGroup "foo".
    let (mut g, vs) = methods_by_name(&[("m1", "foo"), ("m2", "foo")]);
    let rules = lower_fwd(&[namegroup_to_entry(), method_to_entry()], &mut g);
    let mut w = World::with_vs(g, rules, vs);
    w.sync();
    assert_eq!(
        w.live_count("GlossaryEntry"),
        1,
        "eine NameGroup 'foo' ⇒ EIN GlossaryEntry (kein Duplikat)"
    );
    // Beide Methods sind über MethCorr verbunden (2 Corrs, 1 Ziel).
    assert_eq!(w.live_count("MethCorr"), 2, "beide Methods verlinkt");
}

/// Gegenprobe: verschiedene Namen ⇒ getrennte Gruppen ⇒ getrennte
/// Entries (die Teilung folgt der Gruppen-Struktur, nicht einem Verbot).
#[test]
fn seven_c_different_names_get_separate_entries() {
    let (mut g, vs) = methods_by_name(&[("m1", "foo"), ("m2", "bar")]);
    let rules = lower_fwd(&[namegroup_to_entry(), method_to_entry()], &mut g);
    let mut w = World::with_vs(g, rules, vs);
    w.sync();
    assert_eq!(w.live_count("GlossaryEntry"), 2, "foo+bar: zwei Entries");
    assert_eq!(w.live_count("MethCorr"), 2, "je ein Corr");
}
