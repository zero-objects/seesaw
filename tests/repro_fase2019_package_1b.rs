//! Case 1b — STTT2021 Package re-parenting mit 3-Rule + SC-Repair
//! (bidirektionale Opt.1-Fassung, 2026-04-24).
//!
//! Identischer Ablauf wie 1a, aber Rule-Set enthält zusätzlich
//! `Sub-To-Root-SC-Repair` (rank 35 > Root 30). Erwartung: Rank-Selektion
//! wählt SC-Repair vor Root-Rule. Der Graph-End-Zustand ist strukturell
//! identisch zu 1a (gleiche Ghost-IDs), nur die Rule-Autorschaft der
//! Corr-Strukturen unterscheidet sich.

#[path = "fixtures/package_folder_mm.rs"]
mod package_folder_mm;

use package_folder_mm::{build_fig3a_graph, SubtreeSnapshot};
use seesaw_tgg::engine::{run_cascade, run_cascade_observable, Cascade, Rule};
use seesaw_tgg::graph::{GhostId, Status, TypedGraph};
use seesaw_tgg::ops::{DeltaEntry, Op, Origin};
use seesaw_tgg::rule::spec::parse_ruleset;
use seesaw_tgg::rule::{compile, instantiate};

const FIXTURE: &str = include_str!("fixtures/rules_fase2019_3rule_with_sc.json");

fn load_rules() -> Vec<Box<dyn Rule>> {
    let rs = parse_ruleset(FIXTURE).expect("fixture parst");
    rs.rules
        .iter()
        .map(|r| instantiate(&compile(r).expect("compile")))
        .collect()
}

fn find_edge_id(graph: &TypedGraph, src: GhostId, tgt: GhostId, edge_type: &str) -> GhostId {
    graph
        .iter_edges()
        .into_iter()
        .find(|(s, t, data)| *s == src && *t == tgt && data.type_id == edge_type)
        .map(|(_, _, data)| data.id)
        .expect("edge exists in Pre-Graph")
}

fn run_case_1b_full_scenario() -> (TypedGraph, Cascade, SubtreeSnapshot) {
    let (mut graph, snap) = build_fig3a_graph();
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();

    let mut cascade = Cascade::new();
    let term = run_cascade(&mut cascade, &mut graph, &rule_refs, 100).expect("sync");
    eprintln!(
        "Case 1b Initial-Sync: {term:?}, entries: {}",
        cascade.entries.len()
    );

    let edge_id = find_edge_id(&graph, snap.ids["rootP"], snap.ids["p"], "subPackages");
    let user = DeltaEntry::new_user(
        vec![Op::DelEdge { target: edge_id }],
        vec![snap.ids["rootP"], snap.ids["p"]],
    );
    user.apply(&mut graph).unwrap();
    cascade.append(user);
    let term =
        run_cascade_observable(&mut cascade, &mut graph, &rule_refs, 100).expect("post-delta");
    eprintln!("Case 1b Post-Delta (observable): {term:?}");
    (graph, cascade, snap)
}

#[test]
fn case01b_alpha_structure_and_content_preserved() {
    let (graph, _cascade, snap) = run_case_1b_full_scenario();

    let edge_id = find_edge_id(&graph, snap.ids["rootP"], snap.ids["p"], "subPackages");
    let edge = graph.get_edge(&edge_id).unwrap();
    assert_eq!(edge.status, Status::Tombstone);

    let docs: Vec<_> = graph
        .iter_nodes()
        .filter(|n| n.type_id == "DocFile")
        .collect();
    let c_doc = docs
        .iter()
        .find(|d| d.attrs.get("content").map(|s| s.as_str()) == Some("c"));
    assert!(c_doc.is_some(), "α: DocFile mit content='c' via Leaf-Rule");

    // α-Struktur-Hierarchie: 2 Folders + 1 subFolders-Edge (siehe 1a).
    let folders: Vec<_> = graph
        .iter_nodes()
        .filter(|n| n.type_id == "Folder")
        .collect();
    assert_eq!(folders.len(), 2);
    let sub_folders_edges: Vec<_> = graph
        .iter_edges()
        .into_iter()
        .filter(|(_, _, e)| e.type_id == "subFolders")
        .collect();
    assert_eq!(sub_folders_edges.len(), 1, "α: genau 1 subFolders-Edge");
}

#[test]
fn case01b_beta_sc_repair_gewaehlt_vor_root() {
    let (_graph, cascade, _snap) = run_case_1b_full_scenario();

    let rule_ids: Vec<String> = cascade
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
    eprintln!("Case 1b — Rule-Sequence: {rule_ids:?}");

    // β-Hauptaussage: SC-Repair erscheint in der Sequence, und zwar
    // vor Root-Rule (falls Root überhaupt feuert).
    assert!(
        rule_ids.contains(&"Sub-To-Root-SC-Repair".to_string()),
        "β: SC-Repair muss mindestens einmal feuern"
    );

    let sc_first = rule_ids.iter().position(|r| r == "Sub-To-Root-SC-Repair");
    let root_first = rule_ids.iter().position(|r| r == "Root-Rule");

    if let (Some(sc), Some(root)) = (sc_first, root_first) {
        assert!(
            sc < root,
            "β: SC-Repair (idx {sc}) vor Root-Rule (idx {root})"
        );
    }
    // Root-Rule darf auch gar nicht feuern — das ist der stärkere
    // Fall („SC-Repair ersetzt Root komplett").
}

#[test]
fn case01b_gamma_merkle_id_stability() {
    // Referenz-Lauf (ohne User-Delta) für R-Seiten-Ghost-IDs
    let (mut ref_graph, _) = build_fig3a_graph();
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let _ = run_cascade(&mut Cascade::new(), &mut ref_graph, &rule_refs, 100).unwrap();
    let r_ids_before: std::collections::HashSet<GhostId> = ref_graph
        .iter_nodes()
        .filter(|n| {
            n.type_id == "Folder" || n.type_id == "DocFile" || n.type_id.starts_with("Corr")
        })
        .map(|n| n.id)
        .collect();

    let (full_graph, _cas, snap) = run_case_1b_full_scenario();

    // γ-L
    for name in ["rootP", "p", "c"] {
        assert!(
            full_graph.get_node(&snap.ids[name]).is_some(),
            "γ-L: {name} stabil"
        );
    }

    // γ-R
    let found = r_ids_before
        .iter()
        .filter(|id| full_graph.get_node(id).is_some())
        .count();
    eprintln!(
        "Case 1b γ-R: {}/{} R-Seiten-IDs stabil",
        found,
        r_ids_before.len()
    );
    assert_eq!(
        found,
        r_ids_before.len(),
        "γ-R: alle R-Seiten-IDs müssen stabil bleiben"
    );
}

#[cfg(feature = "regen_graphs")]
#[test]
fn case01b_regen_snapshots() {
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
        "case01b",
    ]
    .iter()
    .collect();
    let _ = std::fs::remove_dir_all(&out_root);
    std::fs::create_dir_all(&out_root).unwrap();

    let (mut graph, snap) = build_fig3a_graph();
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let opts = DotOpts::default();
    let mut trace = String::from("# Case 1b — Execution Trace\n\n");
    trace.push_str(
        "Automatisch generiert aus `repro_fase2019_package_1b.rs::case01b_regen_snapshots`.\n\n",
    );

    write_snapshot_triple(&out_root.join("00_initial_l_side"), &graph, &opts).unwrap();
    trace.push_str("## 00 Initial (L-Seite only)\n");
    trace.push_str(
        "[source](00_initial_l_side/source.svg) · [target](00_initial_l_side/target.svg) · \
         [corr](00_initial_l_side/corr.svg)\n\n",
    );

    let mut cascade = Cascade::new();
    let sync_dir = out_root.join("phase_01_initial_sync");
    trace.push_str("## Phase 1 — Initial-Sync\n\n");
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
                trace.push_str(&format!(
                    "### {step_idx:02} {name}\n[source](phase_01_initial_sync/{step_idx:02}_{name}/source.svg)\n\n"
                ));
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

    let edge_id = find_edge_id(&graph, snap.ids["rootP"], snap.ids["p"], "subPackages");
    let user = DeltaEntry::new_user(
        vec![Op::DelEdge { target: edge_id }],
        vec![snap.ids["rootP"], snap.ids["p"]],
    );
    user.apply(&mut graph).unwrap();
    cascade.append(user);
    let delta_dir = out_root.join("phase_02_user_delta");
    write_snapshot_triple(&delta_dir.join("00_user_op"), &graph, &opts).unwrap();
    trace.push_str("## Phase 2 — User-Delta\n\n");
    trace.push_str("### 00 user_op\n[source](phase_02_user_delta/00_user_op/source.svg)\n\n");

    trace.push_str("## Phase 3 — Post-Delta-Kaskade\n\n");
    let post_dir = out_root.join("phase_03_post_delta");
    let mut post = 0;
    loop {
        let eb = cascade.entries.len();
        let term = cascade_step(&mut cascade, &mut graph, &rule_refs).unwrap();
        match term {
            TerminationState::Running => {
                let name = match &cascade.entries[eb].origin {
                    Origin::Rule { rule_id } => rule_id.clone(),
                    _ => "unknown".into(),
                };
                let dir = post_dir.join(format!("{post:02}_{name}"));
                write_snapshot_triple(&dir, &graph, &opts).unwrap();
                trace.push_str(&format!("### {post:02} {name}\n\n"));
                post += 1;
                if post > 50 {
                    panic!("overrun");
                }
            }
            other => {
                if post == 0 {
                    trace.push_str(&format!(
                        "**Keine Post-Delta-Steps.** `{other:?}`. Engine saturiert.\n\n"
                    ));
                } else {
                    trace.push_str(&format!("**Post-Terminierung:** `{other:?}`\n\n"));
                }
                break;
            }
        }
    }

    std::fs::write(out_root.join("trace.md"), trace).unwrap();
}
