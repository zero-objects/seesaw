//! Bug #2 reproducer — `#[ignore]` regression net (failing on rc5,
//! to be fixed in rc6).
//!
//! Isolated from the dry-cleaner `sweep_1_roundtrip_per_platform`
//! cascade trace (2026-05-23): two reverse-cascade rules with
//! identical structural R-side output but different R-pattern
//! literal constraints on the *same R-only-creation node* oscillate
//! forever after rc5's `is_duplicate(SetAttr)` fix.
//!
//! Trace excerpt (4000+ steps in dry-cleaner sweep_1):
//! ```
//! rule=R_gitlab_pipeline_jobs_allow_failure_ref_carrier_rev  ops=8
//!   [0-6] identical structural ops
//!   [7]   SetAttr 3d0fcaa8 target_path = "jobs.allow_failure"
//! rule=R_gitlab_pipeline_jobs_backend_ref_carrier_rev        ops=8
//!   [0-6] identical structural ops
//!   [7]   SetAttr 3d0fcaa8 target_path = "jobs.backend"
//! ```
//!
//! Root cause: compile.rs §6 lowers R-pattern literal constraints
//! into `attrs_to_set` regardless of whether the target node is a
//! context node or an R-only-creation node. For creation nodes the
//! literal is an *identity* attribute — it should flow into the
//! GhostId hash (via `collect_r_attrs`), not become a post-creation
//! SetAttr. Two rules that build structurally-identical R-only
//! creation nodes (same corr-parent, same attribute_bindings) end
//! up writing to the *same* GhostId and overwrite each other's
//! `target_path` ad infinitum.
//!
//! Fix path (rc6): split compile.rs §6 into two collections —
//! `attrs_to_set` for context nodes (B5-rc4 semantics, unchanged)
//! and `creation_attrs` for R-only-creation nodes (new, merged
//! into `collect_r_attrs` so they participate in the GhostId hash).
//!
//! HOW TO RUN (when investigating the fix):
//!   cargo test --test repro_rc5_bug2_attrs_to_set_collision \
//!       -- --ignored --nocapture

use std::collections::BTreeMap;

use seesaw_tgg::engine::{cascade_step, Cascade, Rule, TerminationState};
use seesaw_tgg::graph::TypedGraph;
use seesaw_tgg::rule::compile::compile;
use seesaw_tgg::rule::instantiate::instantiate;
use seesaw_tgg::rule::spec::{
    AttrBindingSpec, AttrConstraintSpec, CorrespondenceLinkSpec, NodePatternSpec, PatternSpec,
    RuleSpec,
};

fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// One of two field-style rules. Both have the same L→R structural
/// shape; they differ ONLY in the R-pattern literal `target_path`
/// constraint on the R-only-creation node `tgt`.
fn rule_with_target_path(name: &str, target_path: &str) -> RuleSpec {
    RuleSpec {
        name: name.into(),
        rank: 50,
        documentation: None,
        l_pattern: Some(PatternSpec {
            nodes: vec![NodePatternSpec {
                id: "src".into(),
                kind: "Hub".into(),
                constraints: vec![AttrConstraintSpec::literal("id", "h1")],
            }],
            edges: vec![],
        }),
        r_pattern: Some(PatternSpec {
            nodes: vec![NodePatternSpec {
                id: "tgt".into(),
                kind: "Carrier".into(),
                // The R-only-creation literal. This is what compile
                // lowers into attrs_to_set (post-creation SetAttr).
                // After rc6 it should instead flow into the GhostId
                // via creation_attrs so each rule produces a
                // distinct Carrier node.
                constraints: vec![AttrConstraintSpec::literal("target_path", target_path)],
            }],
            edges: vec![],
        }),
        correspondence_links: vec![CorrespondenceLinkSpec {
            l_node_id: "src".into(),
            r_node_id: "tgt".into(),
            kind: Some("tgg:refines".into()),
            attribute_bindings: vec![AttrBindingSpec {
                l_attr_name: "id".into(),
                r_attr_name: "src_id".into(),
                transformation: Some("identity".into()),
            }],
        }],
        nacs: vec![],
    }
}

#[test]
#[ignore = "rc5 Bug #2: attrs_to_set collision on R-only-creation node — fix in rc6"]
fn two_rules_writing_same_attr_with_different_values_must_not_loop() {
    let rule_a = compile(&rule_with_target_path(
        "Rule_target_path_allow_failure",
        "jobs.allow_failure",
    ))
    .expect("compile A");
    let rule_b = compile(&rule_with_target_path(
        "Rule_target_path_backend",
        "jobs.backend",
    ))
    .expect("compile B");

    let mut g = TypedGraph::new();
    let _src = g.add_baseline_node("Hub", "src-1", attrs(&[("id", "h1")]));

    let r_a: Box<dyn Rule> = instantiate(&rule_a);
    let r_b: Box<dyn Rule> = instantiate(&rule_b);
    let rules: Vec<&dyn Rule> = vec![r_a.as_ref(), r_b.as_ref()];
    let mut cascade = Cascade::new();

    const SAFETY_CAP: usize = 100;
    for step in 0..SAFETY_CAP {
        match cascade_step(&mut cascade, &mut g, &rules).expect("cascade step") {
            TerminationState::Running => continue,
            TerminationState::Convergence | TerminationState::Duplication => {
                let carriers: Vec<_> = g.matchable_nodes_by_kind("Carrier").collect();
                eprintln!(
                    "cascade terminated at step {step}; {} Carrier node(s)",
                    carriers.len()
                );
                // Expected after rc6 fix: TWO Carrier nodes exist,
                // one per rule, each with its own target_path. The
                // GhostId hash must differ because target_path is
                // an identity attribute on a creation node.
                assert_eq!(
                    carriers.len(),
                    2,
                    "after rc6 fix: each rule must produce its own Carrier node, \
                     because target_path is an identity-attribute on a creation node \
                     and belongs in the GhostId hash"
                );
                let paths: Vec<&str> = carriers
                    .iter()
                    .filter_map(|n| n.attrs.get("target_path").map(String::as_str))
                    .collect();
                assert!(
                    paths.contains(&"jobs.allow_failure"),
                    "missing allow_failure carrier, got: {paths:?}"
                );
                assert!(
                    paths.contains(&"jobs.backend"),
                    "missing backend carrier, got: {paths:?}"
                );
                return;
            }
            TerminationState::Contradiction { reason } => panic!("contradiction: {reason}"),
        }
    }
    panic!(
        "BUG #2 (rc5): cascade did not converge in {SAFETY_CAP} steps. Two rules \
         oscillate writing different `target_path` values onto the same Carrier \
         GhostId. Fix in rc6: split compile.rs §6 — R-pattern literals on R-only-\
         creation nodes must flow into the GhostId hash via collect_r_attrs, not \
         become attrs_to_set / SetAttr."
    );
}
