//! Case 3 — JOT2022 SysML dangling transition (Weidmann et al. 2022).
//!
//! Phase 1: initial sync on an L pre-graph with a dangling transition
//! (t2 has source but no target). TransToEdge matches only for the
//! valid transition t1; t2 stays untranslated on the R-side.
//!
//! Phase 2: user delta adds the missing target edge for t2.
//! Match observability (M5) triggers TransToEdge on t2.

#[path = "fixtures/sysml_machine_mm.rs"]
mod sysml_machine_mm;

use seesaw_tgg::engine::{run_cascade, run_cascade_observable, Cascade, Rule};
use seesaw_tgg::graph::{GhostId, Status, TypedGraph};
use seesaw_tgg::ops::{DeltaEntry, Op, Origin};
use seesaw_tgg::rule::spec::parse_ruleset;
use seesaw_tgg::rule::{compile, instantiate};
use std::collections::BTreeMap;
use sysml_machine_mm::{build_dangling_graph, SysMlSnapshot};

const FIXTURE: &str = include_str!("fixtures/rules_jot2022_sysml.json");

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
    assert_eq!(rs.rules.len(), 4);
    for r in &rs.rules {
        let _ = compile(r).unwrap_or_else(|e| panic!("rule {} does not compile: {e:?}", r.name));
    }
}

fn run_phase1_sync() -> (TypedGraph, Cascade, SysMlSnapshot) {
    let (mut graph, snap) = build_dangling_graph();
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let mut cascade = Cascade::new();
    let term = run_cascade(&mut cascade, &mut graph, &rule_refs, 200).unwrap();
    eprintln!(
        "Case 3 Phase 1 (initial sync) terminates: {term:?}, entries: {}",
        cascade.entries.len()
    );
    for (i, e) in cascade.entries.iter().enumerate() {
        let origin = match &e.origin {
            Origin::Rule { rule_id } => rule_id.clone(),
            _ => "User".into(),
        };
        eprintln!("  step {i}: {origin} ({} ops)", e.op_star.len());
    }
    (graph, cascade, snap)
}

#[test]
fn case03_phase1_initial_sync_with_dangling() {
    let (graph, cascade, _snap) = run_phase1_sync();
    // Expectation: 4 rule steps:
    //   SMToMachine + 2× StateToEvent + VarToVar + TransToEdge (for t1)
    // = 5. NOT 6 — TransToEdge does NOT fire for t2 (dangling).
    let rule_steps: Vec<String> = cascade
        .entries
        .iter()
        .filter_map(|e| {
            if let Origin::Rule { rule_id } = &e.origin {
                Some(rule_id.clone())
            } else {
                None
            }
        })
        .collect();
    eprintln!("Case 3 Phase 1 rule sequence: {rule_steps:?}");

    let trans_to_edge_count = rule_steps.iter().filter(|r| *r == "TransToEdge").count();
    assert_eq!(
        trans_to_edge_count, 1,
        "Phase 1: TransToEdge fires only for valid t1, NOT for dangling t2"
    );

    // R-side: 1 Machine, 2 EventBlocks, 1 EventVariable, 1 MachineEdge
    let counts = |kind: &str| graph.iter_nodes().filter(|n| n.type_id == kind).count();
    assert_eq!(counts("Machine"), 1, "1 Machine");
    assert_eq!(counts("EventBlock"), 2, "2 EventBlocks (for s1+s2)");
    assert_eq!(counts("EventVariable"), 1, "1 EventVariable");
    assert_eq!(
        counts("MachineEdge"),
        1,
        "1 MachineEdge (only for t1 — t2 dangling)"
    );
}

fn run_phase2_complete_dangling() -> (TypedGraph, Cascade, SysMlSnapshot) {
    let (mut graph, cascade_arr, snap) = run_phase1_sync();
    let mut cascade = cascade_arr;
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();

    // Phase 2: user delta adds the missing target edge for t2.
    // We need to create a new edge: t2 --target--> s2.
    let user = DeltaEntry::new_user(
        vec![Op::AddEdge {
            source: snap.ids["t2"],
            target: snap.ids["s2"],
            type_id: "target".into(),
            attrs: BTreeMap::new(),
        }],
        vec![snap.ids["t2"], snap.ids["s2"]],
    );
    user.apply(&mut graph).expect("AddEdge applies");
    cascade.append(user);

    let term =
        run_cascade_observable(&mut cascade, &mut graph, &rule_refs, 200).expect("post-delta");
    eprintln!("Case 3 Phase 2 (user delta + observable) terminates: {term:?}");
    let new_steps: Vec<String> = cascade
        .entries
        .iter()
        .filter_map(|e| {
            if let Origin::Rule { rule_id } = &e.origin {
                Some(rule_id.clone())
            } else {
                None
            }
        })
        .collect();
    eprintln!("Case 3 Phase 2 rule sequence: {new_steps:?}");

    (graph, cascade, snap)
}

#[test]
fn case03_phase2_user_completes_dangling() {
    let (graph, cascade, _snap) = run_phase2_complete_dangling();

    // After Phase 2: TransToEdge should NOW also have fired for t2
    // → 2 MachineEdges total.
    let machine_edges = graph
        .iter_nodes()
        .filter(|n| n.type_id == "MachineEdge" && n.status != Status::Tombstone)
        .count();
    assert_eq!(
        machine_edges, 2,
        "Phase 2: after completing the target edge, TransToEdge also fires for t2"
    );

    // β evidence: TransToEdge appears 2× in the cascade
    let trans_to_edge_count = cascade
        .entries
        .iter()
        .filter(|e| matches!(&e.origin, Origin::Rule { rule_id } if rule_id == "TransToEdge"))
        .count();
    assert_eq!(trans_to_edge_count, 2);
}

#[test]
fn case03_gamma_phase1_substructure_stable_in_phase2() {
    // γ evidence Case 3: the R-side ids produced in Phase 1 for the
    // clean substructure (Machine + EventBlocks for s1/s2 +
    // EventVariable + MachineEdge for t1) stay active in Phase 2
    // under identical ghost ids.
    let (mut ref_graph, _) = build_dangling_graph();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let _ = run_cascade(&mut Cascade::new(), &mut ref_graph, &refs, 200).unwrap();
    let phase1_r_ids: Vec<GhostId> = ref_graph
        .iter_nodes()
        .filter(|n| {
            matches!(
                n.type_id.as_str(),
                "Machine" | "EventBlock" | "EventVariable" | "MachineEdge"
            ) && n.status != Status::Tombstone
        })
        .map(|n| n.id)
        .collect();
    eprintln!("Phase 1 R-ids: {}", phase1_r_ids.len());

    let (full_graph, _cas, _snap) = run_phase2_complete_dangling();
    let stable: usize = phase1_r_ids
        .iter()
        .filter(|id| {
            full_graph
                .get_node(id)
                .map(|n| n.status != Status::Tombstone)
                .unwrap_or(false)
        })
        .count();
    eprintln!(
        "γ: of {} Phase 1 R-ids, {} stay stable after Phase 2",
        phase1_r_ids.len(),
        stable
    );
    assert_eq!(
        stable,
        phase1_r_ids.len(),
        "γ Case 3: all Phase 1 R-ids stay stable through Phase 2"
    );
}

#[cfg(feature = "regen_graphs")]
#[test]
fn case03_regen_snapshots() {
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
        "case03",
    ]
    .iter()
    .collect();
    let _ = std::fs::remove_dir_all(&out_root);
    std::fs::create_dir_all(&out_root).unwrap();

    let (mut graph, snap) = build_dangling_graph();
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let opts = DotOpts::default();
    let mut trace = String::from("# Case 3 — Execution Trace\n\n");

    write_snapshot_triple(&out_root.join("00_initial_l_side"), &graph, &opts).unwrap();
    trace.push_str("## 00 Initial (L-side — t2 without target edge)\n\n");

    let mut cascade = Cascade::new();
    let sync_dir = out_root.join("phase_01_initial_sync");
    trace.push_str("## Phase 1 — initial sync\n\n");
    let mut step_idx = 0;
    loop {
        let eb = cascade.entries.len();
        let term = cascade_step(&mut cascade, &mut graph, &rule_refs).unwrap();
        match term {
            TerminationState::Running => {
                let name = match &cascade.entries[eb].origin {
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

    // Phase 2: user delta fills the target edge
    let user = DeltaEntry::new_user(
        vec![Op::AddEdge {
            source: snap.ids["t2"],
            target: snap.ids["s2"],
            type_id: "target".into(),
            attrs: BTreeMap::new(),
        }],
        vec![snap.ids["t2"], snap.ids["s2"]],
    );
    user.apply(&mut graph).unwrap();
    cascade.append(user);
    let delta_dir = out_root.join("phase_02_user_delta");
    write_snapshot_triple(&delta_dir.join("00_user_op"), &graph, &opts).unwrap();
    trace.push_str("## Phase 2 — user delta: AddEdge(t2 --target--> s2)\n\n");

    let entries_before = cascade.entries.len();
    let _ = run_cascade_observable(&mut cascade, &mut graph, &rule_refs, 200).unwrap();
    let post_dir = out_root.join("phase_03_post_delta");
    trace.push_str("## Phase 3 — post-delta cascade (observable)\n\n");
    for (i, entry) in cascade.entries.iter().enumerate().skip(entries_before) {
        let name = match &entry.origin {
            Origin::Rule { rule_id } => rule_id.clone(),
            _ => "unknown".into(),
        };
        let step = i - entries_before;
        let dir = post_dir.join(format!("{step:02}_{name}"));
        write_snapshot_triple(&dir, &graph, &opts).unwrap();
        trace.push_str(&format!("### {step:02} {name}\n\n"));
    }

    std::fs::write(out_root.join("trace.md"), trace).unwrap();
}
