//! Case 2 — LMCS2024 Terrace-House higher-order edit.
//!
//! Variante C: alle 3 HouseTypes (Nook/Villa/Cube) mit vollen
//! Construction-Sub-Komponenten (Cellar/Floor/SaddleRoof nach Typ).
//!
//! Pre-Graph: h₁(Nook) → h₂(Villa) → h₃(Cube)
//! User-Delta (Multi-Op): DelEdge(h1→h2,next) + DelEdge(h2→h3,next)
//!                         + SetAttr(h3, type, Villa)
//! Expected-Post: h₁→h₃(Villa), c₂ Tombstone, c₃ re-typed; Floor+SaddleRoof
//! unter c₃ bleiben (Paper-γ).

#[path = "fixtures/house_plan_mm.rs"]
mod house_plan_mm;

use house_plan_mm::{build_fig3a_graph, HouseSnapshot};
use seesaw_tgg::engine::{run_cascade, run_cascade_observable, Cascade, Rule};
use seesaw_tgg::graph::{GhostId, Status, TypedGraph};
use seesaw_tgg::ops::{DeltaEntry, Op, Origin};
use seesaw_tgg::rule::spec::parse_ruleset;
use seesaw_tgg::rule::{compile, instantiate};

const FIXTURE: &str = include_str!("fixtures/rules_lmcs2024_terrace.json");

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
    assert_eq!(rs.rules.len(), 4);
    for r in &rs.rules {
        let _ = compile(r).unwrap_or_else(|e| panic!("Rule {} kompiliert nicht: {e:?}", r.name));
    }
}

#[test]
fn case02_pre_fig3a_has_three_houses() {
    let (_g, snap) = build_fig3a_graph();
    assert_eq!(snap.ids.len(), 3);
}

#[test]
fn case02_initial_sync_produces_r_side() {
    let (mut graph, _snap) = build_fig3a_graph();
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();

    let mut cascade = Cascade::new();
    let term = run_cascade(&mut cascade, &mut graph, &rule_refs, 200).expect("sync");
    eprintln!(
        "Case 2 Initial-Sync terminiert: {term:?}, entries: {}",
        cascade.entries.len()
    );
    for (i, e) in cascade.entries.iter().enumerate() {
        let origin = match &e.origin {
            seesaw_tgg::ops::Origin::Rule { rule_id } => rule_id.clone(),
            _ => "User".into(),
        };
        eprintln!("  step {i}: {origin} ({} ops)", e.op_star.len());
    }

    let counts = |kind: &str| graph.iter_nodes().filter(|n| n.type_id == kind).count();
    eprintln!(
        "Graph-Inventar: Construction={}, Cellar={}, Floor={}, SaddleRoof={}, CorrHouse={}",
        counts("Construction"),
        counts("Cellar"),
        counts("Floor"),
        counts("SaddleRoof"),
        counts("CorrHouse"),
    );

    // Erwartete R-Seite nach Initial-Sync:
    // - 3 Constructions (eine pro House)
    // - 1 Cellar (nur h3=Cube)
    // - 2 Floors (h2=Villa, h3=Cube)
    // - 2 SaddleRoofs (h2=Villa, h3=Cube)
    // - 3 CorrHouses
    assert_eq!(counts("Construction"), 3, "3 Constructions");
    assert_eq!(counts("Cellar"), 1, "1 Cellar (nur Cube)");
    assert_eq!(counts("Floor"), 2, "2 Floors (Villa+Cube)");
    assert_eq!(counts("SaddleRoof"), 2, "2 SaddleRoofs (Villa+Cube)");
    assert_eq!(counts("CorrHouse"), 3, "3 CorrHouses");
}

#[test]
fn case02_probe_nextrule_matching() {
    // Diagnose: Feuert NextRule? Wenn nicht, was matcht / matcht nicht?
    use seesaw_tgg::engine::find_matches;
    let (mut graph, _snap) = build_fig3a_graph();
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();

    // Erst Typ-Rules feuern lassen.
    let mut cascade = Cascade::new();
    let _ = run_cascade(&mut cascade, &mut graph, &rule_refs, 200).unwrap();
    eprintln!(
        "Nach Initial-Sync: {} Entries. Edges im Graph: {}",
        cascade.entries.len(),
        graph.iter_edges().len()
    );

    // NextRule ist die letzte Rule; ihr Pattern inspizieren.
    let next_rule = rules.iter().find(|r| r.id() == "NextRule").unwrap();
    eprintln!(
        "NextRule pattern: {} nodes, {} edges",
        next_rule.pattern().nodes.len(),
        next_rule.pattern().edges.len()
    );
    for n in &next_rule.pattern().nodes {
        eprintln!("  node {} : {}", n.var, n.type_id);
    }
    for e in &next_rule.pattern().edges {
        eprintln!(
            "  edge {} --{}--> {}",
            e.source_var, e.type_id, e.target_var
        );
    }

    let matches = find_matches(next_rule.pattern(), &graph);
    eprintln!("NextRule Matches: {}", matches.len());
    for (i, m) in matches.iter().enumerate() {
        eprintln!("  match {i}: {:?}", m.bindings);
    }
}

fn find_edge_id(graph: &TypedGraph, src: GhostId, tgt: GhostId, edge_type: &str) -> GhostId {
    graph
        .iter_edges()
        .into_iter()
        .find(|(s, t, data)| *s == src && *t == tgt && data.type_id == edge_type)
        .map(|(_, _, data)| data.id)
        .expect("edge exists in Pre-Graph")
}

/// Läuft das volle LMCS2024-Szenario: Initial-Sync, Multi-Delta,
/// Post-Delta-Kaskade.
fn run_full_scenario() -> (TypedGraph, Cascade, HouseSnapshot) {
    let (mut graph, snap) = build_fig3a_graph();
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();

    // Phase 1: Initial-Sync
    let mut cascade = Cascade::new();
    let _ = run_cascade(&mut cascade, &mut graph, &rule_refs, 200).expect("sync");

    // Phase 2: Multi-Delta als ein User-Entry.
    let e_h1_h2 = find_edge_id(&graph, snap.ids["h1"], snap.ids["h2"], "next");
    let e_h2_h3 = find_edge_id(&graph, snap.ids["h2"], snap.ids["h3"], "next");
    let user = DeltaEntry::new_user(
        vec![
            Op::DelEdge { target: e_h1_h2 },
            Op::DelEdge { target: e_h2_h3 },
            Op::DelNode {
                target: snap.ids["h2"],
            },
            Op::SetAttr {
                target: snap.ids["h3"],
                key: "type".into(),
                value: "Villa".into(),
            },
        ],
        vec![snap.ids["h1"], snap.ids["h2"], snap.ids["h3"]],
    );
    user.apply(&mut graph).expect("multi-delta apply");
    cascade.append(user);

    // Phase 3: Post-Delta-Kaskade mit Match-Observability (M5).
    let term = run_cascade_observable(&mut cascade, &mut graph, &rule_refs, 200).expect("post");
    eprintln!("Case 2 Post-Delta (observable) terminiert: {term:?}");

    (graph, cascade, snap)
}

#[test]
fn case02_full_scenario_diagnostic() {
    let (graph, cascade, snap) = run_full_scenario();

    // Diagnose-Log
    eprintln!("Cascade-Entries insgesamt: {}", cascade.entries.len());
    for (i, e) in cascade.entries.iter().enumerate() {
        let origin = match &e.origin {
            Origin::User => "USER".to_string(),
            Origin::Rule { rule_id } => rule_id.clone(),
        };
        eprintln!("  {i}: {origin} ({} ops)", e.op_star.len());
    }

    let active_constr = graph
        .iter_nodes()
        .filter(|n| n.type_id == "Construction" && n.status != Status::Tombstone)
        .count();
    let tomb_constr = graph
        .iter_nodes()
        .filter(|n| n.type_id == "Construction" && n.status == Status::Tombstone)
        .count();
    eprintln!("Constructions: {active_constr} aktiv, {tomb_constr} tombstone");

    for kind in ["Cellar", "Floor", "SaddleRoof"] {
        let active = graph
            .iter_nodes()
            .filter(|n| n.type_id == kind && n.status != Status::Tombstone)
            .count();
        let tomb = graph
            .iter_nodes()
            .filter(|n| n.type_id == kind && n.status == Status::Tombstone)
            .count();
        eprintln!("{kind}: {active} aktiv, {tomb} tombstone");
    }

    let h2 = graph.get_node(&snap.ids["h2"]).unwrap();
    eprintln!("h2.status = {:?}", h2.status);
    let h3 = graph.get_node(&snap.ids["h3"]).unwrap();
    eprintln!(
        "h3.type = {}",
        h3.attrs.get("type").map(|s| s.as_str()).unwrap_or("?")
    );
}

#[test]
fn case02_alpha_post_delta_structure() {
    let (graph, _cascade, snap) = run_full_scenario();
    // α: nach Multi-Delta sollte:
    // - h2 Tombstone
    // - h3.type = Villa
    // - 2 aktive Constructions (für h1 und h3); c2 endgültig Tombstone
    let h2 = graph.get_node(&snap.ids["h2"]).unwrap();
    assert_eq!(h2.status, Status::Tombstone);
    let h3 = graph.get_node(&snap.ids["h3"]).unwrap();
    assert_eq!(h3.attrs.get("type").map(|s| s.as_str()), Some("Villa"));

    let active_constr = graph
        .iter_nodes()
        .filter(|n| n.type_id == "Construction" && n.status != Status::Tombstone)
        .count();
    assert_eq!(
        active_constr, 2,
        "α: 2 aktive Constructions (für h1 und h3)"
    );
}

#[cfg(feature = "regen_graphs")]
#[test]
fn case02_regen_snapshots() {
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
        "case02",
    ]
    .iter()
    .collect();
    let _ = std::fs::remove_dir_all(&out_root);
    std::fs::create_dir_all(&out_root).unwrap();

    let (mut graph, snap) = build_fig3a_graph();
    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let opts = DotOpts::default();
    let mut trace = String::from("# Case 2 — Execution Trace\n\n");
    trace.push_str(
        "Automatisch generiert aus `repro_lmcs2024_terrace.rs::case02_regen_snapshots`.\n\n",
    );

    write_snapshot_triple(&out_root.join("00_initial_l_side"), &graph, &opts).unwrap();
    trace.push_str("## 00 Initial (L-Seite only — h1(Nook) → h2(Villa) → h3(Cube))\n\n");

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

    // Multi-Delta-User-Op
    let e_h1_h2 = find_edge_id(&graph, snap.ids["h1"], snap.ids["h2"], "next");
    let e_h2_h3 = find_edge_id(&graph, snap.ids["h2"], snap.ids["h3"], "next");
    let user = DeltaEntry::new_user(
        vec![
            Op::DelEdge { target: e_h1_h2 },
            Op::DelEdge { target: e_h2_h3 },
            Op::DelNode {
                target: snap.ids["h2"],
            },
            Op::SetAttr {
                target: snap.ids["h3"],
                key: "type".into(),
                value: "Villa".into(),
            },
        ],
        vec![snap.ids["h1"], snap.ids["h2"], snap.ids["h3"]],
    );
    user.apply(&mut graph).unwrap();
    cascade.append(user);
    let delta_dir = out_root.join("phase_02_user_delta");
    write_snapshot_triple(&delta_dir.join("00_user_op"), &graph, &opts).unwrap();
    trace.push_str("## Phase 2 — User-Delta (Multi-Op)\n\n");
    trace.push_str("- DelEdge(next h1→h2)\n- DelEdge(next h2→h3)\n- DelNode(h2)\n- SetAttr(h3, type, Villa)\n\n");

    // Phase 3: Post-Delta mit M5 Match-Observability
    use seesaw_tgg::engine::run_cascade_observable;
    let entries_before = cascade.entries.len();
    let _ = run_cascade_observable(&mut cascade, &mut graph, &rule_refs, 200).unwrap();
    let post_dir = out_root.join("phase_03_post_delta");
    trace.push_str("## Phase 3 — Post-Delta-Kaskade (Match-Observability)\n\n");
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
    if cascade.entries.len() == entries_before {
        trace.push_str(
            "**Keine Engine-Folge-Steps.** Cascade hat nur Konsolidierung durchgeführt.\n\n",
        );
    }

    std::fs::write(out_root.join("trace.md"), trace).unwrap();
}

#[test]
fn case02_gamma_floor_and_roof_under_h3_resurrected() {
    // γ-Beweis Case 2: Floor und SaddleRoof unter c3 bleiben unter
    // dem Re-Typing Cube → Villa stabil (Resurrection durch
    // identische Ghost-IDs zwischen CubeRule- und VillaRule-
    // Produktion). Cellar wird endgültig Tombstone (VillaConstr
    // hat keinen Cellar).

    // Referenz-Lauf: nur Initial-Sync, kein Multi-Delta.
    let (mut ref_graph, _) = build_fig3a_graph();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let _ = run_cascade(&mut Cascade::new(), &mut ref_graph, &refs, 200).unwrap();

    let ref_floor_ids: Vec<seesaw_tgg::graph::GhostId> = ref_graph
        .iter_nodes()
        .filter(|n| n.type_id == "Floor")
        .map(|n| n.id)
        .collect();
    let ref_roof_ids: Vec<seesaw_tgg::graph::GhostId> = ref_graph
        .iter_nodes()
        .filter(|n| n.type_id == "SaddleRoof")
        .map(|n| n.id)
        .collect();
    eprintln!(
        "Ref Floors: {}, Roofs: {}",
        ref_floor_ids.len(),
        ref_roof_ids.len()
    );

    let (graph, _cas, _snap) = run_full_scenario();

    // Mindestens 1 Floor + 1 SaddleRoof aus dem Ref-Lauf müssen
    // im voll Szenario noch aktiv sein (Resurrection für h3).
    let active_floors = ref_floor_ids
        .iter()
        .filter(|id| {
            graph
                .get_node(id)
                .map(|n| n.status != Status::Tombstone)
                .unwrap_or(false)
        })
        .count();
    let active_roofs = ref_roof_ids
        .iter()
        .filter(|id| {
            graph
                .get_node(id)
                .map(|n| n.status != Status::Tombstone)
                .unwrap_or(false)
        })
        .count();
    eprintln!("Aktive Floors aus Ref: {active_floors}, Roofs: {active_roofs}");
    assert!(active_floors >= 1, "γ: mindestens 1 Floor stabil");
    assert!(active_roofs >= 1, "γ: mindestens 1 SaddleRoof stabil");

    // Cellar endgültig Tombstone — VillaConstr hat keinen.
    let active_cellars = graph
        .iter_nodes()
        .filter(|n| n.type_id == "Cellar" && n.status != Status::Tombstone)
        .count();
    assert_eq!(
        active_cellars, 0,
        "γ: Cellar Tombstone (VillaConstr hat keinen)"
    );
}
