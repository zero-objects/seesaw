//! Case 7a/7b/7c — FAoC2021 Schema-Violations (Weidmann/Anjorin 2021).
//!
//! Drei Sub-Cases:
//! - 7a NoTwoGlossaries: für einen Doc darf nur ein Glossary entstehen.
//!   In Seesaw automatisch durch Duplikations-Saturation (F11 Säule 1).
//! - 7b NoEmptyClass: leere Class wird nicht übersetzt. In Seesaw
//!   durch Pattern-Strenge (Rule verlangt methods-Edge im L-Pattern).
//! - 7c SameNameSameGlossaryEntry: zwei Methods mit gleichem Namen
//!   sollten denselben GlossaryEntry teilen. In aktuellen
//!   Seesaw-Engine **nicht voll erfüllt** — wir erzeugen pro Method
//!   einen separaten GlossaryEntry. Ehrliche Abgrenzung als
//!   Trade-Off gegen eMoflon's ILP-Lösung.

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
    let rs = parse_ruleset(FIXTURE).expect("fixture parst");
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
        let _ = compile(r).unwrap_or_else(|e| panic!("Rule {} kompiliert nicht: {e:?}", r.name));
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
    eprintln!("Case 7a: Anzahl Glossaries = {glossaries}");
    assert_eq!(
        glossaries, 1,
        "7a: NoTwoGlossaries durch Duplikations-Saturation — exakt 1 Glossary für 1 Doc"
    );

    // Wie oft hätte DocToGlossary versucht zu feuern? Nur 1× pro
    // Doc — das L-Pattern matcht nur 1× (nur 1 Doc).
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

    // C1 hat Method M1, also wird C1→Doc übersetzt.
    // C2 hat keine Method, ClassWithMethodToDoc-Pattern matcht nicht.
    let docs = graph
        .iter_nodes()
        .filter(|n| n.type_id == "Doc" && n.status != Status::Tombstone)
        .count();
    eprintln!("Case 7b: Anzahl Docs = {docs}");
    assert_eq!(
        docs, 1,
        "7b: NoEmptyClass durch Pattern-Strenge — nur C1 (mit Method) wird zu Doc übersetzt"
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
    // EHRLICHE ABGRENZUNG: Seesaw's aktuelle Engine erzeugt für zwei
    // Methods mit gleichem Namen ZWEI GlossaryEntries (einen pro
    // Method-Corr-Subtree). eMoflon's ILP-Lösung kann das via globaler
    // Optimierung auf einen GlossaryEntry kollabieren. Seesaw macht
    // das nicht.
    let (mut graph, _snap) = build_two_methods_same_name();
    let rules = load_rules();
    let refs: Vec<&dyn Rule> = rules.iter().map(|r| r.as_ref()).collect();
    let mut cas = Cascade::new();
    let _ = run_cascade(&mut cas, &mut graph, &refs, 200).unwrap();

    let glossary_entries = graph
        .iter_nodes()
        .filter(|n| n.type_id == "GlossaryEntry" && n.status != Status::Tombstone)
        .count();
    eprintln!("Case 7c: Anzahl GlossaryEntries für 2 Methods (gleicher Name) = {glossary_entries}");
    // Seesaw produziert 2 separate Entries (einen pro Method).
    // Das ist *partielle* Lösung — schwächer als ILP.
    assert_eq!(
        glossary_entries, 2,
        "7c: Seesaw erzeugt 2 separate GlossaryEntries (Trade-Off vs. ILP)"
    );

    // Das ist die ehrliche Abgrenzung: Seesaw löst 7a + 7b strukturell,
    // aber 7c braucht eMoflon's Global-Optimization-Mechanismus.
    // Test belegt diese Trennung explizit.
}
