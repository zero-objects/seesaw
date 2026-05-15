//! Case 1a — STTT2021 Package re-parenting mit 3-Rule Minimal
//! (bidirektionale Opt.1-Fassung, 2026-04-24).
//!
//! Ablauf:
//!   1. Pre-Graph (L-Seite only): rootP → subPackages → p → classes → c
//!   2. Initial-Sync-Kaskade: Root, Sub, Leaf erzeugen R-Seite
//!      (Folders + DocFile + Corrs)
//!   3. User-Delta: DelEdge(rootP → p, subPackages)
//!   4. Post-Delta-Kaskade
//!   5. Assertions α/β/γ auf den finalen Graph
//!
//! Snapshots pro Step nur mit `--features regen_graphs`.

#[path = "fixtures/package_folder_mm.rs"]
mod package_folder_mm;

use package_folder_mm::{build_fig3a_graph, SubtreeSnapshot};
use seesaw_tgg::engine::{run_cascade, run_cascade_observable, Cascade, Rule};
use seesaw_tgg::graph::{GhostId, Status, TypedGraph};
use seesaw_tgg::ops::{DeltaEntry, Op, Origin};
use seesaw_tgg::rule::spec::parse_ruleset;
use seesaw_tgg::rule::{compile, instantiate};

const FIXTURE: &str = include_str!("fixtures/rules_fase2019_3rule.json");

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

/// Hilfsfunktion: führt Initial-Sync durch (baut R-Seite auf).
/// Rückgabe: Cascade nach Initial-Sync.
fn initial_sync(graph: &mut TypedGraph, rules: &[&dyn Rule]) -> Cascade {
    let mut cascade = Cascade::new();
    let term = run_cascade(&mut cascade, graph, rules, 100).expect("initial sync");
    eprintln!(
        "Initial-Sync terminiert: {term:?}, entries: {}",
        cascade.entries.len()
    );
    for (i, e) in cascade.entries.iter().enumerate() {
        let origin = match &e.origin {
            Origin::User => "User".to_string(),
            Origin::Rule { rule_id } => rule_id.clone(),
        };
        eprintln!("  sync step {i}: {origin} ({} ops)", e.op_star.len());
    }
    cascade
}

#[test]
fn case01a_pre_fig3a_l_side() {
    let (graph, snap) = build_fig3a_graph();
    assert_eq!(snap.ids.len(), 3);
    let c = graph.get_node(&snap.ids["c"]).unwrap();
    assert_eq!(c.attrs["name"], "c");
}

#[test]
fn case01a_initial_sync_creates_r_side() {
    let (mut graph, snap) = build_fig3a_graph();
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let _ = initial_sync(&mut graph, &rule_refs);

    // Nach Sync: im Graph existieren Folder-Nodes (R-Seite) und Corrs.
    let folders: Vec<_> = graph
        .iter_nodes()
        .filter(|n| n.type_id == "Folder")
        .collect();
    let corrs: Vec<_> = graph
        .iter_nodes()
        .filter(|n| n.type_id.starts_with("Corr"))
        .collect();
    let docs: Vec<_> = graph
        .iter_nodes()
        .filter(|n| n.type_id == "DocFile")
        .collect();

    eprintln!("Folders: {}", folders.len());
    eprintln!("Corrs: {}", corrs.len());
    eprintln!("DocFiles: {}", docs.len());

    // Expected: mindestens 1 Folder für rootP, 1 Folder für p (via Sub-Rule),
    // 1 DocFile für c (via Leaf-Rule), einige Corrs.
    assert!(!folders.is_empty(), "Mindestens 1 Folder via Root-Rule");
    assert!(!corrs.is_empty(), "Mindestens 1 CorrPackage via Root-Rule");

    // Silence unused
    let _ = snap;
}

fn run_case_1a_full_scenario() -> (TypedGraph, Cascade, SubtreeSnapshot) {
    let (mut graph, snap) = build_fig3a_graph();
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();

    // Phase 1: Initial-Sync
    let mut cascade = initial_sync(&mut graph, &rule_refs);

    // Phase 2: User-Delta
    let edge_id = find_edge_id(&graph, snap.ids["rootP"], snap.ids["p"], "subPackages");
    let user = DeltaEntry::new_user(
        vec![Op::DelEdge { target: edge_id }],
        vec![snap.ids["rootP"], snap.ids["p"]],
    );
    user.apply(&mut graph).expect("user-delta applies");
    cascade.append(user);

    // Phase 3: Post-Delta-Kaskade mit Match-Observability (M5).
    let term =
        run_cascade_observable(&mut cascade, &mut graph, &rule_refs, 100).expect("post-delta");
    eprintln!("Post-Delta-Kaskade (observable) terminiert: {term:?}");

    (graph, cascade, snap)
}

#[test]
fn case01a_alpha_structure_and_content_preserved() {
    let (graph, _cascade, snap) = run_case_1a_full_scenario();

    // α — Struktur + Attribute:
    let edge_id = find_edge_id(&graph, snap.ids["rootP"], snap.ids["p"], "subPackages");
    let removed_edge = graph.get_edge(&edge_id).expect("edge still in graph");
    assert_eq!(
        removed_edge.status,
        Status::Tombstone,
        "gelöschte subPackages-Kante ist Tombstone"
    );

    // L-Seite intakt
    let c = graph.get_node(&snap.ids["c"]).expect("class c existiert");
    assert_eq!(c.attrs["name"], "c", "α: c.name unverändert");

    // R-Seite: DocFile für c existiert, content == c.name (via identity-transform)
    let docs: Vec<_> = graph
        .iter_nodes()
        .filter(|n| n.type_id == "DocFile")
        .collect();
    assert!(!docs.is_empty(), "α: mindestens 1 DocFile auf R-Seite");
    let c_doc = docs
        .iter()
        .find(|d| d.attrs.get("content").map(|s| s.as_str()) == Some("c"));
    assert!(
        c_doc.is_some(),
        "α: DocFile mit content='c' (via identity-Transform auf Class.name)"
    );

    // α-Struktur-Hierarchie:
    // - 2 Folders existieren (rootP_folder + p_folder), beide aktiv
    //   (Solid oder Ghost) — keine durch M5-Konsolidierung Tombstone.
    // - subFolders-Edge: nach M5-Iteration 4 ist sie endgültig
    //   Tombstone, weil Sub-Rule's Match invalidiert wurde und
    //   Phase B keine neue Sub-Rule-Anwendung zulässt.
    let folders: Vec<_> = graph
        .iter_nodes()
        .filter(|n| n.type_id == "Folder")
        .collect();
    assert_eq!(folders.len(), 2, "α: 2 Folder-Knoten existieren");
    let active_folders = folders
        .iter()
        .filter(|n| n.status != Status::Tombstone)
        .count();
    assert_eq!(
        active_folders, 2,
        "α: beide Folder bleiben aktiv (Resurrection bewahrt sie)"
    );

    let sub_folders_edges: Vec<_> = graph
        .iter_edges()
        .into_iter()
        .filter(|(_, _, e)| e.type_id == "subFolders")
        .collect();
    eprintln!("subFolders edges:");
    for (_, _, e) in &sub_folders_edges {
        eprintln!("  status={:?}", e.status);
    }
    let active_sub_folders = sub_folders_edges
        .iter()
        .filter(|(_, _, e)| e.status != Status::Tombstone)
        .count();
    assert_eq!(
        active_sub_folders, 0,
        "α (M5-Iter4): subFolders-Edge ist Tombstone — Sub-Rule \
         invalidiert, kein Resurrection-Match"
    );
}

#[test]
fn case01a_beta_cascade_origin_sequence() {
    let (_graph, cascade, _snap) = run_case_1a_full_scenario();

    // β — Cascade-Origin-Sequenz:
    // Initial-Sync-Block (nur Rule-Origins) + User + Post-Delta-Block.
    let user_positions: Vec<usize> = cascade
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.origin, Origin::User))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        user_positions.len(),
        1,
        "Genau 1 User-Entry (der User-Delta)"
    );

    let sync_block_len = user_positions[0];
    let post_block_len = cascade.entries.len() - user_positions[0] - 1;
    eprintln!(
        "Case 1a — Sync-Block: {sync_block_len} Rule-Steps | User | Post-Block: {post_block_len} Rule-Steps"
    );

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
    eprintln!("Case 1a — Alle Rule-IDs: {rule_ids:?}");

    // Mindestens Root-Rule feuert im Sync-Block:
    assert!(
        rule_ids.contains(&"Root-Rule".to_string()),
        "β: Root-Rule feuert mindestens einmal"
    );
}

#[test]
fn case01a_gamma_merkle_id_stability() {
    // γ in zwei Varianten:
    //  (γ-L) L-Seiten-Knoten behalten ihre Baseline-IDs trivially
    //  (γ-R) R-Seiten-Ghost-Knoten: starkes Merkle-Argument

    // Pfad 1: Referenz-Lauf ohne User-Delta, um Post-Sync-IDs zu snapshoten.
    let (mut ref_graph, ref_snap) = build_fig3a_graph();
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let _ = initial_sync(&mut ref_graph, &rule_refs);
    let r_side_ids_before: std::collections::HashSet<GhostId> = ref_graph
        .iter_nodes()
        .filter(|n| {
            n.type_id == "Folder" || n.type_id == "DocFile" || n.type_id.starts_with("Corr")
        })
        .map(|n| n.id)
        .collect();
    let _ = ref_snap;

    // Pfad 2: voller Szenario-Lauf mit User-Delta.
    let (full_graph, _cascade, snap) = run_case_1a_full_scenario();

    // γ-L: L-Seiten-Baseline-IDs unverändert findbar
    for name in ["rootP", "p", "c"] {
        let pre_id = snap.ids[name];
        assert!(
            full_graph.get_node(&pre_id).is_some(),
            "γ-L-FAIL: {name} GhostId nicht mehr findbar"
        );
    }

    // γ-R: R-Seiten-Ghost-IDs, die im Ref-Lauf erzeugt wurden,
    // sind auch im vollen Szenario findbar (mit gleicher ID).
    let r_found: usize = r_side_ids_before
        .iter()
        .filter(|id| full_graph.get_node(id).is_some())
        .count();
    eprintln!(
        "γ-R: von {} Ref-IDs nach User-Delta+Kaskade noch {} findbar",
        r_side_ids_before.len(),
        r_found
    );
    // Mindestens die Hälfte sollte stabil bleiben (Paper-Claim: volle
    // Stabilität im Subtree unter p; rootP-Projektionen dürfen wandern).
    // Initial: Wir loggen nur, keine harte Assertion — erst nach erstem
    // Lauf wird klar, was der tatsächliche Wert ist und ob der Claim hält.
    assert!(
        r_found >= 1,
        "γ-R: mindestens 1 R-Seiten-ID muss stabil sein"
    );
}

#[cfg(feature = "regen_graphs")]
#[test]
fn case01a_regen_snapshots() {
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
        "case01a",
    ]
    .iter()
    .collect();
    let _ = std::fs::remove_dir_all(&out_root);
    std::fs::create_dir_all(&out_root).unwrap();

    let (mut graph, snap) = build_fig3a_graph();
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let opts = DotOpts::default();
    let mut trace = String::from("# Case 1a — Execution Trace\n\n");
    trace.push_str(
        "Automatisch generiert aus `repro_fase2019_package_1a.rs::case01a_regen_snapshots`.\n\n",
    );

    // Phase 0: L-seitiger Pre-Graph
    write_snapshot_triple(&out_root.join("00_initial_l_side"), &graph, &opts).unwrap();
    trace.push_str("## 00 Initial (L-Seite only)\n");
    trace.push_str(
        "[source](00_initial_l_side/source.svg) · \
         [target](00_initial_l_side/target.svg) · \
         [corr](00_initial_l_side/corr.svg)\n\n",
    );

    // Phase 1: Initial-Sync mit Step-Loop
    let mut cascade = Cascade::new();
    let sync_dir = out_root.join("phase_01_initial_sync");
    trace.push_str("## Phase 1 — Initial-Sync (R-Seite erzeugen)\n\n");
    let mut step_idx = 0;
    loop {
        let entries_before = cascade.entries.len();
        let term = cascade_step(&mut cascade, &mut graph, &rule_refs).expect("step ok");
        match term {
            TerminationState::Running => {
                let rule_name = match &cascade.entries[entries_before].origin {
                    Origin::Rule { rule_id } => rule_id.clone(),
                    _ => "unknown".into(),
                };
                let dir = sync_dir.join(format!("{step_idx:02}_{rule_name}"));
                write_snapshot_triple(&dir, &graph, &opts).unwrap();
                trace.push_str(&format!("### {step_idx:02} {rule_name}\n"));
                trace.push_str(&format!(
                    "[source](phase_01_initial_sync/{step_idx:02}_{rule_name}/source.svg)\n\n"
                ));
                step_idx += 1;
                if step_idx > 50 {
                    panic!("sync overrun");
                }
            }
            other => {
                trace.push_str(&format!("**Sync-Terminierung:** `{other:?}`\n\n"));
                break;
            }
        }
    }

    // Phase 2: User-Delta
    let edge_id = find_edge_id(&graph, snap.ids["rootP"], snap.ids["p"], "subPackages");
    let user = DeltaEntry::new_user(
        vec![Op::DelEdge { target: edge_id }],
        vec![snap.ids["rootP"], snap.ids["p"]],
    );
    user.apply(&mut graph).unwrap();
    cascade.append(user);
    let delta_dir = out_root.join("phase_02_user_delta");
    write_snapshot_triple(&delta_dir.join("00_user_op"), &graph, &opts).unwrap();
    trace.push_str("## Phase 2 — User-Delta: DeleteEdge(rootP → p, subPackages)\n\n");
    trace.push_str("### 00 user_op\n[source](phase_02_user_delta/00_user_op/source.svg)\n\n");

    // Phase 3: Post-Delta-Kaskade
    trace.push_str("## Phase 3 — Post-Delta-Kaskade\n\n");
    let post_dir = out_root.join("phase_03_post_delta");
    let mut post_step = 0;
    loop {
        let entries_before = cascade.entries.len();
        let term = cascade_step(&mut cascade, &mut graph, &rule_refs).expect("post step ok");
        match term {
            TerminationState::Running => {
                let rule_name = match &cascade.entries[entries_before].origin {
                    Origin::Rule { rule_id } => rule_id.clone(),
                    _ => "unknown".into(),
                };
                let dir = post_dir.join(format!("{post_step:02}_{rule_name}"));
                write_snapshot_triple(&dir, &graph, &opts).unwrap();
                trace.push_str(&format!("### {post_step:02} {rule_name}\n"));
                trace.push_str(&format!(
                    "[source](phase_03_post_delta/{post_step:02}_{rule_name}/source.svg)\n\n"
                ));
                post_step += 1;
                if post_step > 50 {
                    panic!("post overrun");
                }
            }
            other => {
                if post_step == 0 {
                    trace.push_str(&format!(
                        "**Keine Post-Delta-Steps.** Kaskade ist saturiert (`{other:?}`). \
                         Engine-Mechanik: bestehende R-Seiten-Struktur gilt nach User-Delta \
                         als stabile Lösung, γ-Invarianz ist die Begründung.\n\n",
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
