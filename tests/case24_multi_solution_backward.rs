//! Case 24 — UML2RDBMS Inheritance-Flattening, Multi-Solution-Backward.
//! FRISCH aus dem Paper-Problem (Bucaioni & Eramo, ECEASST 57, BX 2013),
//! aus dem Problem geschnitten geschnitten, ohne Vorkenntnisse der ersten Generation (2026-07-17).
//!
//! ── Das Paper-Problem ──────────────────────────────────────────────
//! Eine Klassenhierarchie A ← B ← C (parent-Beziehung) wird per
//! Inheritance-Flattening auf EINE Table geflacht: jedes Attribut jeder
//! Klasse der Hierarchie → Column derselben (Wurzel-)Table. Wird im
//! RDBMS manuell eine Column(email) ergaenzt, gibt es DREI konsistente
//! Quellmodelle — email als Attribut von A, von B oder von C. JTL (ASP)
//! liefert alle drei in EINEM Lauf; eMoflon (2013) konnte die Wahl gar
//! nicht ausdruecken.
//!
//! ── Die Position (ehrlich, nicht ueberverkauft) ─────────────────
//! die Engine ist deterministisch und liefert KEINE All-Solutions-Semantik.
//! Aber: der Loesungsraum ist ueber RANG-KONFIGURATIONEN deterministisch
//! NAVIGIERBAR. Drei Rank-Ordnungen der Backward-Attribut-Regeln → email
//! landet deterministisch und EXKLUSIV als Attribut unter A bzw. B bzw.
//! C; jeder Lauf ist bit-reproduzierbar. Das ist Iteration ueber
//! Konfigurationen (eine Loesung pro Lauf), NICHT die Loesungsmenge in
//! einem Lauf wie JTL. Genau so ist es zu klassifizieren.
//!
//! ── Schnitt aus dem Problem ─────────────────────────────────────────────
//! Delta-tragende Beziehungen sind reifizierte KNOTEN, keine anonymen
//! Kanten: die parent-Beziehung der Hierarchie (`Parent`-Knoten:
//! child → Parent → parentClass) und die owner-Beziehung Attribut→Class
//! (`Owner`-Knoten: attr → Owner → ownerClass). Identitaet ist
//! strukturell/wert-frei (Ghost-Ids ueber Provenienz, nie ueber Werte).
//!
//! Genau EINE bidirektionale Spec pro Hierarchie-Tiefe deckt beide
//! Richtungen ab: forward Attribut→Column (Kontext = die Klassen der
//! Kette), backward Column→Attribut-unter-dieser-Klasse. Die drei
//! Backward-Richtungen konkurrieren um die freie Column; der Rang
//! entscheidet, die rc8-#2-Wiedererkennung (Anker traegt bereits einen
//! AttrCorr) unterdrueckt die uebrigen — deshalb EXKLUSIV eine Platzierung.

mod common;

use seesaw_tgg::engine::Engine;
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::Status;
use seesaw_tgg::plan::DirectedRule;

type Id = seesaw_tgg::ident::GhostId;

// ═══════════════ Regeln (Rang parametrisiert) ═══════════════════════════════

/// FR1: Die Wurzel-Klasse BAUT die Table. In einem zusammenhaengenden
/// Graphen hat JEDE Class einen Parent — die Wurzel ist die, deren
/// Parent auf den CONTAINER (Package) zeigt statt auf eine andere Class.
/// Das ist eine positive Typ-Bedingung (Parent-Ziel = Package), keine
/// Abwesenheits-NAC. Subklassen (Parent → Class) fitten diese Regel
/// nicht und bauen KEINE eigene Table — kein Verbot noetig.
fn root_class_to_table() -> serde_json::Value {
    serde_json::json!({
            "name": "RootClass_2_Table", "rank": 500,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Class"},
                    {"name": "l1", "type": "className"},
                    {"name": "l2", "type": "Parent"},
                    {"name": "l3", "type": "Package"}
                ],
                "links": [["l0", "l1"], ["l0", "l2"], ["l2", "l3"]]
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
                {"type": "TableCorr", "left": "l0", "right": "r0", "role": "establishes", "bindings": [{"left": "l1", "right": "r1"}]}
            ]
    })
}

/// Tiefe-A-Spec: Attribut, dessen Owner direkt die Wurzel-Klasse A ist
/// (A traegt den TableCorr → Table). Forward: Attribut → Column der
/// Table. Backward: freie Column → Attribut UNTER A.
fn attr_depth_a(rank: u64) -> serde_json::Value {
    serde_json::json!({
            "name": "Attr_2_Column_A", "rank": rank,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Attribute"},
                    {"name": "l1", "type": "attrName"},
                    {"name": "l2", "type": "Owner"},
                    {"name": "l3", "type": "Class"}
                ],
                "links": [["l0", "l1"], ["l0", "l2"], ["l2", "l3"]]
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
                {"type": "TableCorr", "left": "l3", "right": "r0", "role": "references"},
                {"type": "AttrCorr", "left": "l0", "right": "r1", "role": "establishes", "bindings": [{"left": "l1", "right": "r2"}]}
            ]
    })
}

/// Tiefe-B-Spec: Attribut, dessen Owner-Klasse B genau ein parent-Hop
/// von der Wurzel A entfernt ist (B → Parent → A, A traegt TableCorr).
/// B und der Parent-Knoten sind Kontext (existieren, werden nie erzeugt).
///
/// ── ENGINE-BEFUND: BEHOBEN (Lowering-Fix, references vor Link-Schleife) ──
/// Historisch traegt diese Regel die Spur eines Kern-Befunds: das
/// Lowering trug den `references`-Corr-Endpunkt (hier ClassA, l_pos 5)
/// erst NACH der Ausgangs-Link-Schleife in die Kontext-Positions-
/// Tabelle ein, wodurch der Ausgangs-Link [4, 5] (ParentB → ClassA) als
/// Pattern-Constraint wegfiel und die 1-Hop-Mittelklasse B ihre
/// Rueck-Eindeutigkeit verlor (ClassB matchte auch C). Kompensiert war
/// das durch eine Grosseltern-NAC. Nach dem Lowering-Fix (references-
/// Corrs werden VOR den Link-/Join-Schleifen eingetragen) greift der
/// Link [4, 5] wieder als Constraint — die NAC ist entfallen, die Regel
/// steht in der reinen Paper-Form.
fn attr_depth_b(rank: u64) -> serde_json::Value {
    serde_json::json!({
        "name": "Attr_2_Column_B", "rank": rank,
        "left": {
            "anchor": "attr",
            "nodes": [
                {"name": "attr", "type": "Attribute"},
                {"name": "aname", "type": "attrName"},
                {"name": "owner", "type": "Owner"},
                {"name": "clsB", "type": "Class", "context": true},
                {"name": "parent", "type": "Parent", "context": true},
                {"name": "clsA", "type": "Class"}
            ],
            "links": [["attr", "aname"], ["attr", "owner"], ["owner", "clsB"],
                      ["clsB", "parent"], ["parent", "clsA"]]
        },
        "right": {
            "anchor": "table",
            "nodes": [
                {"name": "table", "type": "Table"},
                {"name": "col", "type": "Column"},
                {"name": "cname", "type": "colName"}
            ],
            "links": [["table", "col"], ["col", "cname"]]
        },
        "corrs": [
            {"type": "TableCorr", "left": "clsA", "right": "table", "role": "references"},
            {"type": "AttrCorr", "left": "attr", "right": "col", "role": "establishes",
             "bindings": [{"left": "aname", "right": "cname"}]}
        ]
    })
}

/// Tiefe-C-Spec: Owner-Klasse C ist zwei parent-Hops von der Wurzel A
/// entfernt (C → Parent → B → Parent → A). C, B und die beiden
/// Parent-Knoten sind Kontext.
fn attr_depth_c(rank: u64) -> serde_json::Value {
    serde_json::json!({
        "name": "Attr_2_Column_C", "rank": rank,
        "left": {
            "anchor": "attr",
            "nodes": [
                {"name": "attr", "type": "Attribute"},
                {"name": "aname", "type": "attrName"},
                {"name": "owner", "type": "Owner"},
                {"name": "clsC", "type": "Class", "context": true},
                {"name": "parentCB", "type": "Parent", "context": true},
                {"name": "clsB", "type": "Class", "context": true},
                {"name": "parentBA", "type": "Parent", "context": true},
                {"name": "clsA", "type": "Class"}
            ],
            "links": [["attr", "aname"], ["attr", "owner"], ["owner", "clsC"],
                      ["clsC", "parentCB"], ["parentCB", "clsB"],
                      ["clsB", "parentBA"], ["parentBA", "clsA"]]
        },
        "right": {
            "anchor": "table",
            "nodes": [
                {"name": "table", "type": "Table"},
                {"name": "col", "type": "Column"},
                {"name": "cname", "type": "colName"}
            ],
            "links": [["table", "col"], ["col", "cname"]]
        },
        "corrs": [
            {"type": "TableCorr", "left": "clsA", "right": "table", "role": "references"},
            {"type": "AttrCorr", "left": "attr", "right": "col", "role": "establishes",
             "bindings": [{"left": "aname", "right": "cname"}]}
        ]
    })
}

/// Rang-Konfiguration der drei Backward-Attribut-Regeln.
#[derive(Clone, Copy)]
struct Ranks {
    a: u64,
    b: u64,
    c: u64,
}

fn ruleset(g: &mut Graph, r: Ranks) -> Vec<DirectedRule> {
    let specs = [
        root_class_to_table(),
        attr_depth_a(r.a),
        attr_depth_b(r.b),
        attr_depth_c(r.c),
    ];
    common::load("case24_multi_solution_backward", specs.to_vec(), g)
}

// ═══════════════ World: Hierarchie A ← B ← C + reifizierte Knoten ═══════════

struct World {
    g: Graph,
    vs: ValueStore,
    engine: Engine<'static>,
}

impl World {
    /// A (Wurzel), B child-of A, C child-of B. Attribute a1@A, b1@B, c1@C,
    /// jeweils ueber einen reifizierten `Owner`-Knoten an die Klasse
    /// gebunden. parent-Beziehung ebenfalls reifiziert (`Parent`-Knoten).
    fn new(r: Ranks) -> Self {
        let mut g = Graph::new();
        let mut vs = ValueStore::default();

        // Klassen + Namen.
        let mk_class = |g: &mut Graph, vs: &mut ValueStore, ext: &str, name: &str| -> Id {
            let c = g.add_baseline(ext, "Class");
            let cn = g.add_baseline(&format!("{ext}/name"), "className");
            g.connect(c, cn, Status::Solid);
            vs.insert(cn, name.to_string());
            c
        };
        let a = mk_class(&mut g, &mut vs, "cls/A", "A");
        let b = mk_class(&mut g, &mut vs, "cls/B", "B");
        let c = mk_class(&mut g, &mut vs, "cls/C", "C");

        // parent-Kette reifiziert: child → Parent → parentClass.
        let mk_parent = |g: &mut Graph, child: Id, parent: Id, ext: &str| {
            let p = g.add_baseline(ext, "Parent");
            g.connect(child, p, Status::Solid);
            g.connect(p, parent, Status::Solid);
        };
        mk_parent(&mut g, b, a, "par/B_A");
        mk_parent(&mut g, c, b, "par/C_B");
        // Die Wurzel A haengt am CONTAINER (Package): ihr Parent-Ziel ist
        // KEINE Class, sondern der Container. Kein Nullwert, positiv
        // geschrieben — so ist die Wurzel positiv erkennbar (Parent →
        // Package) statt durch Abwesenheit einer superClass.
        let pkg = g.add_baseline("pkg", "Package");
        mk_parent(&mut g, a, pkg, "par/A_pkg");

        // Attribute reifiziert: attr → Owner → ownerClass.
        let mk_attr = |g: &mut Graph, vs: &mut ValueStore, owner: Id, ext: &str, name: &str| {
            let at = g.add_baseline(ext, "Attribute");
            let an = g.add_baseline(&format!("{ext}/name"), "attrName");
            g.connect(at, an, Status::Solid);
            vs.insert(an, name.to_string());
            let ow = g.add_baseline(&format!("{ext}/owner"), "Owner");
            g.connect(at, ow, Status::Solid);
            g.connect(ow, owner, Status::Solid);
        };
        mk_attr(&mut g, &mut vs, a, "attr/a1", "a1");
        mk_attr(&mut g, &mut vs, b, "attr/b1", "b1");
        mk_attr(&mut g, &mut vs, c, "attr/c1", "c1");

        let rules: &'static [DirectedRule] = Box::leak(ruleset(&mut g, r).into_boxed_slice());
        World {
            g,
            vs,
            engine: Engine::new(rules),
        }
    }

    /// World-sync bis Fixpunkt (frisches Seed je Runde, TT-bewusst) —
    /// identische Schleife wie Case 5/6.
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

    /// Backward-Delta: freie Column(email) unter die (einzige) Table.
    /// KEIN AttrCorr — genau die Situation aus dem Paper.
    fn add_email_column(&mut self) {
        let table = self.the_table().expect("Table existiert nach Forward");
        let col = self.g.add_baseline("delta/email_col", "Column");
        let cn = self.g.add_baseline("delta/email_col/name", "colName");
        self.g.connect(table, col, Status::Solid);
        self.g.connect(col, cn, Status::Solid);
        self.vs.insert(cn, "email");
    }

    // ── Navigation ────────────────────────────────────────────────

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
            .map(|t| self.g.nodes_of_type(t).count())
            .unwrap_or(0)
    }

    fn the_table(&self) -> Option<Id> {
        let tt = self.g.types.lookup("Table")?;
        self.g.nodes_of_type(tt).next().map(|n| n.id)
    }

    fn leaf_value(&self, node: &Id, leaf: &str) -> Option<String> {
        self.g
            .child_leaf_of_type(node, leaf)
            .and_then(|l| self.g.resolve_value(&l, &self.vs))
    }

    /// Alle lebendigen Attribute (Ids).
    fn attributes(&self) -> Vec<Id> {
        self.g
            .types
            .lookup("Attribute")
            .map(|t| self.g.nodes_of_type(t).map(|n| n.id).collect())
            .unwrap_or_default()
    }

    /// Owner-Klassenname eines Attributs: attr → Owner → Class → className.
    fn owner_class_name(&self, attr: &Id) -> Option<String> {
        let ot = self.g.types.lookup("Owner")?;
        let ct = self.g.types.lookup("Class")?;
        for p in self.g.parts_by_other_type(attr, ot) {
            if !p.outgoing || !self.conn_alive(&p.connection) || !self.node_alive(&p.other) {
                continue;
            }
            for q in self.g.parts_by_other_type(&p.other, ct) {
                if !q.outgoing || !self.conn_alive(&q.connection) || !self.node_alive(&q.other) {
                    continue;
                }
                return self.leaf_value(&q.other, "className");
            }
        }
        None
    }

    /// Kanonischer Fingerprint: sortierte (attrName, ownerClass)-Paare
    /// aller lebendigen Attribute — Grundlage des Determinismus-Belegs.
    fn fingerprint(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .attributes()
            .iter()
            .map(|a| {
                (
                    self.leaf_value(a, "attrName").unwrap_or_default(),
                    self.owner_class_name(a).unwrap_or_default(),
                )
            })
            .collect();
        out.sort();
        out
    }

    /// Owner-Klasse des einzigen email-Attributs.
    fn email_owner(&self) -> Option<String> {
        self.attributes()
            .iter()
            .find(|a| self.leaf_value(a, "attrName").as_deref() == Some("email"))
            .and_then(|a| self.owner_class_name(a))
    }

    fn email_attr_count(&self) -> usize {
        self.attributes()
            .iter()
            .filter(|a| self.leaf_value(a, "attrName").as_deref() == Some("email"))
            .count()
    }
}

// ═══════════════ (a) Forward-Baseline ═══════════════════════════════════════

/// Hierarchie A ← B ← C mit je einem Attribut → EINE Table mit drei
/// Columns. Beide Richtungen geladen: kein Ping-Pong (die
/// rc8-#2-Wiedererkennung stoppt die Rueckrichtung an uebersetzten
/// Columns), keine spontanen Backward-Attribute.
#[test]
fn forward_baseline_flattens_to_one_table() {
    let mut w = World::new(Ranks {
        a: 900,
        b: 100,
        c: 50,
    });
    w.sync();

    assert_eq!(w.live_count("Table"), 1, "genau eine (Wurzel-)Table");
    assert_eq!(w.live_count("Column"), 3, "a1,b1,c1 → drei Columns");
    // Alle drei Original-Attribute unveraendert, korrekt zugeordnet.
    assert_eq!(
        w.fingerprint(),
        vec![
            ("a1".to_string(), "A".to_string()),
            ("b1".to_string(), "B".to_string()),
            ("c1".to_string(), "C".to_string()),
        ]
    );
    // Column-Namen = Attribut-Namen (Flattening ueber die identity-Bindung).
    let tbl = w.the_table().unwrap();
    let ct = w.g.types.lookup("Column").unwrap();
    let mut cols: Vec<String> =
        w.g.parts_by_other_type(&tbl, ct)
            .filter(|p| p.outgoing && w.conn_alive(&p.connection) && w.node_alive(&p.other))
            .filter_map(|p| w.leaf_value(&p.other, "colName"))
            .collect();
    cols.sort();
    assert_eq!(cols, vec!["a1", "b1", "c1"]);
}

// ═══════════════ (b) Drei Rang-Konfigurationen → A / B / C ═══════════════════

/// Ein Lauf pro Konfiguration: Forward-Baseline, dann Column(email)
/// als Backward-Delta, dann sync. Die ranghoechste Backward-Regel
/// gewinnt und platziert email EXKLUSIV unter ihrer Klasse.
fn run_config(r: Ranks) -> World {
    let mut w = World::new(r);
    w.sync(); // Forward-Baseline
    w.add_email_column();
    w.sync(); // Backward-Delta propagieren
    w
}

/// Gemeinsame Zusicherungen fuer jede Konfiguration: genau EIN
/// email-Attribut, unter der erwarteten Klasse, kein Wildwuchs.
fn assert_email_under(r: Ranks, expected: &str) {
    let w = run_config(r);
    // Genau EIN email-Attribut (nicht drei — die Engine platziert exklusiv).
    assert_eq!(
        w.email_attr_count(),
        1,
        "genau ein email-Attribut (exklusive Platzierung), Ziel {expected}"
    );
    assert_eq!(
        w.email_owner().as_deref(),
        Some(expected),
        "email deterministisch unter {expected}"
    );
    // Vier Attribute total (a1,b1,c1 + email), vier Columns (keine
    // Duplikat-Column fuer das neue Attribut: forward erkennt den
    // bestehenden AttrCorr).
    assert_eq!(w.live_count("Column"), 4, "email-Column bleibt einzig");
    assert_eq!(
        w.attributes().len(),
        4,
        "genau ein neues Attribut, keine Geister"
    );
    // Fingerprint enthaelt email genau einmal, gebunden an `expected`.
    let fp = w.fingerprint();
    let email_pairs: Vec<&(String, String)> = fp.iter().filter(|(n, _)| n == "email").collect();
    assert_eq!(
        email_pairs,
        vec![&("email".to_string(), expected.to_string())]
    );
}

#[test]
fn config_a_places_email_under_root_a() {
    assert_email_under(
        Ranks {
            a: 900,
            b: 100,
            c: 50,
        },
        "A",
    );
}

#[test]
fn config_b_places_email_under_b() {
    assert_email_under(
        Ranks {
            a: 100,
            b: 900,
            c: 50,
        },
        "B",
    );
}

#[test]
fn config_c_places_email_under_leaf_c() {
    assert_email_under(
        Ranks {
            a: 50,
            b: 100,
            c: 900,
        },
        "C",
    );
}

// ═══════════════ (c) Determinismus je Konfiguration ═════════════════════════

/// Dieselbe Konfiguration → bit-identisches Ergebnis ueber zwei Laeufe
/// (Ghost-Ids sind strukturell, μ-Ordnung total) — und die drei
/// Konfigurationen unterscheiden sich WIRKLICH nur in der email-Zeile.
#[test]
fn each_configuration_is_bit_reproducible() {
    let cfg_a = Ranks {
        a: 900,
        b: 100,
        c: 50,
    };
    let cfg_b = Ranks {
        a: 100,
        b: 900,
        c: 50,
    };
    let cfg_c = Ranks {
        a: 50,
        b: 100,
        c: 900,
    };

    // Reproduzierbarkeit: zwei Laeufe je Konfiguration identisch.
    assert_eq!(
        run_config(cfg_a).fingerprint(),
        run_config(cfg_a).fingerprint()
    );
    assert_eq!(
        run_config(cfg_b).fingerprint(),
        run_config(cfg_b).fingerprint()
    );
    assert_eq!(
        run_config(cfg_c).fingerprint(),
        run_config(cfg_c).fingerprint()
    );

    // Die drei Loesungen sind paarweise verschieden — der Loesungsraum
    // wird ueber die Konfiguration NAVIGIERT, eine Loesung pro Lauf.
    let owner = |r| run_config(r).email_owner().unwrap();
    assert_eq!(owner(cfg_a), "A");
    assert_eq!(owner(cfg_b), "B");
    assert_eq!(owner(cfg_c), "C");
    assert_ne!(owner(cfg_a), owner(cfg_b));
    assert_ne!(owner(cfg_b), owner(cfg_c));
}

// ═══════════════ (d) Kein Overlay-Gap: exklusiv, nicht kaschiert ════════════

/// Ehrlicher Gap-Check. Case 16/24 auf der ALTEN Engine der ersten Generation hatte einen
/// dir-NAC-Gap: die nicht-praeferierten Backward-Varianten haengten
/// Overlay-`attrs`-Kanten an den wiederverwendeten Knoten (3 statt 1).
///
/// Auf die Engine tritt dieser Gap NICHT auf: die rc8-#2-Wiedererkennung
/// (`translated` in `engine::step_with_limit`) greift VOR
/// `apply_creation` und liefert `Some(false)` ohne jede Erzeugung.
/// Nachweis: nach der Platzierung existiert genau EIN `Owner`-Knoten,
/// der ein email-Attribut mit einer Klasse verbindet — keine Streu-Kante
/// zu den beiden nicht-praeferierten Klassen.
#[test]
fn no_stray_overlay_edges_on_unpreferred_variants() {
    let w = run_config(Ranks {
        a: 100,
        b: 900,
        c: 50,
    }); // email → B

    // Alle Owner-Knoten, deren gebundenes Attribut "email" heisst.
    let at_t = w.g.types.lookup("Attribute").unwrap();
    let ot = w.g.types.lookup("Owner").unwrap();
    let ct = w.g.types.lookup("Class").unwrap();

    let mut email_owner_edges: Vec<String> = Vec::new();
    for attr in w.attributes() {
        if w.leaf_value(&attr, "attrName").as_deref() != Some("email") {
            continue;
        }
        for p in w.g.parts_by_other_type(&attr, ot) {
            if !p.outgoing || !w.conn_alive(&p.connection) || !w.node_alive(&p.other) {
                continue;
            }
            for q in w.g.parts_by_other_type(&p.other, ct) {
                if q.outgoing && w.conn_alive(&q.connection) && w.node_alive(&q.other) {
                    if let Some(name) = w.leaf_value(&q.other, "className") {
                        email_owner_edges.push(name);
                    }
                }
            }
        }
    }
    // GENAU eine owner-Kante (email → B). KEIN Gap: die verdraengten
    // Varianten A und C haengen NICHTS an.
    assert_eq!(
        email_owner_edges,
        vec!["B".to_string()],
        "exklusive owner-Kante, keine Streu-Overlays der Verlierer-Regeln"
    );
    let _ = at_t;
}
