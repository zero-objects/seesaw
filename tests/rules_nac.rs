//! M2 Integration-Test: NACs.
//!
//! Root-Rule-With-NAC hat eine NAC "no_incoming_subPackages". Sie
//! darf nur auf Packages ohne eingehende subPackages-Kante matchen.

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
    // Graph mit rootP → p (subPackages-Kante vorhanden)
    // Erwartung: Root-Rule-With-NAC matcht nur auf rootP, nicht auf p,
    // weil p eine incoming subPackages-Kante hat.
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

    // Root-Rule-With-NAC sollte genau 1× feuern (nur für rootP).
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
        "NAC verhindert Rule-Feuern auf p (mit incoming subPackages); nur rootP matcht"
    );
}

#[test]
fn nac_allows_rule_when_pattern_absent() {
    // Graph mit nur rootP (keine subPackages-Kante).
    // Erwartung: Root-Rule-With-NAC feuert.
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
        "NAC trifft nicht → Rule muss feuern"
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
    let err = compile(&rs.rules[0]).expect_err("shared-Anker 'ghost' existiert nicht");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("NacSharedAnchorUnknown") || msg.contains("ghost-anchor"),
        "Error zeigt ghost-anchor-Probleme: {msg}"
    );
}
