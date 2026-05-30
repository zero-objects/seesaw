//! Case 7a/7b/7c — FAOC2021 schema violations (Weidmann/Anjorin 2021).
//!
//! Three sub-cases:
//! - 7a NoTwoGlossaries: for one Doc only one Glossary may emerge.
//!   In Seesaw, automatic via duplication saturation (F11 pillar 1).
//! - 7b NoEmptyClass: an empty Class is not translated. In Seesaw,
//!   via pattern strictness (the rule requires a methods edge in L).
//! - 7c SameNameSameGlossaryEntry: two methods with the same name
//!   should share the same GlossaryEntry. In the current
//!   Seesaw engine **not fully satisfied** — we produce one separate
//!   GlossaryEntry per method. Honest delimitation as a
//!   trade-off versus eMoflon's ILP solution.

#[path = "fixtures/java_javadoc_mm.rs"]
mod java_javadoc_mm;

use java_javadoc_mm::{
    build_doc_two_entries, build_two_classes_one_empty, build_two_methods_same_name,
};
use seesaw_tgg::engine::{run_cascade, Cascade, Rule};
use seesaw_tgg::graph::Status;
use seesaw_tgg::rule::spec::parse_ruleset;
use seesaw_tgg::rule::{compile, instantiate};

const FIXTURE: &str = include_str!("fixtures/rules_faoc2021_schema.json");

fn load_rules() -> Vec<Box<dyn Rule>> {
    let rs = parse_ruleset(FIXTURE).expect("fixture parses");
    rs.rules
        .iter()
        .map(|r| instantiate(&compile(r).expect("compile")))
        .collect()
}

#[test]
fn fixture_parses_and_compiles() {
    let rs = parse_ruleset(FIXTURE).unwrap();
    assert_eq!(rs.rules.len(), 3);
    for r in &rs.rules {
        let _ = compile(r).unwrap_or_else(|e| panic!("rule {} does not compile: {e:?}", r.name));
    }
}

// ── 7a: NoTwoGlossaries ──────────────────────────────────────────────────

#[test]
fn case07a_no_two_glossaries_for_one_doc() {
    let (mut graph, _snap) = build_doc_two_entries();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let mut cas = Cascade::new();
    let _ = run_cascade(&mut cas, &mut graph, &refs, 200).unwrap();

    let glossaries = graph
        .iter_nodes()
        .filter(|n| n.type_id == "Glossary" && n.status != Status::Tombstone)
        .count();
    eprintln!("Case 7a: glossary count = {glossaries}");
    assert_eq!(
        glossaries, 1,
        "7a: NoTwoGlossaries via duplication saturation — exactly 1 Glossary per Doc"
    );

    // How often would DocToGlossary have tried to fire? Only 1×
    // per Doc — the L-pattern matches only once (only 1 Doc).
    let doc_to_glossary_apps = cas
        .entries
        .iter()
        .filter(|e| {
            matches!(&e.origin,
                seesaw_tgg::ops::Origin::Rule { rule_id }
                if rule_id == "DocToGlossary")
        })
        .count();
    assert_eq!(doc_to_glossary_apps, 1);
}

// ── 7b: NoEmptyClass ─────────────────────────────────────────────────────

#[test]
fn case07b_empty_class_not_translated() {
    let (mut graph, _snap) = build_two_classes_one_empty();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let mut cas = Cascade::new();
    let _ = run_cascade(&mut cas, &mut graph, &refs, 200).unwrap();

    // C1 has method M1, so C1 → Doc is translated.
    // C2 has no method, so the ClassWithMethodToDoc pattern does not match.
    let docs = graph
        .iter_nodes()
        .filter(|n| n.type_id == "Doc" && n.status != Status::Tombstone)
        .count();
    eprintln!("Case 7b: doc count = {docs}");
    assert_eq!(
        docs, 1,
        "7b: NoEmptyClass via pattern strictness — only C1 (with method) is translated to Doc"
    );

    let corr_classes = graph
        .iter_nodes()
        .filter(|n| n.type_id == "CorrClass" && n.status != Status::Tombstone)
        .count();
    assert_eq!(corr_classes, 1);
}

// ── 7c: SameNameSameGlossaryEntry ────────────────────────────────────────

#[test]
fn case07c_two_methods_same_name_partial_belegt() {
    // HONEST DELIMITATION: Seesaw's current engine produces TWO
    // GlossaryEntries for two methods with the same name (one per
    // method corr subtree). eMoflon's ILP solution can collapse this
    // to one GlossaryEntry via global optimization. Seesaw does not.
    let (mut graph, _snap) = build_two_methods_same_name();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let mut cas = Cascade::new();
    let _ = run_cascade(&mut cas, &mut graph, &refs, 200).unwrap();

    let glossary_entries = graph
        .iter_nodes()
        .filter(|n| n.type_id == "GlossaryEntry" && n.status != Status::Tombstone)
        .count();
    eprintln!("Case 7c: GlossaryEntry count for 2 methods (same name) = {glossary_entries}");
    // Seesaw produces 2 separate entries (one per method).
    // This is a *partial* solution — weaker than ILP.
    assert_eq!(
        glossary_entries, 2,
        "7c: Seesaw produces 2 separate GlossaryEntries (trade-off vs. ILP)"
    );

    // This is the honest delimitation: Seesaw solves 7a + 7b structurally,
    // but 7c needs eMoflon's global-optimization mechanism.
    // The test makes this separation explicit.
}
