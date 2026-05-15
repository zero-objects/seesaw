//! Case 4 — SLE2020 Concurrent delete+modify (Fritsche et al. 2020).
//!
//! Phase 1: Initial-Sync auf 2 Classes mit je 1 Method → 2 Docs + 2 Entries.
//! Phase 2: Concurrent-Delta — Source-User löscht C1 + Target-User
//!   fügt zusätzliches Entry E_extra unter D1 hinzu (BEVOR die Sync
//!   reagiert). Beides in einem User-Delta-Entry kombiniert.
//! Phase 3: Match-Observability invalidiert ClassToDoc(C1) und
//!   MethodToEntry(M1). Konflikt sichtbar: D1 endet als Tombstone,
//!   aber E_extra (User-erzeugt) hängt noch an D1 als Orphan.

#[path = "fixtures/class_doc_mm.rs"]
mod class_doc_mm;

use class_doc_mm::{build_pre_graph, ClassDocSnapshot};
use seesaw_tgg::engine::{run_cascade, run_cascade_observable, Cascade, Rule};
use seesaw_tgg::graph::{GhostId, Status, TypedGraph};
use seesaw_tgg::ops::{DeltaEntry, Op, Origin};
use seesaw_tgg::rule::spec::parse_ruleset;
use seesaw_tgg::rule::{compile, instantiate};
use std::collections::BTreeMap;

const FIXTURE: &str = include_str!("fixtures/rules_sle2020_class_doc.json");

fn load_rules() -> Vec<Box<dyn Rule>> {
    let rs = parse_ruleset(FIXTURE).expect("fixture parst");
    rs.rules
        .iter()
        .map(|r| instantiate(&compile(r).expect("compile")))
        .collect()
}

#[test]
fn fixture_parses_and_compiles() {
    let rs = parse_ruleset(FIXTURE).unwrap();
    assert_eq!(rs.rules.len(), 2);
    for r in &rs.rules {
        let _ = compile(r).unwrap_or_else(|e| panic!("Rule {} kompiliert nicht: {e:?}", r.name));
    }
}

fn run_phase1_sync() -> (TypedGraph, Cascade, ClassDocSnapshot) {
    let (mut graph, snap) = build_pre_graph();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let mut cas = Cascade::new();
    let term = run_cascade(&mut cas, &mut graph, &refs, 200).unwrap();
    eprintln!(
        "Case 4 Phase 1 (Sync) terminiert: {term:?}, entries: {}",
        cas.entries.len()
    );
    for (i, e) in cas.entries.iter().enumerate() {
        let origin = match &e.origin {
            Origin::Rule { rule_id } => rule_id.clone(),
            _ => "User".into(),
        };
        eprintln!("  step {i}: {origin} ({} ops)", e.op_star.len());
    }
    (graph, cas, snap)
}

#[test]
fn case04_phase1_sync_creates_r_side() {
    let (graph, _cas, _snap) = run_phase1_sync();
    let counts = |kind: &str| {
        graph
            .iter_nodes()
            .filter(|n| n.type_id == kind && n.status != Status::Tombstone)
            .count()
    };
    assert_eq!(counts("Doc"), 2, "2 Docs (für C1 + C2)");
    assert_eq!(counts("Entry"), 2, "2 Entries (für M1 + M2)");
}

/// Konstruiert den Concurrent-Delta-Entry: Source-User löscht C1
/// (impliziert M1, weil unter C1) UND Target-User erzeugt
/// E_extra unter D1.
fn run_phase2_concurrent() -> (TypedGraph, Cascade, ClassDocSnapshot, GhostId, GhostId) {
    let (mut graph, mut cas, snap) = run_phase1_sync();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();

    // D1 finden — das R-seitige Doc für C1 (über CorrClass-Topologie)
    let d1 = graph
        .iter_nodes()
        .find(|n| n.type_id == "Doc" && n.attrs.get("name").map(|s| s.as_str()) == Some("C1"))
        .map(|n| n.id)
        .expect("D1 existiert nach Phase 1");

    // E_extra: ein User-erzeugtes Entry direkt unter D1 (Target-Side-Edit).
    // Wir müssen eine Op konstruieren, die einen neuen Knoten als Kind
    // von D1 anhängt. Op::AddNode mit parent=D1, edge_type=z.B. "extra".
    let extra_attrs: BTreeMap<String, String> = [("name".to_string(), "E_extra".to_string())]
        .into_iter()
        .collect();
    let ops = vec![
        // Source-side delete: C1 (cascade-aware: M1 auch)
        Op::DelNode {
            target: snap.ids["C1"],
        },
        // Target-side add: neuer Entry unter D1
        Op::AddNode {
            parent: d1,
            edge_type: "extra".into(),
            type_id: "Entry".into(),
            attrs: extra_attrs.clone(),
        },
    ];
    let user = DeltaEntry::new_user(ops, vec![snap.ids["C1"], d1]);
    user.apply(&mut graph).unwrap();
    cas.append(user);

    // Phase 3: observable cascade
    let term = run_cascade_observable(&mut cas, &mut graph, &refs, 200).unwrap();
    eprintln!("Case 4 Phase 3 terminiert: {term:?}");
    for (i, e) in cas.entries.iter().enumerate() {
        let origin = match &e.origin {
            Origin::User => "USER".to_string(),
            Origin::Rule { rule_id } => rule_id.clone(),
        };
        eprintln!("  entry {i}: {origin} ({} ops)", e.op_star.len());
    }

    // Berechne erwartete E_extra-ID
    let e_extra_id = GhostId::from_parent(&d1, "extra", "Entry", &extra_attrs);

    (graph, cas, snap, d1, e_extra_id)
}

#[test]
fn case04_alpha_class_side_invalidated() {
    let (graph, _cas, snap, d1, _e_extra) = run_phase2_concurrent();
    // Source-Seite: C1 + M1 sind Tombstone (User-Delete + Cascade);
    // C2 + M2 unverändert
    assert_eq!(
        graph.get_node(&snap.ids["C1"]).unwrap().status,
        Status::Tombstone
    );
    assert_eq!(
        graph.get_node(&snap.ids["C2"]).unwrap().status,
        Status::Solid
    );

    // Target-Seite: D1 muss tombstoned sein, weil ClassToDoc(C1) und
    // alle abhängigen Apps invalidiert sind und kein Resurrection-Match.
    let d1_status = graph.get_node(&d1).unwrap().status;
    eprintln!("D1.status = {d1_status:?}");
    assert_eq!(d1_status, Status::Tombstone, "α: D1 final Tombstone");
}

#[test]
fn case04_conflict_marker_orphan_e_extra() {
    let (graph, _cas, _snap, _d1, e_extra) = run_phase2_concurrent();
    // E_extra existiert noch im Graph (User-erzeugt, unabhängig von
    // Konflikt-Cascade). Es ist NICHT Tombstone, weil keine
    // automatische Invalidation-Logik einen User-erzeugten Knoten
    // anfasst.
    let e_extra_node = graph.get_node(&e_extra);
    assert!(e_extra_node.is_some(), "E_extra existiert im Graph");
    let status = e_extra_node.unwrap().status;
    eprintln!("E_extra.status = {status:?}");
    // E_extra ist Ghost (via add_ghost_node-Op-Anwendung) — aber
    // sein Parent D1 ist Tombstone. Das ist die strukturelle
    // Konflikt-Signatur.
    assert!(
        status != Status::Tombstone,
        "Konflikt: E_extra ist Orphan (aktiv, aber Parent-Tombstone)"
    );
}

#[test]
fn case04_gamma_c2_substructure_stable() {
    // γ: die nicht-konflikt-betroffene C2/D2/M2/E2-Substruktur
    // bleibt stabil — Phase-1-IDs der R-Seite für C2 sind weiterhin
    // aktiv nach Phase 3.
    let (mut ref_graph, _) = build_pre_graph();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let _ = run_cascade(&mut Cascade::new(), &mut ref_graph, &refs, 200).unwrap();
    // R-IDs für C2-Doc und M2-Entry
    let c2_doc_id = ref_graph
        .iter_nodes()
        .find(|n| n.type_id == "Doc" && n.attrs.get("name").map(|s| s.as_str()) == Some("C2"))
        .unwrap()
        .id;
    let m2_entry_id = ref_graph
        .iter_nodes()
        .find(|n| n.type_id == "Entry" && n.attrs.get("name").map(|s| s.as_str()) == Some("M2"))
        .unwrap()
        .id;

    let (full_graph, _cas, _snap, _d1, _e_extra) = run_phase2_concurrent();
    let c2_doc = full_graph.get_node(&c2_doc_id).unwrap();
    let m2_entry = full_graph.get_node(&m2_entry_id).unwrap();
    assert!(c2_doc.status != Status::Tombstone, "γ: C2's Doc stabil");
    assert!(m2_entry.status != Status::Tombstone, "γ: M2's Entry stabil");
}

#[cfg(feature = "regen_graphs")]
#[test]
fn case04_regen_snapshots() {
    use seesaw_tgg::engine::{cascade_step, TerminationState};
    use seesaw_tgg::viz::dot::{write_snapshot_triple, DotOpts};
    use std::path::PathBuf;

    let out_root: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "..",
        "..",
        "paper",
        "implementation-report",
        "graphs",
        "case04",
    ]
    .iter()
    .collect();
    let _ = std::fs::remove_dir_all(&out_root);
    std::fs::create_dir_all(&out_root).unwrap();

    let (mut graph, snap) = build_pre_graph();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let opts = DotOpts::default();
    let mut trace = String::from("# Case 4 — Execution Trace\n\n");

    write_snapshot_triple(&out_root.join("00_initial_l_side"), &graph, &opts).unwrap();
    trace.push_str("## 00 Initial (L-Seite — 2 Classes mit je 1 Method)\n\n");

    let mut cas = Cascade::new();
    let sync_dir = out_root.join("phase_01_initial_sync");
    trace.push_str("## Phase 1 — Initial-Sync\n\n");
    let mut step_idx = 0;
    loop {
        let eb = cas.entries.len();
        let term = cascade_step(&mut cas, &mut graph, &refs).unwrap();
        match term {
            TerminationState::Running => {
                let name = match &cas.entries[eb].origin {
                    Origin::Rule { rule_id } => rule_id.clone(),
                    _ => "unknown".into(),
                };
                let dir = sync_dir.join(format!("{step_idx:02}_{name}"));
                write_snapshot_triple(&dir, &graph, &opts).unwrap();
                trace.push_str(&format!("### {step_idx:02} {name}\n\n"));
                step_idx += 1;
                if step_idx > 50 {
                    panic!("overrun");
                }
            }
            other => {
                trace.push_str(&format!("**Sync-Terminierung:** `{other:?}`\n\n"));
                break;
            }
        }
    }

    // Concurrent-Delta
    let d1 = graph
        .iter_nodes()
        .find(|n| n.type_id == "Doc" && n.attrs.get("name").map(|s| s.as_str()) == Some("C1"))
        .map(|n| n.id)
        .unwrap();
    let extra_attrs: BTreeMap<String, String> = [("name".to_string(), "E_extra".to_string())]
        .into_iter()
        .collect();
    let user = DeltaEntry::new_user(
        vec![
            Op::DelNode {
                target: snap.ids["C1"],
            },
            Op::AddNode {
                parent: d1,
                edge_type: "extra".into(),
                type_id: "Entry".into(),
                attrs: extra_attrs,
            },
        ],
        vec![snap.ids["C1"], d1],
    );
    user.apply(&mut graph).unwrap();
    cas.append(user);
    let delta_dir = out_root.join("phase_02_concurrent_delta");
    write_snapshot_triple(&delta_dir.join("00_user_op"), &graph, &opts).unwrap();
    trace.push_str("## Phase 2 — Concurrent-Delta (DelNode C1 + AddNode E_extra under D1)\n\n");

    let entries_before = cas.entries.len();
    let _ = run_cascade_observable(&mut cas, &mut graph, &refs, 200).unwrap();
    let post_dir = out_root.join("phase_03_post_delta");
    trace.push_str("## Phase 3 — Post-Delta-Kaskade (Observable)\n\n");
    for (i, entry) in cas.entries.iter().enumerate().skip(entries_before) {
        let name = match &entry.origin {
            Origin::Rule { rule_id } => rule_id.clone(),
            _ => "unknown".into(),
        };
        let step = i - entries_before;
        let dir = post_dir.join(format!("{step:02}_{name}"));
        write_snapshot_triple(&dir, &graph, &opts).unwrap();
        trace.push_str(&format!("### {step:02} {name}\n\n"));
    }
    if cas.entries.len() == entries_before {
        trace.push_str("**Keine Engine-Folge-Steps.** Match-Observability hat nur Invalidation/Konsolidierung durchgeführt.\n\n");
    }
    std::fs::write(out_root.join("trace.md"), trace).unwrap();
}
