//! Case 5 — Hildebrandt2013 CDDS Backtracking-Trilemma.
//!
//! Hildebrandt et al. (Survey 2013) §3 zeigt: TGG-Tools schließen
//! Klassen aus, um Backtracking zu vermeiden (functional behavior /
//! DEC-1 / look-ahead). Die volle Pathologie braucht zwei
//! sich-überlappende Regeln, deren Greedy-Wahl in eine Sackgasse
//! führt.
//!
//! Da das volle CDDS-Beispiel nicht in einer JSON-Rule-Form
//! ausdrückbar ist (es braucht spezielle DelNode-Ops in der
//! Produktion), nutzt dieser Test **programmatisch konstruierte
//! BasicRules**. Das spiegelt die ehrliche Engineering-Realität
//! wider — auch in der Originalliteratur ist der Case ein
//! Konstrukt, kein generisches Modell.
//!
//! Setup:
//!   - **R_TableMaker** (rank 30): matcht Class c → erzeugt Table
//!     mit kind="primary" als Kind von c.
//!   - **R_TableConflict** (rank 5): matcht Table mit kind="primary"
//!     → versucht DelNode darauf, was V₇ verletzt (R_TableMaker ist
//!     Vorfahre).
//!
//! Ohne Rollback: TableMaker feuert greedy → TableConflict triggert
//! Contradiction.
//! Mit Rollback: TableMaker wird zurückgerollt → TableConflict
//! matcht nicht mehr → Konvergenz.

use seesaw_tgg::engine::{
    run_cascade, run_cascade_with_rollback, BasicRule, Cascade, NodePattern, Pattern, Rule,
    TerminationState,
};
use seesaw_tgg::graph::{Status, TypedGraph};
use seesaw_tgg::ops::Op;
use std::collections::BTreeMap;

fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn build_class_diagram() -> TypedGraph {
    let mut g = TypedGraph::new();
    let _c = g.add_baseline_node("Class", "C", attrs(&[("name", "C")]));
    g
}

fn make_rules() -> (BasicRule, BasicRule) {
    let r_table_maker = BasicRule::new(
        "TableMaker",
        30,
        Pattern::new().with_node(NodePattern::new("c", "Class")),
        |m, _g| {
            let c = *m.get("c").unwrap();
            vec![Op::AddNode {
                parent: c,
                edge_type: "hasTable".into(),
                type_id: "Table".into(),
                attrs: attrs(&[("kind", "primary")]),
            }]
        },
    );
    let mut np_table = NodePattern::new("t", "Table");
    np_table.attr_constraints.push((
        "kind".to_string(),
        seesaw_tgg::engine::AttrPredicate::Equals("primary".into()),
    ));
    let r_table_conflict = BasicRule::new(
        "TableConflict",
        5,
        Pattern::new().with_node(np_table),
        |m, _g| {
            let t = *m.get("t").unwrap();
            vec![Op::DelNode { target: t }]
        },
    );
    (r_table_maker, r_table_conflict)
}

#[test]
fn case05_without_rollback_reaches_contradiction() {
    // Ohne Rollback: TableMaker feuert (rank 30), dann TableConflict
    // triggert Contradiction (V₇: Vorfahre wird angegriffen).
    let mut g = build_class_diagram();
    let (r1, r2) = make_rules();
    let rules: Vec<&dyn Rule> = vec![&r1, &r2];
    let mut cas = Cascade::new();
    let term = run_cascade(&mut cas, &mut g, &rules, 20).unwrap();
    eprintln!("Case 5 ohne Rollback: {term:?}");
    assert!(
        matches!(term, TerminationState::Contradiction { .. }),
        "Ohne Rollback erwarten wir Contradiction, got {term:?}"
    );
}

#[test]
fn case05_with_rollback_recovers_to_convergence() {
    // Mit Rollback: TableMaker-Anwendung wird zurückgerollt, sobald
    // TableConflict's Contradiction sichtbar wird. Endzustand:
    // Konvergenz ohne Table.
    let mut g = build_class_diagram();
    let base = g.clone();
    let (r1, r2) = make_rules();
    let rules: Vec<&dyn Rule> = vec![&r1, &r2];
    let mut cas = Cascade::new();
    let (term, stats) = run_cascade_with_rollback(&base, &mut cas, &mut g, &rules, 50, 10).unwrap();
    eprintln!(
        "Case 5 mit Rollback: {term:?}, rollbacks={}",
        stats.rollback_count
    );
    assert!(stats.rollback_count >= 1, "Mindestens 1 Rollback erwartet");
    assert!(
        matches!(
            term,
            TerminationState::Convergence | TerminationState::Duplication
        ),
        "Nach Rollback Konvergenz erwartet, got {term:?}"
    );

    // Final-Graph: keine aktive Table (TableMaker wurde rausgerollt)
    let active_tables = g
        .iter_nodes()
        .filter(|n| n.type_id == "Table" && n.status != Status::Tombstone)
        .count();
    assert_eq!(
        active_tables, 0,
        "Nach Rollback gibt es keine aktive Table mehr"
    );
}

#[test]
fn case05_rollback_stats_show_position_limits() {
    // β-Beleg: Rollback-Stats enthalten die positions-skopierten
    // Rang-Limits. Das ist die paper-relevante Mechanik —
    // Backtracking ist nicht global, sondern auf konkrete Cascade-
    // Positionen beschränkt.
    let mut g = build_class_diagram();
    let base = g.clone();
    let (r1, r2) = make_rules();
    let rules: Vec<&dyn Rule> = vec![&r1, &r2];
    let mut cas = Cascade::new();
    let (_, stats) = run_cascade_with_rollback(&base, &mut cas, &mut g, &rules, 50, 10).unwrap();
    eprintln!("Limits: {:?}", stats.limits_applied);
    assert!(!stats.limits_applied.is_empty(), "limits_applied gefüllt");
}
