//! Multi-rule reverse-cascade reproducer — das Profil, das der
//! Downstream-Konsument (dry-cleaner) im rc4-Regressionsbericht
//! als „mutmasslich zweiter Bug" markiert hat:
//!
//! > A multi-rule reverse-cascade reproducer where rule A produces
//! > a node that rule B's R-pattern literal targets. That's the
//! > shape the catalog's generated rulesets have after reversal
//! > (construct-rule creates the construct mapping, field-rule's
//! > R-literal needs to stamp `key=…` onto a MappingEntry created
//! > via a synthesised correspondence). I didn't manage to isolate
//! > this into a clean stand-alone test in this session; the dry-
//! > cleaner ruleset does exhibit it.
//!
//! Aufbau hier — beides minimale Pendants zur dry-cleaner-Topologie
//! nach reverse_ruleset (L↔R-Tausch der Vorwärts-Regeln):
//!
//!   ConstructRule:
//!     L: SrcJob[type=construct]
//!     R: TgtMapping        (R-only, via Corr neu erzeugt)
//!     Corr: SrcJob ↔ TgtMapping
//!
//!   FieldRule:
//!     L: SrcField[name=after_script]
//!     R: TgtEntry[key=after_script]   ←── R-Pattern-Literal
//!     Corr: SrcField ↔ TgtEntry
//!
//! Beide Rules emittieren in jedem produce() je eine SetAttr
//! (FieldRule via attrs_to_set; ConstructRule nicht, aber wir
//! geben SrcJob ein `tag` Constraint und Corr-Binding, damit Form-
//! Parität mit dem dry-cleaner-Profil herrscht).
//!
//! Vor dem rc5-Fix (`is_duplicate(SetAttr)`) loopt der Cascade nach
//! Anwendung der FieldRule unbegrenzt: `produce()` emittiert
//! weiterhin SetAttr key=after_script, die Op gilt als nicht-
//! duplikat, jede Iteration ergibt `Running`. Mit Fix terminiert
//! der Cascade nach wenigen Steps mit Convergence/Duplication.

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

fn construct_rule() -> RuleSpec {
    RuleSpec {
        name: "ConstructRule".into(),
        rank: 100,
        documentation: None,
        l_pattern: Some(PatternSpec {
            nodes: vec![NodePatternSpec {
                id: "src".into(),
                kind: "SrcJob".into(),
                constraints: vec![AttrConstraintSpec::literal("type", "construct")],
            }],
            edges: vec![],
        }),
        r_pattern: Some(PatternSpec {
            nodes: vec![NodePatternSpec {
                id: "tgt".into(),
                kind: "TgtMapping".into(),
                constraints: vec![],
            }],
            edges: vec![],
        }),
        correspondence_links: vec![CorrespondenceLinkSpec {
            l_node_id: "src".into(),
            r_node_id: "tgt".into(),
            kind: Some("tgg:refines".into()),
            attribute_bindings: vec![AttrBindingSpec {
                l_attr_name: "type".into(),
                r_attr_name: "type".into(),
                transformation: Some("identity".into()),
            }],
        }],
        nacs: vec![],
    }
}

fn field_rule() -> RuleSpec {
    RuleSpec {
        name: "FieldRule".into(),
        rank: 50,
        documentation: None,
        l_pattern: Some(PatternSpec {
            nodes: vec![NodePatternSpec {
                id: "src".into(),
                kind: "SrcField".into(),
                constraints: vec![AttrConstraintSpec::literal("name", "after_script")],
            }],
            edges: vec![],
        }),
        r_pattern: Some(PatternSpec {
            nodes: vec![NodePatternSpec {
                id: "tgt".into(),
                kind: "TgtEntry".into(),
                // Genau das ist die Stelle, die rc4 als attrs_to_set
                // lowert — und vor dem rc5-Fix in die Endlosschleife
                // im Cascade-Step führte.
                constraints: vec![AttrConstraintSpec::literal("key", "after_script")],
            }],
            edges: vec![],
        }),
        correspondence_links: vec![CorrespondenceLinkSpec {
            l_node_id: "src".into(),
            r_node_id: "tgt".into(),
            kind: Some("tgg:refines".into()),
            attribute_bindings: vec![AttrBindingSpec {
                l_attr_name: "name".into(),
                r_attr_name: "name".into(),
                transformation: Some("identity".into()),
            }],
        }],
        nacs: vec![],
    }
}

#[test]
fn multi_rule_reverse_cascade_terminates() {
    let construct = compile(&construct_rule()).expect("ConstructRule compile");
    let field = compile(&field_rule()).expect("FieldRule compile");

    // Sanity: nur die FieldRule liefert attrs_to_set; die ConstructRule
    // hat kein R-Literal und folglich keinen attrs_to_set-Eintrag.
    assert_eq!(construct.creation_plan.attrs_to_set.len(), 0);
    assert_eq!(field.creation_plan.attrs_to_set.len(), 1);
    eprintln!(
        "field.attrs_to_set = {:#?}",
        field.creation_plan.attrs_to_set
    );

    let mut graph = TypedGraph::new();
    let _src_job = graph.add_baseline_node("SrcJob", "src-job", attrs(&[("type", "construct")]));
    let _src_field =
        graph.add_baseline_node("SrcField", "src-field", attrs(&[("name", "after_script")]));

    let r_construct: Box<dyn Rule> = instantiate(&construct);
    let r_field: Box<dyn Rule> = instantiate(&field);
    let rules: Vec<&dyn Rule> = vec![r_construct.as_ref(), r_field.as_ref()];
    let mut cascade = Cascade::new();

    const SAFETY_CAP: usize = 100;
    for step in 0..SAFETY_CAP {
        match cascade_step(&mut cascade, &mut graph, &rules).expect("cascade step") {
            TerminationState::Running => continue,
            TerminationState::Convergence | TerminationState::Duplication => {
                eprintln!("cascade terminated at step {step}");
                let entries: Vec<_> = graph.matchable_nodes_by_kind("TgtEntry").collect();
                assert_eq!(entries.len(), 1, "exactly one TgtEntry should exist");
                assert_eq!(
                    entries[0].attrs.get("key").map(String::as_str),
                    Some("after_script"),
                    "FieldRule R-Literal must end up on the TgtEntry",
                );
                let mappings: Vec<_> = graph.matchable_nodes_by_kind("TgtMapping").collect();
                assert_eq!(mappings.len(), 1, "exactly one TgtMapping should exist");
                return;
            }
            TerminationState::Contradiction { reason } => panic!("contradiction: {reason}"),
        }
    }
    panic!(
        "REGRESSION (rc4 multi-rule profile): cascade did not converge after \
         {SAFETY_CAP} steps. Either is_duplicate(SetAttr) is still missing, \
         or a second interaction (z.B. apply_attr_propagation, \
         expand_with_retraction) emits a non-idempotent op every step."
    );
}
