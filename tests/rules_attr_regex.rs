//! M1 Integration-Test: Attribute-Matcher + Regex.
//!
//! Validiert parsen, kompilieren und Matcher-Verhalten über alle
//! AttrMatcherSpec-Varianten.

use seesaw_tgg::engine::{find_matches, AttrPredicate, NodePattern, Pattern, Rule};
use seesaw_tgg::graph::TypedGraph;
use seesaw_tgg::rule::spec::{parse_ruleset, AttrMatcherSpec};
use seesaw_tgg::rule::{compile, instantiate};
use std::collections::BTreeMap;

const FIXTURE: &str = include_str!("fixtures/rules_attr_regex_demo.json");

#[test]
fn fixture_parses_and_compiles() {
    let rs = parse_ruleset(FIXTURE).expect("parst");
    assert_eq!(rs.rules.len(), 4);
    for r in &rs.rules {
        let cr = compile(r).unwrap_or_else(|e| panic!("Rule {} kompiliert nicht: {:?}", r.name, e));
        let _boxed: Box<dyn Rule> = instantiate(&cr);
    }
}

#[test]
fn matcher_types_deserialized_correctly() {
    let rs = parse_ruleset(FIXTURE).unwrap();
    // Rule 0 Abstract-Class: regex
    let regex_rule = &rs.rules[0];
    let c = &regex_rule.l_pattern.as_ref().unwrap().nodes[0].constraints[0];
    assert!(matches!(
        c.matcher.clone(),
        AttrMatcherSpec::Regex { pattern } if pattern == "^Abstract"
    ));

    // Rule 1 Get-Method: prefix
    let prefix_rule = &rs.rules[1];
    let c = &prefix_rule.l_pattern.as_ref().unwrap().nodes[0].constraints[0];
    assert!(matches!(
        c.matcher.clone(),
        AttrMatcherSpec::Prefix { prefix } if prefix == "get"
    ));

    // Rule 2 Score: numeric_range
    let range_rule = &rs.rules[2];
    let c = &range_rule.l_pattern.as_ref().unwrap().nodes[0].constraints[0];
    assert!(matches!(
        c.matcher.clone(),
        AttrMatcherSpec::NumericRange {
            min: 0.0,
            max: 100.0
        }
    ));

    // Rule 3 Literal-Flag: neues Matcher-Schema
    let literal_rule = &rs.rules[3];
    let c = &literal_rule.l_pattern.as_ref().unwrap().nodes[0].constraints[0];
    assert!(matches!(
        c.matcher.clone(),
        AttrMatcherSpec::Literal { value } if value == "true"
    ));
}

#[test]
fn regex_matcher_on_real_graph() {
    let mut g = TypedGraph::new();
    let _abstract_base = g.add_baseline_node(
        "Class",
        "AbstractBase",
        [("name".to_string(), "AbstractBase".to_string())]
            .into_iter()
            .collect::<BTreeMap<_, _>>(),
    );
    let _concrete = g.add_baseline_node(
        "Class",
        "ConcreteClass",
        [("name".to_string(), "ConcreteClass".to_string())]
            .into_iter()
            .collect::<BTreeMap<_, _>>(),
    );

    // Pattern mit Regex direkt bauen
    let pat = Pattern::new().with_node({
        let mut np = NodePattern::new("c", "Class");
        np.attr_constraints.push((
            "name".to_string(),
            AttrPredicate::Regex(regex::Regex::new("^Abstract").unwrap()),
        ));
        np
    });

    let matches = find_matches(&pat, &g);
    // Nur AbstractBase matcht
    assert_eq!(matches.len(), 1, "nur 'AbstractBase' matcht ^Abstract");
}

#[test]
fn prefix_matcher_on_real_graph() {
    let mut g = TypedGraph::new();
    let _get_age = g.add_baseline_node(
        "Method",
        "getAge",
        [("name".to_string(), "getAge".to_string())]
            .into_iter()
            .collect(),
    );
    let _set_age = g.add_baseline_node(
        "Method",
        "setAge",
        [("name".to_string(), "setAge".to_string())]
            .into_iter()
            .collect(),
    );
    let _is_active = g.add_baseline_node(
        "Method",
        "isActive",
        [("name".to_string(), "isActive".to_string())]
            .into_iter()
            .collect(),
    );

    let pat = Pattern::new().with_node({
        let mut np = NodePattern::new("m", "Method");
        np.attr_constraints
            .push(("name".to_string(), AttrPredicate::Prefix("get".into())));
        np
    });

    let matches = find_matches(&pat, &g);
    assert_eq!(matches.len(), 1, "nur getAge matcht Prefix 'get'");
}

#[test]
fn numeric_range_matcher_accepts_in_range_rejects_out() {
    let mut g = TypedGraph::new();
    let _valid = g.add_baseline_node(
        "Result",
        "r1",
        [("score".to_string(), "42.5".to_string())]
            .into_iter()
            .collect(),
    );
    let _over = g.add_baseline_node(
        "Result",
        "r2",
        [("score".to_string(), "150.0".to_string())]
            .into_iter()
            .collect(),
    );
    let _not_numeric = g.add_baseline_node(
        "Result",
        "r3",
        [("score".to_string(), "foo".to_string())]
            .into_iter()
            .collect(),
    );

    let pat = Pattern::new().with_node({
        let mut np = NodePattern::new("r", "Result");
        np.attr_constraints.push((
            "score".to_string(),
            AttrPredicate::NumericRange {
                min: 0.0,
                max: 100.0,
            },
        ));
        np
    });

    let matches = find_matches(&pat, &g);
    assert_eq!(matches.len(), 1, "nur r1 matcht Range [0..100]");
}

#[test]
fn literal_matcher_via_tagged_json() {
    // Literal-Variante im neuen Matcher-Schema.
    let json = r#"{
        "name": "literal-test",
        "rules": [{
            "name": "L",
            "rank": 1,
            "l_pattern": {
                "nodes": [{
                    "id": "n",
                    "kind": "Flag",
                    "constraints": [{
                        "name": "on",
                        "matcher": { "type": "literal", "value": "yes" }
                    }]
                }],
                "edges": []
            },
            "r_pattern": null,
            "correspondence_links": []
        }]
    }"#;
    let rs = parse_ruleset(json).unwrap();
    let cr = compile(&rs.rules[0]).unwrap();
    assert_eq!(cr.match_plan.constraints.len(), 1);
    assert!(matches!(
        &cr.match_plan.constraints[0].predicate,
        AttrPredicate::Equals(v) if v == "yes"
    ));
}

#[test]
fn invalid_regex_reports_compile_error() {
    let json = r#"{
        "name": "bad-regex",
        "rules": [{
            "name": "Bad",
            "rank": 1,
            "l_pattern": {
                "nodes": [{
                    "id": "n",
                    "kind": "X",
                    "constraints": [{
                        "name": "x",
                        "matcher": { "type": "regex", "pattern": "(" }
                    }]
                }],
                "edges": []
            },
            "r_pattern": null,
            "correspondence_links": []
        }]
    }"#;
    let rs = parse_ruleset(json).unwrap();
    let err = compile(&rs.rules[0]).expect_err("invalid regex muss scheitern");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("InvalidRegex") || msg.contains("Regex"),
        "Error-Message enthält InvalidRegex: {msg}"
    );
}
