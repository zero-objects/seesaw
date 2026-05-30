//! Case 4 — SLE2020 concurrent delete + modify (Fritsche et al. 2020).
//!
//! Phase 1: initial sync on 2 Classes each with 1 Method → 2 Docs + 2 Entries.
//! Phase 2: concurrent delta — source-user deletes C1 + target-user
//!   adds an extra Entry E_extra under D1 (BEFORE the sync reacts).
//!   Both combined in one user delta entry.
//! Phase 3: match observability invalidates ClassToDoc(C1) and
//!   MethodToEntry(M1). Conflict visible: D1 ends as Tombstone,
//!   while E_extra (user-created) still hangs off D1 as an orphan.

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
    let rs = parse_ruleset(FIXTURE).expect("fixture parses");
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
        let _ = compile(r).unwrap_or_else(|e| panic!("rule {} does not compile: {e:?}", r.name));
    }
}

fn run_phase1_sync() -> (TypedGraph, Cascade, ClassDocSnapshot) {
    let (mut graph, snap) = build_pre_graph();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let mut cas = Cascade::new();
    let term = run_cascade(&mut cas, &mut graph, &refs, 200).unwrap();
    eprintln!(
        "Case 4 Phase 1 (sync) terminates: {term:?}, entries: {}",
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
    assert_eq!(counts("Doc"), 2, "2 Docs (for C1 + C2)");
    assert_eq!(counts("Entry"), 2, "2 Entries (for M1 + M2)");
}

/// Builds the concurrent delta entry: source-user deletes C1
/// (implies M1, because under C1) AND target-user creates
/// E_extra under D1.
fn run_phase2_concurrent() -> (TypedGraph, Cascade, ClassDocSnapshot, GhostId, GhostId) {
    let (mut graph, mut cas, snap) = run_phase1_sync();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();

    // Find D1 — the R-side Doc for C1 (via the CorrClass topology)
    let d1 = graph
        .iter_nodes()
        .find(|n| n.type_id == "Doc" && n.attrs.get("name").map(|s| s.as_str()) == Some("C1"))
        .map(|n| n.id)
        .expect("D1 exists after Phase 1");

    // E_extra: a user-created Entry directly under D1 (target-side edit).
    // We construct an op that appends a new node as a child of D1:
    // Op::AddNode with parent=D1, edge_type="extra".
    let extra_attrs: BTreeMap<String, String> = [("name".to_string(), "E_extra".to_string())]
        .into_iter()
        .collect();
    let ops = vec![
        // Source-side delete: C1 (cascade-aware: M1 too)
        Op::DelNode {
            target: snap.ids["C1"],
        },
        // Target-side add: new Entry under D1
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
    eprintln!("Case 4 Phase 3 terminates: {term:?}");
    for (i, e) in cas.entries.iter().enumerate() {
        let origin = match &e.origin {
            Origin::User => "USER".to_string(),
            Origin::Rule { rule_id } => rule_id.clone(),
        };
        eprintln!("  entry {i}: {origin} ({} ops)", e.op_star.len());
    }

    // Compute the expected E_extra id
    let e_extra_id = GhostId::from_parent(&d1, "extra", "Entry", &extra_attrs);

    (graph, cas, snap, d1, e_extra_id)
}

#[test]
fn case04_alpha_class_side_invalidated() {
    let (graph, _cas, snap, d1, _e_extra) = run_phase2_concurrent();
    // Source side: C1 + M1 are Tombstone (user-delete + cascade);
    // C2 + M2 unchanged
    assert_eq!(
        graph.get_node(&snap.ids["C1"]).unwrap().status,
        Status::Tombstone
    );
    assert_eq!(
        graph.get_node(&snap.ids["C2"]).unwrap().status,
        Status::Solid
    );

    // Target side: D1 must be tombstoned, because ClassToDoc(C1) and
    // all dependent applications are invalidated and there is no
    // resurrection match.
    let d1_status = graph.get_node(&d1).unwrap().status;
    eprintln!("D1.status = {d1_status:?}");
    assert_eq!(d1_status, Status::Tombstone, "α: D1 finally Tombstone");
}

#[test]
fn case04_conflict_marker_orphan_e_extra() {
    let (graph, _cas, _snap, _d1, e_extra) = run_phase2_concurrent();
    // E_extra still exists in the graph (user-created, independent
    // of the conflict cascade). It is NOT Tombstone, because no
    // automatic invalidation logic touches a user-created node.
    let e_extra_node = graph.get_node(&e_extra);
    assert!(e_extra_node.is_some(), "E_extra exists in the graph");
    let status = e_extra_node.unwrap().status;
    eprintln!("E_extra.status = {status:?}");
    // E_extra is Ghost (via add_ghost_node op application) — but
    // its parent D1 is Tombstone. That is the structural
    // conflict signature.
    assert!(
        status != Status::Tombstone,
        "conflict: E_extra is an orphan (active, but parent Tombstone)"
    );
}

#[test]
fn case04_gamma_c2_substructure_stable() {
    // γ: the conflict-free C2/D2/M2/E2 substructure stays stable —
    // Phase 1 R-side ids for C2 are still active after Phase 3.
    let (mut ref_graph, _) = build_pre_graph();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let _ = run_cascade(&mut Cascade::new(), &mut ref_graph, &refs, 200).unwrap();
    // R-ids for C2's Doc and M2's Entry
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
    assert!(c2_doc.status != Status::Tombstone, "γ: C2's Doc stable");
    assert!(m2_entry.status != Status::Tombstone, "γ: M2's Entry stable");
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
    trace.push_str("## 00 Initial (L-side — 2 Classes each with 1 Method)\n\n");

    let mut cas = Cascade::new();
    let sync_dir = out_root.join("phase_01_initial_sync");
    trace.push_str("## Phase 1 — initial sync\n\n");
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
                trace.push_str(&format!("**sync termination:** `{other:?}`\n\n"));
                break;
            }
        }
    }

    // Concurrent delta
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
    trace.push_str("## Phase 2 — concurrent delta (DelNode C1 + AddNode E_extra under D1)\n\n");

    let entries_before = cas.entries.len();
    let _ = run_cascade_observable(&mut cas, &mut graph, &refs, 200).unwrap();
    let post_dir = out_root.join("phase_03_post_delta");
    trace.push_str("## Phase 3 — post-delta cascade (observable)\n\n");
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
        trace.push_str("**No engine follow-up steps.** Match observability only performed invalidation/consolidation.\n\n");
    }
    std::fs::write(out_root.join("trace.md"), trace).unwrap();
}
