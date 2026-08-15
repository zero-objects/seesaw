//! Case 1 — FASE 2019 (Fritsche, Kosiol, Schürr, Taentzer): „Efficient
//! Model Synchronization by Automatically Constructed Repair Processes".
//!
//! FRISCH aus dem Paper-Problem abgeleitet (2026-07-16, Sandras
//! Auftrag: „alle Probleme frisch ohne Vorkenntnisse der ersten Generation auf das heutige Datenmodell
//! porten"). KEINE Übernahme der Rekonstruktion der ersten Generation (3-Rule-Fixture,
//! Root/Sub-NAC, DelEdge-Reduktion).
//!
//! ── Das Paper-Problem ──────────────────────────────────────────────
//! LEAST-CHANGING model synchronization: ein Edit (kanonisch ein MOVE)
//! soll propagiert werden, OHNE die abhängigen Target-Elemente zu
//! löschen und neu zu erzeugen — Informationsverlust vermeiden. Das
//! Paper löst das mit Short-Cut-Rules (eine Edit-Aktion zurücknehmen +
//! Alternative ausführen statt zerstören+neu).
//!
//! ── Die frische These, die hier getestet wird ───────────────────
//! die strukturelle Identität (Weg C) BEHAUPTET implizit, das
//! Least-Changing-Ziel OHNE Short-Cut-Rules zu erreichen: hängt die
//! Ziel-Identität nur am verschobenen Element (nicht an seinem
//! Container), reklamiert die Sync das Target beim Move statt es zu
//! zerstören. Das verlangt einen aus dem Problem geschnittenen TGG-SCHNITT, der die
//! Container-Kopplung vermeidet — anders als die Root der ersten Generation/Sub-Regeln,
//! wo die Ziel-Erzeugung an den Container-Status gebunden ist.
//!
//! ── Schnitt aus dem Problem (aus dem Datenmodell, nicht aus der ersten Generation) ─────────
//! - „alles ist Knoten": Containment ist ein reifizierter
//!   Beziehungs-Knoten (Contains / FContains), NICHT eine anonyme
//!   Kante — so ist ein Move ein sauberes Knoten-Delta.
//! - EINE `Package→Folder`-Regel für JEDES Package (Identität = f(pkg),
//!   container-unabhängig). Kein Root/Sub-Split, keine NAC.
//! - EINE `Contains→FContains`-Regel (Containment-Beziehung ↔
//!   Folder-Beziehung), Kontext beide Package-Enden.
//! - Blatt-Regel `Class→DocFile`.
//!   Move = Contains-Knoten (A→p) tombstonen + Contains-Knoten (B→p)
//!   hinzufügen. Erwartung: p-Folder + Sub-Docs + manuelle Ziel-
//!   Ergänzung ÜBERLEBEN ID-stabil; nur die FContains-Beziehung wandert.

mod common;

use seesaw_tgg::engine::{DeltaDomain, Engine};
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::Status;
use seesaw_tgg::plan::DirectedRule;

type Id = seesaw_tgg::ident::GhostId;

// ═══════════════ TGG aus dem Problem ═════════════

/// Jedes Package ↔ ein Folder. Identität container-UNABHÄNGIG — der
/// Schlüssel für Least-Changing beim Move.
fn package_to_folder() -> serde_json::Value {
    serde_json::json!({
            "name": "Package_2_Folder", "rank": 100,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Package"},
                    {"name": "l1", "type": "pkgName"}
                ],
                "links": [["l0", "l1"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Folder"},
                    {"name": "r1", "type": "folderName"}
                ],
                "links": [["r0", "r1"]]
            },
            "corrs": [
                {"type": "PkgFolder", "left": "l0", "right": "r0", "role": "establishes", "bindings": [{"left": "l1", "right": "r1"}]}
            ]
    })
}

/// Containment-Beziehung (parent enthält child) ↔ Folder-Containment.
/// Beide Package-Enden sind Kontext (ihre Folder existieren schon über
/// Package_2_Folder). Erzeugt NUR die FContains-Beziehung.
fn contains_to_fcontains() -> serde_json::Value {
    serde_json::json!({
            "name": "Contains_2_FContains", "rank": 90,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Package"},
                    {"name": "l1", "type": "Contains"},
                    {"name": "l2", "type": "Package"}
                ],
                "links": [["l0", "l1"], ["l1", "l2"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Folder"},
                    {"name": "r1", "type": "FContains"},
                    {"name": "r2", "type": "Folder"}
                ],
                "links": [["r0", "r1"], ["r1", "r2"]]
            },
            "corrs": [
                {"type": "PkgFolder", "left": "l0", "right": "r0", "role": "references"},
                {"type": "PkgFolder", "left": "l2", "right": "r2", "role": "references"},
                {"type": "ContCorr", "left": "l1", "right": "r1", "role": "establishes"}
            ]
    })
}

/// Class im Package ↔ DocFile im Folder (Blatt; name ↦ content).
fn class_to_doc() -> serde_json::Value {
    serde_json::json!({
            "name": "Class_2_Doc", "rank": 80,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Package"},
                    {"name": "l1", "type": "Class"},
                    {"name": "l2", "type": "className"}
                ],
                "links": [["l0", "l1"], ["l1", "l2"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Folder"},
                    {"name": "r1", "type": "DocFile"},
                    {"name": "r2", "type": "docContent"}
                ],
                "links": [["r0", "r1"], ["r1", "r2"]]
            },
            "corrs": [
                {"type": "PkgFolder", "left": "l0", "right": "r0", "role": "references"},
                {"type": "ClsDoc", "left": "l1", "right": "r1", "role": "establishes", "bindings": [{"left": "l2", "right": "r2"}]}
            ]
    })
}

fn ruleset(g: &mut Graph) -> Vec<DirectedRule> {
    common::load(
        "case01_least_changing_move",
        vec![package_to_folder(), contains_to_fcontains(), class_to_doc()],
        g,
    )
}

// ═══════════════ World ═══════════════

struct World {
    g: Graph,
    vs: ValueStore,
    engine: Engine<'static>,
    root_p: Id,
    q: Id,
    p: Id,
    c: Id,
    cont_root_p: Id, // Contains: rootP → p
}

impl World {
    /// Ausgangsmodell: rootP ⊇ p ⊇ c (Class); q ist ein leeres
    /// Ziel-Package (top-level). Alle Containments reifiziert.
    fn new() -> Self {
        let mut g = Graph::new();
        let mk_pkg = |g: &mut Graph, ext: &str| -> (Id, Id) {
            let pkg = g.add_baseline(ext, "Package");
            let nl = g.add_baseline(&format!("{ext}/name"), "pkgName");
            g.connect(pkg, nl, Status::Solid);
            (pkg, nl)
        };
        let (root_p, root_n) = mk_pkg(&mut g, "rootP");
        let (q, q_n) = mk_pkg(&mut g, "q");
        let (p, p_n) = mk_pkg(&mut g, "p");
        // Containment rootP → p als reifizierter Knoten.
        let cont = g.add_baseline("cont/rootP/p", "Contains");
        g.connect(root_p, cont, Status::Solid);
        g.connect(cont, p, Status::Solid);
        // Class c im Package p (classes: anonyme Kante, kein Delta).
        let c = g.add_baseline("c", "Class");
        let c_n = g.add_baseline("c/name", "className");
        g.connect(c, c_n, Status::Solid);
        g.connect(p, c, Status::Solid);

        let rules: &'static [DirectedRule] = Box::leak(ruleset(&mut g).into_boxed_slice());
        let mut w = World {
            g,
            vs: ValueStore::default(),
            engine: Engine::new(rules),
            root_p,
            q,
            p,
            c,
            cont_root_p: cont,
        };
        w.vs.insert(root_n, "rootP");
        w.vs.insert(q_n, "q");
        w.vs.insert(p_n, "p");
        w.vs.insert(c_n, "c");
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

    fn leaf_value(&self, node: &Id, leaf_typ: &str) -> String {
        self.g
            .child_leaf_of_type(node, leaf_typ)
            .and_then(|l| self.g.resolve_value(&l, &self.vs))
            .unwrap_or_default()
    }

    fn nodes_of(&self, typ: &str) -> Vec<Id> {
        self.g
            .types
            .lookup(typ)
            .map(|t| self.g.nodes_of_type(t).map(|n| n.id).collect())
            .unwrap_or_default()
    }

    /// Folder, den ein Package über PkgFolder etabliert.
    fn folder_of(&self, pkg: &Id) -> Option<Id> {
        self.corr_partner(pkg, "PkgFolder", "Folder")
    }

    fn doc_of(&self, class: &Id) -> Option<Id> {
        self.corr_partner(class, "ClsDoc", "DocFile")
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

    /// Lebendige FContains-Beziehung parent_f ⊇ child_f?
    fn fcontains(&self, parent_f: &Id, child_f: &Id) -> Option<Id> {
        let ft = self.g.types.lookup("FContains")?;
        for p in self.g.parts_by_other_type(parent_f, ft) {
            if !p.outgoing || !self.conn_alive(&p.connection) || !self.node_alive(&p.other) {
                continue;
            }
            for q in self.g.parts(&p.other) {
                if q.outgoing && q.other == *child_f && self.conn_alive(&q.connection) {
                    return Some(p.other);
                }
            }
        }
        None
    }

    /// MOVE p von rootP nach q: alten Contains-Knoten tombstonen, neuen
    /// hinzufügen. Beides Knoten-Deltas (aus dem Problem geschnitten).
    fn move_p_to_q(&mut self) {
        self.engine.admit_delta(&[DeltaDomain::Source]);
        // alten Contains(rootP→p) entfernen.
        let old = self.cont_root_p;
        self.g.set_node_status(&old, Status::Tombstone);
        self.engine.element_removed(&old);
        self.engine.element_deleted(&mut self.g, &old);
        // neuen Contains(q→p) hinzufügen.
        let cont = self.g.add_baseline("cont/q/p", "Contains");
        self.g.connect(self.q, cont, Status::Solid);
        self.g.connect(cont, self.p, Status::Solid);
    }

    /// Manuelle Ziel-Ergänzung: eine Notiz am p-Folder, die ein
    /// least-changing Sync ERHALTEN muss (Informationsverlust-Test).
    fn annotate_folder(&mut self, folder: Id, note: &str) {
        let a = self.g.add_baseline(&format!("note/{note}"), "FolderNote");
        self.g.connect(folder, a, Status::Solid);
        self.vs.insert(a, note);
    }

    fn folder_note(&self, folder: &Id) -> Option<String> {
        let nt = self.g.types.lookup("FolderNote")?;
        self.g
            .parts_by_other_type(folder, nt)
            .filter(|p| p.outgoing && self.conn_alive(&p.connection) && self.node_alive(&p.other))
            .find_map(|p| self.g.resolve_value(&p.other, &self.vs))
    }
}

// ═══════════════ Tests ═══════════════

#[test]
fn initial_sync_builds_target() {
    let mut w = World::new();
    w.sync();
    // 3 Folder (rootP, q, p), 1 DocFile (c).
    assert_eq!(w.nodes_of("Folder").len(), 3, "3 Folder (rootP, q, p)");
    assert_eq!(w.nodes_of("DocFile").len(), 1, "1 DocFile (c)");
    let root_f = w.folder_of(&w.root_p).expect("rootP-Folder");
    let p_f = w.folder_of(&w.p).expect("p-Folder");
    assert!(w.fcontains(&root_f, &p_f).is_some(), "rootP_f ⊇ p_f");
    let c_d = w.doc_of(&w.c).expect("c-Doc");
    assert_eq!(w.leaf_value(&c_d, "docContent"), "c");
}

/// DER Paper-Test: Move p von rootP nach q, LEAST-CHANGING.
/// These: p-Folder + Sub-Doc + manuelle Notiz überleben ID-stabil,
/// nur die FContains-Beziehung wandert (rootP_f → q_f). Kein
/// Löschen+Neuerzeugen, keine Short-Cut-Rule nötig.
#[test]
fn move_is_least_changing() {
    let mut w = World::new();
    w.sync();
    let root_f = w.folder_of(&w.root_p).expect("rootP-Folder");
    let q_f = w.folder_of(&w.q).expect("q-Folder");
    let p_f_before = w.folder_of(&w.p).expect("p-Folder vor Move");
    let c_d_before = w.doc_of(&w.c).expect("c-Doc vor Move");
    // Manuelle Ziel-Ergänzung am p-Folder (muss den Move überleben).
    w.annotate_folder(p_f_before, "handmade");

    w.move_p_to_q();
    w.sync();

    // Least-Changing: p-Folder IDENTISCH erhalten (nicht neu erzeugt).
    let p_f_after = w.folder_of(&w.p).expect("p-Folder nach Move");
    assert_eq!(p_f_after, p_f_before, "p-Folder ID-stabil über Move");
    // Manuelle Notiz überlebt (kein Informationsverlust).
    assert_eq!(
        w.folder_note(&p_f_after).as_deref(),
        Some("handmade"),
        "manuelle Ziel-Ergänzung überlebt den Move"
    );
    // Sub-Doc identisch erhalten.
    let c_d_after = w.doc_of(&w.c).expect("c-Doc nach Move");
    assert_eq!(c_d_after, c_d_before, "c-Doc ID-stabil");
    assert_eq!(w.leaf_value(&c_d_after, "docContent"), "c");
    // Die Beziehung ist gewandert: rootP_f ⊉ p_f, q_f ⊇ p_f.
    assert!(
        w.fcontains(&root_f, &p_f_after).is_none(),
        "rootP_f ⊉ p_f (alt gelöst)"
    );
    assert!(w.fcontains(&q_f, &p_f_after).is_some(), "q_f ⊇ p_f (neu)");
    // Keine Folder-Duplikate (3 Folder wie zuvor).
    let live = w
        .nodes_of("Folder")
        .into_iter()
        .filter(|f| w.node_alive(f))
        .count();
    assert_eq!(live, 3, "keine Folder-Neuerzeugung (least-changing)");
}
