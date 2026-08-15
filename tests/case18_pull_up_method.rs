//! Case 18 — TTC 2015 (Peldszus, Kulcsár, Lochau): Java-Refactoring
//! „Pull-Up-Method" + „Create-Superclass". FRISCH aus dem Paper-Problem
//! abgeleitet (2026-07-17), ohne Vorkenntnisse der ersten Generation.
//!
//! ── ABGRENZUNG (bewusst) ───────────────────────────────────────────
//! Case 18 hat zwei Ebenen. Diese Datei baut NUR die erste:
//!   1. KONZEPTUELLER BX-KERN (hier): Pull-Up-Method / Create-Superclass
//!      als strukturelle Graph-Transformation auf einem ABSTRAKTEN
//!      Typgraph (Class/Method/Field als Knoten), rein in Rust auf der
//!      Engine.
//!   2. Operationelle Tool-Ebene (NICHT hier, separates Vorhaben):
//!      echte Java-Source via Eclipse JDT, javac-compile-Assertion,
//!      externe .ttc-Fixtures. Davon wird hier NICHTS gebaut.
//!
//! ── Das Paper-Problem (konzeptueller Kern) ─────────────────────────
//! Typgraph: Class(Name), Method(Name/Signatur), Field. Beziehungen
//! `extends` (Class→Superclass) und `contains` (Class→Method) sind
//! aus dem Problem geschnitten als KNOTEN reifiziert (nicht als anonyme Kanten). Zwei
//! Refactorings:
//!   - Pull-Up-Method: dieselbe Methode liegt in mehreren Subklassen
//!     einer gemeinsamen Superklasse → sie wird in die Superklasse
//!     hochgezogen (aus den Subklassen entfernt, oben einmal vorhanden).
//!   - Create-Superclass: zwei Klassen mit gemeinsamer Methode → eine
//!     neue gemeinsame Superklasse entsteht, die Methode wandert hoch.
//!     Constraints als NACs: nur public/protected-Methoden sind hochziehbar
//!     (private nicht).
//!
//! ── Die frische These, die hier getestet wird ───────────────────
//! Pull-Up-Method IST konzeptuell derselbe „Move Subtree" wie Case 1
//! (Re-Parenting), nur auf Method-Ebene: das Hochziehen ist ein MOVE
//! der `contains`-Beziehung (Subklasse→Superklasse). Die strukturelle,
//! wert-freie Ghost-Identität (Weg C) BEHAUPTET, das ID-stabil zu
//! leisten (kein Löschen+Neuerzeugen), sofern die Ziel-Identität der
//! Methode container-UNABHÄNGIG geschnitten ist.
//!
//! ── Schnitt aus dem Problem, nicht aus der ersten Generation ─────────────
//! Source-Typgraph  ↔  Target-Sicht (ein „Signatur-Doc"-Modell):
//!   - Class  ↔ ClassDoc   (Identität = f(class), container-unabhängig)
//!   - Method ↔ MethodDoc  (Identität = f(method), container-unabhängig
//!     — der Schlüssel für den ID-stabilen Move)
//!   - Contains(Class→Method) ↔ DContains(ClassDoc→MethodDoc)
//!     `extends` ist reifiziert (Source-Struktur), im Ziel nicht gespiegelt.
//!     Pull-Up = Contains(Sub→m) tombstonen + Contains(Super→m) hinzufügen
//!     (beides Knoten-Deltas) + die redundante Zwillings-Methode löschen.
//!     Erwartung: hochgezogene MethodDoc + manuelle Ziel-Ergänzung ÜBERLEBEN
//!     ID-stabil; nur die DContains-Beziehung wandert.

mod common;

use std::collections::BTreeMap;

use seesaw_tgg::engine::{DeltaDomain, Engine};
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::Status;
use seesaw_tgg::plan::DirectedRule;

type Id = seesaw_tgg::ident::GhostId;

// ═══════════════ TGG aus dem Problem ═════════════

/// Jede Class ↔ ein ClassDoc. Identität container-UNABHÄNGIG.
fn class_to_doc() -> serde_json::Value {
    serde_json::json!({
            "name": "Class_2_Doc", "rank": 100,
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
                    {"name": "r0", "type": "ClassDoc"},
                    {"name": "r1", "type": "cdName"}
                ],
                "links": [["r0", "r1"]]
            },
            "corrs": [
                {"type": "ClassDocCorr", "left": "l0", "right": "r0", "role": "establishes", "bindings": [{"left": "l1", "right": "r1"}]}
            ]
    })
}

/// Jede Method ↔ ein MethodDoc. Identität container-UNABHÄNGIG — der
/// Schlüssel für Least-Changing beim Pull-Up (Method wird bewegt, nicht
/// zerstört+neu erzeugt).
fn method_to_doc() -> serde_json::Value {
    serde_json::json!({
            "name": "Method_2_Doc", "rank": 90,
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
                    {"name": "r0", "type": "MethodDoc"},
                    {"name": "r1", "type": "mdContent"}
                ],
                "links": [["r0", "r1"]]
            },
            "corrs": [
                {"type": "MethDocCorr", "left": "l0", "right": "r0", "role": "establishes", "bindings": [{"left": "l1", "right": "r1"}]}
            ]
    })
}

/// Containment (Class enthält Method) ↔ Doc-Containment. Beide Enden
/// sind Kontext (ihre Docs existieren schon). Erzeugt NUR die
/// DContains-Beziehung — Move = diese Beziehung umhängen.
fn contains_to_dcontains() -> serde_json::Value {
    serde_json::json!({
            "name": "Contains_2_DContains", "rank": 80,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Class"},
                    {"name": "l1", "type": "Contains"},
                    {"name": "l2", "type": "Method"}
                ],
                "links": [["l0", "l1"], ["l1", "l2"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "ClassDoc"},
                    {"name": "r1", "type": "DContains"},
                    {"name": "r2", "type": "MethodDoc"}
                ],
                "links": [["r0", "r1"], ["r1", "r2"]]
            },
            "corrs": [
                {"type": "ClassDocCorr", "left": "l0", "right": "r0", "role": "references"},
                {"type": "MethDocCorr", "left": "l2", "right": "r2", "role": "references"},
                {"type": "ContCorr", "left": "l1", "right": "r1", "role": "establishes"}
            ]
    })
}

/// Constraint-Regel (Test c): eine Method in einer Subklasse (die eine
/// Superklasse `extends`) ist HOCHZIEHBAR ⇒ erhält eine PullUp-Lizenz.
/// Je EINE Bau-Regel pro zulässiger Sichtbarkeit (public, protected),
/// beide bauen dieselbe PullUp-Lizenz (PullCorr an der Method → gleiche
/// Id, Saturation). POSITIV über den visibility-Wert (Equals, rückwärts-
/// erzeugbar) statt einer Private-Abwesenheits-NAC. Eine private Method
/// fittet KEINE der Regeln — keine Lizenz, ohne Verbot. TGG-native
/// Fassung von `assertTrue/assertFalse(executable_pum)`.
fn pull_up_eligible(vis: &str) -> serde_json::Value {
    serde_json::json!({
        "name": format!("PullUp_Eligible_{vis}"), "rank": 50,
        "left": {
            "anchor": "sub",
            "nodes": [
                {"name": "sub", "type": "Class"},
                {"name": "ext", "type": "Extends"},
                {"name": "super", "type": "Class"},
                {"name": "cont", "type": "Contains"},
                {"name": "meth", "type": "Method"},
                {"name": "mname", "type": "methName"},
                {"name": "vis", "type": "visibility",
                 "predicate": {"kind": "equals", "value": vis}, "constant": vis}
            ],
            "links": [["sub", "ext"], ["ext", "super"], ["sub", "cont"],
                      ["cont", "meth"], ["meth", "mname"], ["meth", "vis"]]
        },
        "right": {
            "anchor": "pull",
            "nodes": [{"name": "pull", "type": "PullUp"}]
        },
        "corrs": [
            {"type": "PullCorr", "left": "meth", "right": "pull", "role": "establishes"}
        ]
    })
}

/// Die zwei zulässigen-Sichtbarkeits-Bauregeln.
fn pull_up_rules() -> Vec<serde_json::Value> {
    vec![pull_up_eligible("public"), pull_up_eligible("protected")]
}

// ═══════════════ Sync-World ═══════════════

struct Sync {
    g: Graph,
    vs: ValueStore,
    engine: Engine<'static>,
}

impl Sync {
    /// Baut die gerichteten Regeln IN g (Typ-Interning) und übernimmt
    /// die schon befüllte ValueStore. `forward_only` = nur Vorwärts
    /// (Translation ohne Rück-Delta, für die Constraint-Regel).
    fn new(mut g: Graph, vs: ValueStore, specs: &[serde_json::Value], forward_only: bool) -> Self {
        let dr: Vec<DirectedRule> = if forward_only {
            common::load_forward("case18_pull_up_method", specs.to_vec(), &mut g)
        } else {
            common::load("case18_pull_up_method", specs.to_vec(), &mut g)
        };
        let leaked: &'static [DirectedRule] = Box::leak(dr.into_boxed_slice());
        let mut sync = Sync {
            g,
            vs,
            engine: Engine::new(leaked),
        };
        sync.engine.admit_delta(&[DeltaDomain::Source]);
        sync
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

    fn leaf_value(&self, node: &Id, leaf_typ: &str) -> String {
        self.g
            .child_leaf_of_type(node, leaf_typ)
            .and_then(|l| self.g.resolve_value(&l, &self.vs))
            .unwrap_or_default()
    }

    fn corr_partner(&self, node: &Id, corr_typ: &str, want_typ: &str) -> Option<Id> {
        let ct = self.g.types.lookup(corr_typ)?;
        let wt = self.g.types.lookup(want_typ)?;
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

    fn class_doc(&self, class: &Id) -> Option<Id> {
        self.corr_partner(class, "ClassDocCorr", "ClassDoc")
    }
    fn method_doc(&self, method: &Id) -> Option<Id> {
        self.corr_partner(method, "MethDocCorr", "MethodDoc")
    }

    /// Lebendige DContains-Beziehung parent_doc ⊇ child_doc?
    fn dcontains(&self, parent_doc: &Id, child_doc: &Id) -> Option<Id> {
        let dt = self.g.types.lookup("DContains")?;
        for p in self.g.parts_by_other_type(parent_doc, dt) {
            if !p.outgoing || !self.conn_alive(&p.connection) || !self.node_alive(&p.other) {
                continue;
            }
            for q in self.g.parts(&p.other) {
                if q.outgoing && q.other == *child_doc && self.conn_alive(&q.connection) {
                    return Some(p.other);
                }
            }
        }
        None
    }

    /// Contains-Knoten (parent enthält child) im Source, lebendig?
    fn contains_node(&self, parent: &Id, child: &Id) -> Option<Id> {
        let ct = self.g.types.lookup("Contains")?;
        for p in self.g.parts_by_other_type(parent, ct) {
            if !p.outgoing || !self.conn_alive(&p.connection) || !self.node_alive(&p.other) {
                continue;
            }
            for q in self.g.parts(&p.other) {
                if q.outgoing && q.other == *child && self.conn_alive(&q.connection) {
                    return Some(p.other);
                }
            }
        }
        None
    }

    fn remove_node(&mut self, id: Id) {
        self.g.set_node_status(&id, Status::Tombstone);
        self.engine.element_removed(&id);
        self.engine.element_deleted(&mut self.g, &id);
    }

    /// Manuelle Ziel-Ergänzung: Notiz an einem MethodDoc — muss den
    /// Pull-Up überleben (Informationsverlust-Test).
    fn annotate(&mut self, doc: Id, note: &str) {
        let a = self.g.add_baseline(&format!("note/{note}"), "Note");
        self.g.connect(doc, a, Status::Solid);
        self.vs.insert(a, note);
    }
    fn note_of(&self, doc: &Id) -> Option<String> {
        let nt = self.g.types.lookup("Note")?;
        self.g
            .parts_by_other_type(doc, nt)
            .filter(|p| p.outgoing && self.conn_alive(&p.connection) && self.node_alive(&p.other))
            .find_map(|p| self.g.resolve_value(&p.other, &self.vs))
    }
}

// ═══════════════ Modell-Bausteine ═══════════════

fn add_class(g: &mut Graph, vs: &mut ValueStore, ext: &str, name: &str) -> Id {
    let c = g.add_baseline(ext, "Class");
    let n = g.add_baseline(&format!("{ext}/name"), "className");
    g.connect(c, n, Status::Solid);
    vs.insert(n, name);
    c
}

fn add_method(g: &mut Graph, vs: &mut ValueStore, ext: &str, name: &str) -> Id {
    add_method_vis(g, vs, ext, name, "public")
}

/// Method mit POSITIVER visibility — jede Method traegt IMMER eine
/// (public/protected/private), nicht einen Private-Marker-oder-nichts.
fn add_method_vis(g: &mut Graph, vs: &mut ValueStore, ext: &str, name: &str, vis: &str) -> Id {
    let m = g.add_baseline(ext, "Method");
    let n = g.add_baseline(&format!("{ext}/name"), "methName");
    g.connect(m, n, Status::Solid);
    vs.insert(n, name);
    let v = g.add_baseline(&format!("{ext}/vis"), "visibility");
    g.connect(m, v, Status::Solid);
    vs.insert(v, vis);
    m
}

/// Reifiziertes contains: parent → Contains → child.
fn add_contains(g: &mut Graph, parent: Id, child: Id, ext: &str) -> Id {
    let c = g.add_baseline(ext, "Contains");
    g.connect(parent, c, Status::Solid);
    g.connect(c, child, Status::Solid);
    c
}

/// Reifiziertes extends: sub → Extends → super.
fn add_extends(g: &mut Graph, sub: Id, sup: Id, ext: &str) -> Id {
    let e = g.add_baseline(ext, "Extends");
    g.connect(sub, e, Status::Solid);
    g.connect(e, sup, Status::Solid);
    e
}

// ═══════════════ Szene 1: Pull-Up-Method ═══════════════

struct PullUpScene {
    handles: BTreeMap<&'static str, Id>,
}

/// Super ⊳ {SubA, SubB}; SubA enthält mA=foo, SubB enthält mB=foo.
/// (extends reifiziert.)
fn build_pullup(g: &mut Graph, vs: &mut ValueStore) -> PullUpScene {
    let sup = add_class(g, vs, "Super", "Super");
    let sub_a = add_class(g, vs, "SubA", "SubA");
    let sub_b = add_class(g, vs, "SubB", "SubB");
    add_extends(g, sub_a, sup, "ext/SubA");
    add_extends(g, sub_b, sup, "ext/SubB");
    let m_a = add_method(g, vs, "mA", "foo");
    let m_b = add_method(g, vs, "mB", "foo");
    let cont_a = add_contains(g, sub_a, m_a, "cont/SubA/foo");
    let cont_b = add_contains(g, sub_b, m_b, "cont/SubB/foo");
    let mut handles = BTreeMap::new();
    handles.insert("Super", sup);
    handles.insert("SubA", sub_a);
    handles.insert("SubB", sub_b);
    handles.insert("mA", m_a);
    handles.insert("mB", m_b);
    handles.insert("contA", cont_a);
    handles.insert("contB", cont_b);
    PullUpScene { handles }
}

/// Pull-Up: mA (foo) aus SubA in Super hochziehen (Contains(SubA→mA)
/// tombstonen, Contains(Super→mA) hinzufügen) + die redundante
/// Zwillings-Methode mB samt Containment löschen.
fn pull_up_foo(w: &mut Sync, sc: &PullUpScene) {
    w.engine.admit_delta(&[DeltaDomain::Source]);
    let sup = sc.handles["Super"];
    let m_a = sc.handles["mA"];
    let m_b = sc.handles["mB"];
    let cont_a = sc.handles["contA"];
    let cont_b = sc.handles["contB"];
    // mA nach Super MOVEN.
    w.remove_node(cont_a);
    add_contains(&mut w.g, sup, m_a, "cont/Super/foo");
    // Zwilling mB entfernen (redundant nach Pull-Up).
    w.remove_node(cont_b);
    w.remove_node(m_b);
}

#[test]
fn initial_sync_builds_docs() {
    let mut g = Graph::new();
    let mut vs = ValueStore::default();
    let _sc = build_pullup(&mut g, &mut vs);
    let mut w = Sync::new(
        g,
        vs,
        &[class_to_doc(), method_to_doc(), contains_to_dcontains()],
        false,
    );
    w.sync();
    assert_eq!(
        w.live_count("ClassDoc"),
        3,
        "3 ClassDocs (Super, SubA, SubB)"
    );
    assert_eq!(w.live_count("MethodDoc"), 2, "2 MethodDocs (mA, mB)");
    assert_eq!(w.live_count("DContains"), 2, "2 Doc-Containments");
}

/// DER Paper-Test (a)+(d): Pull-Up-Method, ID-stabil.
/// These: die hochgezogene MethodDoc + manuelle Notiz überleben
/// ID-stabil; die DContains-Beziehung wandert von SubA-Doc zu Super-Doc.
/// Kein Löschen+Neuerzeugen, keine Short-Cut-Rule nötig.
#[test]
fn pull_up_is_id_stable() {
    let mut g = Graph::new();
    let mut vs = ValueStore::default();
    let sc = build_pullup(&mut g, &mut vs);
    let mut w = Sync::new(
        g,
        vs,
        &[class_to_doc(), method_to_doc(), contains_to_dcontains()],
        false,
    );
    w.sync();

    let super_doc = w.class_doc(&sc.handles["Super"]).expect("Super-Doc");
    let sub_a_doc = w.class_doc(&sc.handles["SubA"]).expect("SubA-Doc");
    let m_a_doc_before = w.method_doc(&sc.handles["mA"]).expect("mA-Doc vor Pull-Up");
    // (d) Manuelle Ziel-Ergänzung an mA-Doc — muss den Pull-Up überleben.
    w.annotate(m_a_doc_before, "handmade");
    // Vor dem Pull-Up: SubA-Doc enthält mA-Doc, Super-Doc nicht.
    assert!(
        w.dcontains(&sub_a_doc, &m_a_doc_before).is_some(),
        "SubA_doc ⊇ mA_doc (vor)"
    );
    assert!(
        w.dcontains(&super_doc, &m_a_doc_before).is_none(),
        "Super_doc ⊉ mA_doc (vor)"
    );

    pull_up_foo(&mut w, &sc);
    w.sync();

    // (a) mA-Doc IDENTISCH erhalten (nicht neu erzeugt).
    let m_a_doc_after = w
        .method_doc(&sc.handles["mA"])
        .expect("mA-Doc nach Pull-Up");
    assert_eq!(
        m_a_doc_after, m_a_doc_before,
        "mA-Doc ID-stabil über Pull-Up"
    );
    // (d) Manuelle Notiz überlebt (kein Informationsverlust).
    assert_eq!(
        w.note_of(&m_a_doc_after).as_deref(),
        Some("handmade"),
        "manuelle Ziel-Ergänzung überlebt den Pull-Up"
    );
    assert_eq!(w.leaf_value(&m_a_doc_after, "mdContent"), "foo");
    // Beziehung gewandert: Super_doc ⊇ mA_doc, SubA_doc ⊉ mA_doc.
    assert!(
        w.dcontains(&super_doc, &m_a_doc_after).is_some(),
        "Super_doc ⊇ mA_doc (neu)"
    );
    assert!(
        w.dcontains(&sub_a_doc, &m_a_doc_after).is_none(),
        "SubA_doc ⊉ mA_doc (gelöst)"
    );
    // Subklassen enthalten die Methode nicht mehr (Source-Ebene).
    assert!(
        w.contains_node(&sc.handles["SubA"], &sc.handles["mA"])
            .is_none(),
        "SubA enthält foo nicht mehr"
    );
    assert!(!w.node_alive(&sc.handles["mB"]), "Zwilling mB gelöscht");
    // Genau EINE Methode foo, einmal in Super vorhanden.
    assert_eq!(
        w.live_count("MethodDoc"),
        1,
        "genau 1 MethodDoc (hochgezogen)"
    );
    assert_eq!(
        w.live_count("DContains"),
        1,
        "genau 1 Doc-Containment (unter Super)"
    );
}

// ═══════════════ Szene 2: Create-Superclass ═══════════════

struct CreateSuperScene {
    handles: BTreeMap<&'static str, Id>,
}

/// Zwei unverwandte Klassen C1, C2, jede enthält foo (m1 bzw. m2).
/// Keine gemeinsame Superklasse.
fn build_create_super(g: &mut Graph, vs: &mut ValueStore) -> CreateSuperScene {
    let c1 = add_class(g, vs, "C1", "C1");
    let c2 = add_class(g, vs, "C2", "C2");
    let m1 = add_method(g, vs, "m1", "foo");
    let m2 = add_method(g, vs, "m2", "foo");
    let cont1 = add_contains(g, c1, m1, "cont/C1/foo");
    let cont2 = add_contains(g, c2, m2, "cont/C2/foo");
    let mut handles = BTreeMap::new();
    handles.insert("C1", c1);
    handles.insert("C2", c2);
    handles.insert("m1", m1);
    handles.insert("m2", m2);
    handles.insert("cont1", cont1);
    handles.insert("cont2", cont2);
    CreateSuperScene { handles }
}

/// DER Paper-Test (b): Create-Superclass. Eine neue gemeinsame
/// Superklasse S entsteht, C1/C2 `extends` S, foo (m1) wandert hoch,
/// der Zwilling m2 verschwindet. Erwartung: neue ClassDoc für S,
/// m1-Doc ID-stabil, jetzt DContained unter S-Doc.
#[test]
fn create_superclass_pulls_method_up() {
    let mut g = Graph::new();
    let mut vs = ValueStore::default();
    let sc = build_create_super(&mut g, &mut vs);
    let mut w = Sync::new(
        g,
        vs,
        &[class_to_doc(), method_to_doc(), contains_to_dcontains()],
        false,
    );
    w.sync();
    assert_eq!(w.live_count("ClassDoc"), 2, "vorher 2 ClassDocs (C1, C2)");
    let m1_doc_before = w.method_doc(&sc.handles["m1"]).expect("m1-Doc vor");

    // Refactoring: neue Superklasse S; C1, C2 extends S; m1 nach S
    // hochziehen; Zwilling m2 löschen.
    w.engine.admit_delta(&[DeltaDomain::Source]);
    let s = add_class(&mut w.g, &mut w.vs, "S", "S");
    add_extends(&mut w.g, sc.handles["C1"], s, "ext/C1");
    add_extends(&mut w.g, sc.handles["C2"], s, "ext/C2");
    w.remove_node(sc.handles["cont1"]);
    add_contains(&mut w.g, s, sc.handles["m1"], "cont/S/foo");
    w.remove_node(sc.handles["cont2"]);
    w.remove_node(sc.handles["m2"]);
    w.sync();

    // Neue Superklasse hat ein ClassDoc.
    assert_eq!(
        w.live_count("ClassDoc"),
        3,
        "nachher 3 ClassDocs (C1, C2, S)"
    );
    let s_doc = w.class_doc(&s).expect("S-Doc (neu erzeugt)");
    assert_eq!(w.leaf_value(&s_doc, "cdName"), "S");
    // m1-Doc ID-stabil, jetzt unter S-Doc.
    let m1_doc_after = w.method_doc(&sc.handles["m1"]).expect("m1-Doc nach");
    assert_eq!(m1_doc_after, m1_doc_before, "m1-Doc ID-stabil");
    assert!(
        w.dcontains(&s_doc, &m1_doc_after).is_some(),
        "S_doc ⊇ m1_doc (hochgezogen)"
    );
    // Genau eine Methode foo überlebt.
    assert!(!w.node_alive(&sc.handles["m2"]), "Zwilling m2 gelöscht");
    assert_eq!(
        w.live_count("MethodDoc"),
        1,
        "genau 1 MethodDoc (hochgezogen)"
    );
}

// ═══════════════ Szene 3: Constraint-NAC ═══════════════

/// Baut Super ⊳ Sub; Sub enthält Method foo mit POSITIVER visibility
/// (private=true → "private", sonst "public"). Liefert (Graph, vs).
fn build_visibility_scene(private: bool) -> (Graph, ValueStore) {
    let mut g = Graph::new();
    let mut vs = ValueStore::default();
    let sup = add_class(&mut g, &mut vs, "Super", "Super");
    let sub = add_class(&mut g, &mut vs, "Sub", "Sub");
    add_extends(&mut g, sub, sup, "ext/Sub");
    let vis = if private { "private" } else { "public" };
    let m = add_method_vis(&mut g, &mut vs, "m", "foo", vis);
    add_contains(&mut g, sub, m, "cont/Sub/foo");
    (g, vs)
}

/// DER Paper-Test (c): Visibility POSITIV. Eine public/protected-Method
/// fittet die Bau-Regel (Lizenz entsteht); eine private Method fittet
/// sie NICHT (ihr visibility-Wert matcht das Muster nicht) — keine
/// Lizenz, ohne Verbot. TGG-native Fassung von
/// `assertTrue/assertFalse(executable_pum)`.
#[test]
fn visibility_gates_pull_up_positively() {
    // public: fittet ⇒ Lizenz entsteht.
    let (g, vs) = build_visibility_scene(false);
    let mut w = Sync::new(g, vs, &pull_up_rules(), true);
    w.sync();
    assert_eq!(
        w.live_count("PullUp"),
        1,
        "public-Methode ist hochziehbar (Lizenz erteilt)"
    );

    // private: fittet nicht ⇒ KEINE Lizenz (kein Verbot, die Regel baut
    // schlicht nicht).
    let (g, vs) = build_visibility_scene(true);
    let mut w = Sync::new(g, vs, &pull_up_rules(), true);
    w.sync();
    assert_eq!(
        w.live_count("PullUp"),
        0,
        "private-Methode fittet die Bau-Regel nicht (keine Lizenz)"
    );
}
