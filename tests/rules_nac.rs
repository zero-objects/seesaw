//! M2 integration test: NACs.
//!
//! Root-Rule-With-NAC has a NAC "no_incoming_subPackages". It may
//! only match on packages without an incoming subPackages edge.

use seesaw_tgg::engine::{cascade_step, Cascade, Rule, TerminationState};
use seesaw_tgg::graph::{Status, TypedGraph};
use seesaw_tgg::rule::spec::parse_ruleset;
use seesaw_tgg::rule::{compile, instantiate};
use std::collections::BTreeMap;

const FIXTURE: &str = include_str!("fixtures/rules_nac_demo.json");

fn load_rules() -> Vec<Box<dyn Rule>> {
    let rs = parse_ruleset(FIXTURE).unwrap();
    rs.rules
        .iter()
        .map(|r| instantiate(&compile(r).unwrap()))
        .collect()
}

#[test]
fn nac_fixture_parses_and_compiles() {
    let rs = parse_ruleset(FIXTURE).unwrap();
    assert_eq!(rs.rules.len(), 1);
    let rule = &rs.rules[0];
    assert_eq!(rule.nacs.len(), 1);
    assert_eq!(rule.nacs[0].name, "no_incoming_subPackages");
    assert_eq!(rule.nacs[0].shared_with_l, vec!["top".to_string()]);
    let _cr = compile(rule).unwrap();
}

#[test]
fn nac_forbids_rule_when_pattern_matches() {
    // Graph with rootP → p (subPackages edge present).
    // Expectation: Root-Rule-With-NAC matches only on rootP, not on p,
    // because p has an incoming subPackages edge.
    let mut g = TypedGraph::new();
    let root_p = g.add_baseline_node(
        "Package",
        "rootP",
        [("name".to_string(), "rootP".to_string())]
            .into_iter()
            .collect::<BTreeMap<_, _>>(),
    );
    let p = g.add_baseline_node(
        "Package",
        "p",
        [("name".to_string(), "p".to_string())]
            .into_iter()
            .collect(),
    );
    g.add_edge(root_p, p, "subPackages", BTreeMap::new(), Status::Solid);

    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();

    let mut cascade = Cascade::new();
    let mut term_states = Vec::new();
    for _ in 0..10 {
        let term = cascade_step(&mut cascade, &mut g, &rule_refs).unwrap();
        term_states.push(term.clone());
        if !matches!(term, TerminationState::Running) {
            break;
        }
    }

    // Root-Rule-With-NAC should fire exactly once (only for rootP).
    let rule_firings = cascade
        .entries
        .iter()
        .filter(|e| {
            matches!(&e.origin,
                seesaw_tgg::ops::Origin::Rule { rule_id } if rule_id == "Root-Rule-With-NAC")
        })
        .count();
    assert_eq!(
        rule_firings, 1,
        "NAC prevents rule firing on p (with incoming subPackages); only rootP matches"
    );
}

#[test]
fn nac_allows_rule_when_pattern_absent() {
    // Graph with only rootP (no subPackages edge).
    // Expectation: Root-Rule-With-NAC fires.
    let mut g = TypedGraph::new();
    let _root_p = g.add_baseline_node(
        "Package",
        "rootP",
        [("name".to_string(), "rootP".to_string())]
            .into_iter()
            .collect::<BTreeMap<_, _>>(),
    );

    let rules = load_rules();
    let rule_refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();

    let mut cascade = Cascade::new();
    for _ in 0..10 {
        let term = cascade_step(&mut cascade, &mut g, &rule_refs).unwrap();
        if !matches!(term, TerminationState::Running) {
            break;
        }
    }
    assert!(
        !cascade.entries.is_empty(),
        "NAC does not apply → rule must fire"
    );
}

#[test]
fn nac_unknown_shared_anchor_gives_compile_error() {
    let json = r#"{
        "name": "bad-nac",
        "rules": [{
            "name": "R",
            "rank": 1,
            "l_pattern": {
                "nodes": [{ "id": "x", "kind": "X", "constraints": [] }],
                "edges": []
            },
            "r_pattern": null,
            "correspondence_links": [],
            "nacs": [{
                "name": "broken",
                "nodes": [{ "id": "y", "kind": "Y", "constraints": [] }],
                "edges": [],
                "shared_with_l": ["ghost-anchor-that-doesnt-exist"]
            }]
        }]
    }"#;
    let rs = parse_ruleset(json).unwrap();
    let err = compile(&rs.rules[0]).expect_err("shared anchor 'ghost' does not exist");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("NacSharedAnchorUnknown") || msg.contains("ghost-anchor"),
        "error reports ghost-anchor problem: {msg}"
    );
}
