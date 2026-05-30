//! Cross-module compatibility: the Java-side
//! `RuleSetJsonExporter` produces JSON that the Rust spec
//! deserializer consumes 1:1. The test loads a Java-produced
//! fixture and validates its structure against the `DemoRules`
//! contract.
//!
//! The fixture file is initially produced by the Java test setup.
//! When the Java exporter output changes (e.g. renamed fields), the
//! fixture must be re-exported — these tests then trip and make
//! the incompatibility visible.

use seesaw_tgg::rule::spec::{parse_ruleset, AttrTransform};

const DEMO_FIXTURE: &str = include_str!("fixtures/demo-ruleset.json");

#[test]
fn demo_ruleset_fixture_deserialisiert() {
    let rs = parse_ruleset(DEMO_FIXTURE).expect("fixture parses");
    assert_eq!(rs.name.as_deref(), Some("seesaw-demo"));
    assert_eq!(rs.rules.len(), 4);
}

#[test]
fn demo_rules_haben_erwartete_namen_und_ranks() {
    let rs = parse_ruleset(DEMO_FIXTURE).unwrap();
    let expected = [
        ("R_Class", 40),
        ("R_Attr", 30),
        ("R_Getter", 20),
        ("R_Setter", 10),
    ];
    for (i, (name, rank)) in expected.iter().enumerate() {
        assert_eq!(rs.rules[i].name, *name, "rule {i} name");
        assert_eq!(rs.rules[i].rank, *rank, "rule {i} rank");
    }
}

#[test]
fn r_class_hat_model_als_shared_anchor() {
    let rs = parse_ruleset(DEMO_FIXTURE).unwrap();
    let r_class = &rs.rules[0];
    let l = r_class.l_pattern.as_ref().unwrap();
    let r = r_class.r_pattern.as_ref().unwrap();
    let l_has_m = l.nodes.iter().any(|n| n.id == "m" && n.kind == "Model");
    let r_has_m = r.nodes.iter().any(|n| n.id == "m" && n.kind == "Model");
    assert!(
        l_has_m && r_has_m,
        "R_Class must have the Model node as shared anchor on both sides"
    );
}

#[test]
fn r_class_corr_attr_binding_ist_identity() {
    let rs = parse_ruleset(DEMO_FIXTURE).unwrap();
    let cl = &rs.rules[0].correspondence_links[0];
    assert_eq!(cl.kind.as_deref(), Some("CorrClass"));
    assert_eq!(cl.attribute_bindings.len(), 1);
    let binding = &cl.attribute_bindings[0];
    assert_eq!(binding.l_attr_name, "name");
    assert_eq!(binding.r_attr_name, "name");
    let transform = AttrTransform::parse(binding.transformation.as_deref()).unwrap();
    assert_eq!(transform, AttrTransform::Identity);
}

#[test]
fn r_getter_hat_drei_corrs_und_getter_name_binding() {
    let rs = parse_ruleset(DEMO_FIXTURE).unwrap();
    let r_getter = &rs.rules[2];
    assert_eq!(r_getter.name, "R_Getter");
    assert_eq!(r_getter.correspondence_links.len(), 3);

    // The 3rd corr (CorrGetter) has a `getter_name` binding on name
    let cg = &r_getter.correspondence_links[2];
    assert_eq!(cg.kind.as_deref(), Some("CorrGetter"));
    let name_binding = cg
        .attribute_bindings
        .iter()
        .find(|b| b.l_attr_name == "name")
        .expect("name binding exists");
    let t = AttrTransform::parse(name_binding.transformation.as_deref()).unwrap();
    assert_eq!(t, AttrTransform::GetterName);
    assert_eq!(t.apply("name"), "getName");
    assert_eq!(t.apply_inverse("getName"), "name");
}

#[test]
fn alle_patterns_sind_adjazenz_vollstaendig() {
    // Mirror of the Java test `allDemoRulesHaveAdjacencyClosedPatterns`:
    // for patterns with 2+ nodes, every node must be connected via
    // at least one edge.
    let rs = parse_ruleset(DEMO_FIXTURE).unwrap();
    for r in &rs.rules {
        for (label, pat) in [("l", r.l_pattern.as_ref()), ("r", r.r_pattern.as_ref())] {
            let Some(p) = pat else { continue };
            if p.nodes.len() <= 1 {
                continue;
            }
            let connected: std::collections::HashSet<_> = p
                .edges
                .iter()
                .flat_map(|e| [e.source_node_id.as_str(), e.target_node_id.as_str()])
                .collect();
            for n in &p.nodes {
                assert!(
                    connected.contains(n.id.as_str()),
                    "{}.{}: node '{}' ({}) has no edge — adjacency gap",
                    r.name,
                    label,
                    n.id,
                    n.kind
                );
            }
        }
    }
}

#[test]
fn rcount_sum_ueber_correspondence_links_matcht() {
    // Summary invariant: total number of correspondence_links in the
    // demo rule set — wired explicitly so a silent count change on
    // the Java side becomes visible here.
    let rs = parse_ruleset(DEMO_FIXTURE).unwrap();
    let total: usize = rs.rules.iter().map(|r| r.correspondence_links.len()).sum();
    // R_Class=1, R_Attr=2, R_Getter=3, R_Setter=3
    assert_eq!(total, 1 + 2 + 3 + 3);
}
