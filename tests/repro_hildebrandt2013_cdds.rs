//! Case 5 — Hildebrandt 2013 CDDS backtracking trilemma.
//!
//! Hildebrandt et al. (Survey 2013) §3 shows: TGG tools exclude
//! classes to avoid backtracking (functional behavior / DEC-1 /
//! look-ahead). The full pathology needs two overlapping rules
//! whose greedy choice ends in a dead end.
//!
//! Because the full CDDS example is not expressible in JSON rule
//! form (it needs special DelNode ops in the production), this
//! test uses **programmatically constructed BasicRules**. This
//! reflects the honest engineering reality — even in the original
//! literature the case is a construct, not a generic model.
//!
//! Setup:
//!   - **R_TableMaker** (rank 30): matches Class c → produces a
//!     Table with kind="primary" as a child of c.
//!   - **R_TableConflict** (rank 5): matches Table with kind="primary"
//!     → tries DelNode on it, which violates V₇ (R_TableMaker is
//!     ancestor).
//!
//! Without rollback: TableMaker fires greedy → TableConflict
//! triggers Contradiction.
//! With rollback: TableMaker is rolled back → TableConflict no
//! longer matches → convergence.

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
    // Without rollback: TableMaker fires (rank 30), then TableConflict
    // triggers Contradiction (V₇: ancestor under attack).
    let mut g = build_class_diagram();
    let (r1, r2) = make_rules();
    let rules: Vec<&dyn Rule> = vec![&r1, &r2];
    let mut cas = Cascade::new();
    let term = run_cascade(&mut cas, &mut g, &rules, 20).unwrap();
    eprintln!("Case 5 without rollback: {term:?}");
    assert!(
        matches!(term, TerminationState::Contradiction { .. }),
        "without rollback we expect Contradiction, got {term:?}"
    );
}

#[test]
fn case05_with_rollback_recovers_to_convergence() {
    // With rollback: TableMaker application is rolled back once
    // TableConflict's Contradiction becomes visible. End state:
    // convergence without Table.
    let mut g = build_class_diagram();
    let base = g.clone();
    let (r1, r2) = make_rules();
    let rules: Vec<&dyn Rule> = vec![&r1, &r2];
    let mut cas = Cascade::new();
    let (term, stats) = run_cascade_with_rollback(&base, &mut cas, &mut g, &rules, 50, 10).unwrap();
    eprintln!(
        "Case 5 with rollback: {term:?}, rollbacks={}",
        stats.rollback_count
    );
    assert!(stats.rollback_count >= 1, "at least 1 rollback expected");
    assert!(
        matches!(
            term,
            TerminationState::Convergence | TerminationState::Duplication
        ),
        "after rollback we expect convergence, got {term:?}"
    );

    // Final graph: no active Table (TableMaker was rolled out)
    let active_tables = g
        .iter_nodes()
        .filter(|n| n.type_id == "Table" && n.status != Status::Tombstone)
        .count();
    assert_eq!(active_tables, 0, "after rollback no active Table remains");
}

#[test]
fn case05_rollback_stats_show_position_limits() {
    // β evidence: rollback stats contain position-scoped
    // rank limits. This is the paper-relevant mechanic —
    // backtracking is not global, but restricted to concrete
    // cascade positions.
    let mut g = build_class_diagram();
    let base = g.clone();
    let (r1, r2) = make_rules();
    let rules: Vec<&dyn Rule> = vec![&r1, &r2];
    let mut cas = Cascade::new();
    let (_, stats) = run_cascade_with_rollback(&base, &mut cas, &mut g, &rules, 50, 10).unwrap();
    eprintln!("Limits: {:?}", stats.limits_applied);
    assert!(!stats.limits_applied.is_empty(), "limits_applied populated");
}
