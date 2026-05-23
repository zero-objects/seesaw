//! Regression reproducer for an rc4 cascade non-termination bug.
//!
//! Triggering condition: a single rule whose R-pattern contains a
//! literal Attr constraint on a freshly created node (i.e. the
//! constraint becomes `attrs_to_set` during compile). On rc3 the
//! rule was rejected as `CreationPlan` lacked the field, so the
//! cascade terminated immediately. rc4 introduces `attrs_to_set` —
//! the production closure emits `Op::SetAttr` for each — but
//! `engine::is_duplicate` was not extended to classify a SetAttr
//! op as a duplicate when the target already carries the same
//! value at the same key. Consequence: every cascade step produces
//! the same SetAttr op, the duplicate predicate returns `false`,
//! the cascade reports `Running`, and `cascade_step` never reaches
//! `Convergence`/`Duplication`. Infinite loop.
//!
//! HOW TO RUN:
//!   cargo test --test repro_rc4_attrs_to_set_loop -- --nocapture
//!
//! EXPECTED on a fix: the cascade terminates within a handful of
//! steps (one productive step for the rule, then a duplicate).
//! CURRENT on rc4: the test's safety counter trips and the test
//! panics with "cascade did not converge after N steps".

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

/// A single rule: `Source[tag=foo]` ↔ `Target` with R-side literal
/// constraint `Target.label = "x"` (must become attrs_to_set during
/// compile) and one corr with span-binding to drive R-node
/// creation. This is the smallest shape that triggers the loop.
fn rule_with_r_literal() -> RuleSpec {
    RuleSpec {
        name: "R_minimal_attrs_to_set".into(),
        rank: 50,
        documentation: Some("repro for rc4 attrs_to_set non-termination".into()),
        l_pattern: Some(PatternSpec {
            nodes: vec![NodePatternSpec {
                id: "L".into(),
                kind: "Source".into(),
                constraints: vec![AttrConstraintSpec::literal("tag", "foo")],
            }],
            edges: vec![],
        }),
        r_pattern: Some(PatternSpec {
            nodes: vec![NodePatternSpec {
                id: "R".into(),
                kind: "Target".into(),
                // This literal — present in R, absent in L — is what
                // compile classifies as `attrs_to_set`. On rc3 the
                // field didn't exist; on rc4 it does and the SetAttr
                // op the production closure emits is what trips the
                // duplicate-check.
                constraints: vec![AttrConstraintSpec::literal("label", "x")],
            }],
            edges: vec![],
        }),
        correspondence_links: vec![CorrespondenceLinkSpec {
            l_node_id: "L".into(),
            r_node_id: "R".into(),
            kind: Some("tgg:refines".into()),
            attribute_bindings: vec![AttrBindingSpec {
                l_attr_name: "tag".into(),
                r_attr_name: "tag".into(),
                transformation: Some("identity".into()),
            }],
        }],
        nacs: vec![],
    }
}

#[test]
fn cascade_terminates_with_r_literal_constraint() {
    let spec = rule_with_r_literal();
    let compiled = compile(&spec).expect("compile");
    eprintln!(
        "compiled.attrs_to_set = {:#?}",
        compiled.creation_plan.attrs_to_set
    );
    assert_eq!(
        compiled.creation_plan.attrs_to_set.len(),
        1,
        "the R-side literal `label=x` should be lowered to one attrs_to_set entry",
    );

    let mut graph = TypedGraph::new();
    let _source = graph.add_baseline_node("Source", "source", attrs(&[("tag", "foo")]));

    let rule_box: Box<dyn Rule> = instantiate(&compiled);
    let rules: Vec<&dyn Rule> = vec![rule_box.as_ref()];
    let mut cascade = Cascade::new();

    // Hard cap: a healthy run terminates within ~5 steps. A loop
    // bug burns through this in seconds.
    const SAFETY_CAP: usize = 50;
    for step in 0..SAFETY_CAP {
        match cascade_step(&mut cascade, &mut graph, &rules).expect("cascade step") {
            TerminationState::Running => continue,
            TerminationState::Convergence | TerminationState::Duplication => {
                eprintln!("cascade terminated at step {step}");
                // Verify the R-node materialised once with label=x.
                let targets: Vec<_> = graph.matchable_nodes_by_kind("Target").collect();
                assert_eq!(targets.len(), 1, "exactly one Target should exist");
                assert_eq!(
                    targets[0].attrs.get("label").map(String::as_str),
                    Some("x"),
                    "the R-literal `label=x` must end up on the materialised Target node",
                );
                return;
            }
            TerminationState::Contradiction { reason } => panic!("contradiction: {reason}"),
        }
    }
    panic!(
        "REGRESSION (rc4): cascade did not converge after {SAFETY_CAP} steps. \
         Rule emits `Op::SetAttr {{ key: label, value: x }}` on every step; \
         `engine::is_duplicate` returns false for SetAttr ops, so the cascade \
         reports `Running` indefinitely. Fix candidate: extend `is_duplicate` \
         to treat `Op::SetAttr {{ target, key, value }}` as a duplicate when \
         `graph.get_node(target).attrs.get(key) == Some(value)`."
    );
}
