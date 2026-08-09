//! Case 2 — LMCS 2024 (Fritsche, Kosiol, Lauer, Möller, Schürr):
//! „Advanced Model Consistency Restoration with Higher-Order Short-Cut
//! Rules". FRISCH aus dem Paper-Problem, ohne Vorkenntnisse der ersten Generation
//! (2026-07-16).
//!
//! ── Das Paper-Problem ──────────────────────────────────────────────
//! MULTI-EDIT-Reparatur: mehrere verflochtene Edits gleichzeitig
//! (Struktur-Umbau + Re-Typing) sollen least-changing propagiert
//! werden. Der Paper-Claim: pre-computed Short-Cut-Rules reichen NICHT,
//! man braucht zur Laufzeit ILP-synthetisierte higher-order Short-Cut-
//! Rules.
//!
//! ── Die frische These ───────────────────────────────────────────
//! Die Engine hasht KEINE Werte in Identitäten (Weg C). Ein Re-Typing ändert
//! daher die strukturelle Identität der Ziel-Elemente NICHT — die
//! zwischen alt- und neu-Typ GETEILTEN Komponenten werden reklamiert,
//! nur die differierenden fallen weg. Kein ILP, keine higher-order
//! Regeln — wenn das TGG aus dem Problem geschnitten geschnitten ist (Komponenten-
//! Identität house-basiert, nicht typ-basiert).
//!
//! Domäne (Terrace-Houses): House{type ∈ Nook|Villa|Cube} —next→ House.
//! Konstruktion je Typ: Nook = nur Construction; Villa = +Floor+Roof;
//! Cube = +Cellar+Floor+Roof. Ziel-Seite: Construction mit
//! Sub-Komponenten, nextConstr.
//!
//! Schnitt aus dem Problem: `next` reifiziert (NextRel). EINE
//! `House→Construction`-Regel (typ-UNABHÄNGIG). Je Komponente eine
//! typ-gefilterte Regel — die Filter ÜBERLAPPEN (Floor/Roof für
//! Villa∪Cube), so dass ein Cube→Villa-Re-Typing Floor+Roof erhält und
//! nur den Cellar verliert.

mod common;

use seesaw_tgg::engine::Engine;
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::Status;
use seesaw_tgg::plan::DirectedRule;

type Id = seesaw_tgg::ident::GhostId;

// ═══════════════ TGG aus dem Problem ═══════════════

/// Jedes House ↔ eine Construction (typ-UNABHÄNGIG). Identität =
/// f(house) — Basis für Least-Changing beim Re-Typing.
fn house_to_construction() -> serde_json::Value {
    serde_json::json!({
            "name": "House_2_Construction", "rank": 100,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "House"}
                ]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Construction"}
                ]
            },
            "corrs": [
                {"type": "HouseConstr", "left": "l0", "right": "r0", "role": "establishes"}
            ]
    })
}

/// Komponenten-Regel: House mit Typ == `for_type` ↔ Sub-Komponente in
/// der Construction. Ein Typ, der Floor+Roof MIT einem anderen teilt
/// (Villa∪Cube), bekommt je eine Regel pro Typ — ihre Match-Refs
/// ([House, houseType-Blatt]) sind identisch (der WERT ist kein Ref),
/// also erzeugen sie dieselbe Komponenten-Identität. Genau das liefert
/// Least-Changing beim Re-Typing (die geteilte Komponente wird
/// reklamiert, nicht neu erzeugt). `Equals` statt `Regex`, damit das
/// (ungenutzte) Backward-Lowering kompiliert.
fn component_rule(name: &str, comp_typ: &str, corr: &str, for_type: &str) -> serde_json::Value {
    serde_json::json!({
            "name": name, "rank": 90,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "House"},
                    {"name": "l1", "type": "houseType",
                     "predicate": {"kind": "equals", "value": for_type},
                     "constant": for_type}
                ],
                "links": [["l0", "l1"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Construction"},
                    {"name": "r1", "type": comp_typ}
                ],
                "links": [["r0", "r1"]]
            },
            "corrs": [
                {"type": "HouseConstr", "left": "l0", "right": "r0", "role": "references"},
                {"type": corr, "left": "l0", "right": "r1", "role": "establishes"}
            ]
    })
}

/// next(h1→h2) ↔ nextConstr(c1→c2); beide Constructions Kontext.
fn next_to_nextconstr() -> serde_json::Value {
    serde_json::json!({
            "name": "Next_2_NextConstr", "rank": 80,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "House"},
                    {"name": "l1", "type": "NextRel"},
                    {"name": "l2", "type": "House"}
                ],
                "links": [["l0", "l1"], ["l1", "l2"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Construction"},
                    {"name": "r1", "type": "NextConstrRel"},
                    {"name": "r2", "type": "Construction"}
                ],
                "links": [["r0", "r1"], ["r1", "r2"]]
            },
            "corrs": [
                {"type": "HouseConstr", "left": "l0", "right": "r0", "role": "references"},
                {"type": "HouseConstr", "left": "l2", "right": "r2", "role": "references"},
                {"type": "NextCorr", "left": "l1", "right": "r1", "role": "establishes"}
            ]
    })
}

fn ruleset(g: &mut Graph) -> Vec<DirectedRule> {
    let specs = [
        house_to_construction(),
        // Cellar nur für Cube. Floor+Roof für Villa UND Cube (je eine
        // Regel pro Typ; geteilte Identität ⇒ Least-Changing).
        component_rule("Cellar_Cube", "Cellar", "CellarCorr", "Cube"),
        component_rule("Floor_Villa", "Floor", "FloorCorr", "Villa"),
        component_rule("Floor_Cube", "Floor", "FloorCorr", "Cube"),
        component_rule("Roof_Villa", "SaddleRoof", "RoofCorr", "Villa"),
        component_rule("Roof_Cube", "SaddleRoof", "RoofCorr", "Cube"),
        next_to_nextconstr(),
    ];
    // Forward-Sync-Szenario (House-Welt → Construction-Welt). Der
    // Typ-Filter der Komponenten-Regeln ist eine Gleichheit auf einem
    // Blatt, das die Rueckrichtung erzeugen wuerde; er traegt deshalb
    // zusaetzlich `constant` mit demselben Wert (die einzige Form, die
    // das Format auf einem erzeugten Knoten zulaesst). Gefahren wird nur
    // vorwaerts.
    common::load_forward("case02_higher_order_edit", specs.to_vec(), g)
}

// ═══════════════ World ═══════════════

struct World {
    g: Graph,
    vs: ValueStore,
    engine: Engine<'static>,
    h1: Id,
    h2: Id,
    h3: Id,
    h3_type: Id,
    next12: Id,
    next23: Id,
}

impl World {
    /// h1(Nook) --next--> h2(Villa) --next--> h3(Cube). `next`
    /// reifiziert (NextRel-Knoten).
    fn new() -> Self {
        let mut g = Graph::new();
        let mk_house = |g: &mut Graph, ext: &str| -> (Id, Id) {
            let h = g.add_baseline(ext, "House");
            let t = g.add_baseline(&format!("{ext}/type"), "houseType");
            g.connect(h, t, Status::Solid);
            (h, t)
        };
        let (h1, t1) = mk_house(&mut g, "h1");
        let (h2, t2) = mk_house(&mut g, "h2");
        let (h3, t3) = mk_house(&mut g, "h3");
        let next12 = g.add_baseline("next/h1/h2", "NextRel");
        g.connect(h1, next12, Status::Solid);
        g.connect(next12, h2, Status::Solid);
        let next23 = g.add_baseline("next/h2/h3", "NextRel");
        g.connect(h2, next23, Status::Solid);
        g.connect(next23, h3, Status::Solid);

        let rules: &'static [DirectedRule] = Box::leak(ruleset(&mut g).into_boxed_slice());
        let mut w = World {
            g,
            vs: ValueStore::default(),
            engine: Engine::new(rules),
            h1,
            h2,
            h3,
            h3_type: t3,
            next12,
            next23,
        };
        w.vs.insert(t1, "Nook");
        w.vs.insert(t2, "Villa");
        w.vs.insert(t3, "Cube");
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

    fn nodes_of(&self, typ: &str) -> Vec<Id> {
        self.g
            .types
            .lookup(typ)
            .map(|t| self.g.nodes_of_type(t).map(|n| n.id).collect())
            .unwrap_or_default()
    }

    fn live_count(&self, typ: &str) -> usize {
        self.nodes_of(typ)
            .into_iter()
            .filter(|n| self.node_alive(n))
            .count()
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

    fn construction_of(&self, house: &Id) -> Option<Id> {
        self.corr_partner(house, "HouseConstr", "Construction")
    }

    fn component_of(&self, house: &Id, corr: &str, comp: &str) -> Option<Id> {
        self.corr_partner(house, corr, comp)
    }

    /// nextConstr zwischen zwei Constructions?
    fn nextconstr(&self, c1: &Id, c2: &Id) -> Option<Id> {
        let rt = self.g.types.lookup("NextConstrRel")?;
        for p in self.g.parts_by_other_type(c1, rt) {
            if !p.outgoing || !self.conn_alive(&p.connection) || !self.node_alive(&p.other) {
                continue;
            }
            for q in self.g.parts(&p.other) {
                if q.outgoing && q.other == *c2 && self.conn_alive(&q.connection) {
                    return Some(p.other);
                }
            }
        }
        None
    }

    fn remove_node(&mut self, id: Id) {
        self.g.set_node_status(&id, Status::Tombstone);
        self.engine.element_removed(&id);
        self.engine.retract_for(&mut self.g, &id);
    }

    /// SetAttr auf einem Blatt: Wert ändern + Match-Neubewertung
    /// anstoßen (das Blatt bleibt derselbe Knoten, nur der Wert
    /// ändert sich — die Engine hasht keine Werte, also bleiben abhängige
    /// Identitäten strukturell stabil).
    fn set_type(&mut self, type_leaf: Id, value: &str) {
        self.vs.insert(type_leaf, value);
        self.engine.element_removed(&type_leaf);
        self.engine.retract_for(&mut self.g, &type_leaf);
    }

    /// Manuelle Ziel-Ergänzung (Informationsverlust-Test).
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
fn initial_sync_builds_constructions() {
    let mut w = World::new();
    w.sync();
    // Nook: nur Construction. Villa: +Floor+Roof. Cube: +Cellar+Floor+Roof.
    assert_eq!(w.live_count("Construction"), 3, "3 Constructions");
    assert_eq!(w.live_count("Cellar"), 1, "1 Cellar (nur Cube=h3)");
    assert_eq!(w.live_count("Floor"), 2, "2 Floors (Villa=h2, Cube=h3)");
    assert_eq!(w.live_count("SaddleRoof"), 2, "2 Roofs (h2, h3)");
    // h1 (Nook) hat keine Sub-Komponenten.
    assert!(
        w.component_of(&w.h1, "FloorCorr", "Floor").is_none(),
        "Nook: kein Floor"
    );
    // h3 (Cube) hat alle drei.
    assert!(
        w.component_of(&w.h3, "CellarCorr", "Cellar").is_some(),
        "Cube: Cellar"
    );
    assert!(
        w.component_of(&w.h3, "FloorCorr", "Floor").is_some(),
        "Cube: Floor"
    );
    assert!(
        w.component_of(&w.h3, "RoofCorr", "SaddleRoof").is_some(),
        "Cube: Roof"
    );
}

/// DER Paper-Test: Higher-order Multi-Edit (h2 löschen, Sequenz
/// umbauen, h3 Cube→Villa re-typen), LEAST-CHANGING.
/// These: h3's Construction + Floor + Roof ID-STABIL erhalten (Villa
/// teilt sie mit Cube), NUR der Cellar fällt weg; h2 + Sub gelöscht;
/// nextConstr wandert c1→c3. Kein ILP, keine higher-order Regel.
#[test]
fn higher_order_edit_is_least_changing() {
    let mut w = World::new();
    w.sync();
    let c1 = w.construction_of(&w.h1).expect("c1");
    let c3_before = w.construction_of(&w.h3).expect("c3");
    let floor3_before = w
        .component_of(&w.h3, "FloorCorr", "Floor")
        .expect("Floor(h3)");
    let roof3_before = w
        .component_of(&w.h3, "RoofCorr", "SaddleRoof")
        .expect("Roof(h3)");
    // manuelle Ergänzung an h3's Floor — muss das Re-Typing überleben.
    w.annotate(floor3_before, "renovated");

    // Multi-Edit (higher-order): alles zusammen, dann EIN sync.
    w.remove_node(w.next12); //  next h1→h2 weg
    w.remove_node(w.next23); //  next h2→h3 weg
    let h2 = w.h2;
    w.remove_node(h2); //        h2 gelöscht
    w.set_type(w.h3_type, "Villa"); // h3 Cube→Villa
    let new_next = w.g.add_baseline("next/h1/h3", "NextRel"); // h1→h3 neu
    w.g.connect(w.h1, new_next, Status::Solid);
    w.g.connect(new_next, w.h3, Status::Solid);
    w.sync();

    // Least-Changing an h3: Construction + Floor + Roof ID-STABIL.
    let c3_after = w.construction_of(&w.h3).expect("c3 bleibt");
    assert_eq!(c3_after, c3_before, "c3 ID-stabil über Re-Typing");
    let floor3_after = w
        .component_of(&w.h3, "FloorCorr", "Floor")
        .expect("Floor(h3) bleibt");
    assert_eq!(
        floor3_after, floor3_before,
        "Floor(h3) ID-stabil (Villa teilt ihn)"
    );
    let roof3_after = w
        .component_of(&w.h3, "RoofCorr", "SaddleRoof")
        .expect("Roof(h3) bleibt");
    assert_eq!(roof3_after, roof3_before, "Roof(h3) ID-stabil");
    // Manuelle Ergänzung überlebt (kein Informationsverlust).
    assert_eq!(
        w.note_of(&floor3_after).as_deref(),
        Some("renovated"),
        "Notiz überlebt Re-Typing"
    );
    // NUR der Cellar fällt weg (Villa hat keinen).
    assert!(
        w.component_of(&w.h3, "CellarCorr", "Cellar").is_none(),
        "Cellar(h3) weg (Villa)"
    );
    assert_eq!(w.live_count("Cellar"), 0, "0 Cellar");
    // h2 + Sub-Komponenten gelöscht.
    assert!(!w.node_alive(&h2), "h2 tombstone");
    assert_eq!(w.live_count("Construction"), 2, "2 Constructions (h1, h3)");
    assert_eq!(w.live_count("Floor"), 1, "1 Floor (nur h3)");
    assert_eq!(w.live_count("SaddleRoof"), 1, "1 Roof (nur h3)");
    // Sequenz wandert: nextConstr c1→c3 neu, kein anderer.
    assert!(
        w.nextconstr(&c1, &c3_after).is_some(),
        "nextConstr c1→c3 (neu)"
    );
    assert_eq!(w.live_count("NextConstrRel"), 1, "genau 1 nextConstr");
}
