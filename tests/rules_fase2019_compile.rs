//! Validates that the 3-rule fixtures parse and compile.
//! If any rule throws a compile error, this test trips
//! immediately — ahead of the actual case tests.

use seesaw_tgg::rule::spec::parse_ruleset;
use seesaw_tgg::rule::{compile, instantiate};

const FIXTURE_3RULE: &str = include_str!("fixtures/rules_fase2019_3rule.json");
const FIXTURE_WITH_SC: &str = include_str!("fixtures/rules_fase2019_3rule_with_sc.json");

#[test]
fn fase2019_3rule_parses_and_compiles() {
    let rs = parse_ruleset(FIXTURE_3RULE).expect("parses");
    assert_eq!(rs.name.as_deref(), Some("fase2019-3rule"));
    assert_eq!(rs.rules.len(), 3);
    for r in &rs.rules {
        let compiled =
            compile(r).unwrap_or_else(|e| panic!("rule {} does not compile: {:?}", r.name, e));
        let _instantiated = instantiate(&compiled);
    }
}

#[test]
fn fase2019_3rule_ranks() {
    let rs = parse_ruleset(FIXTURE_3RULE).unwrap();
    let expected = [("Root-Rule", 30), ("Sub-Rule", 20), ("Leaf-Rule", 10)];
    for (i, (name, rank)) in expected.iter().enumerate() {
        assert_eq!(rs.rules[i].name, *name);
        assert_eq!(rs.rules[i].rank as i32, *rank);
    }
}

#[test]
fn fase2019_with_sc_parses_and_compiles() {
    let rs = parse_ruleset(FIXTURE_WITH_SC).expect("parses");
    assert_eq!(rs.rules.len(), 4);
    for r in &rs.rules {
        let compiled =
            compile(r).unwrap_or_else(|e| panic!("rule {} does not compile: {:?}", r.name, e));
        let _ = instantiate(&compiled);
    }
}

#[test]
fn sc_repair_rank_higher_than_root() {
    let rs = parse_ruleset(FIXTURE_WITH_SC).unwrap();
    let root = rs
        .rules
        .iter()
        .find(|r| r.name == "Root-Rule")
        .unwrap()
        .rank;
    let sc = rs
        .rules
        .iter()
        .find(|r| r.name == "Sub-To-Root-SC-Repair")
        .unwrap()
        .rank;
    assert!(
        sc > root,
        "SC-Repair ({sc}) must have rank > Root-Rule ({root})"
    );
}
