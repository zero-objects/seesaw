//! Statischer Ruleset-Validator: Superset-Konflikte erkennen und
//! klassifizieren (2026-07-17, Sandras Superset-Kriterium + die
//! Kontext-Abhängigkeits-Verfeinerung aus dem Case-17-Refactor-Test).
//!
//! ── Hintergrund ────────────────────────────────────────────────────
//! In der deterministischen Engine bestimmt die Rang-Ordnung das
//! Ergebnis NUR bei echten Superset-Paaren (eine Regel matcht alles,
//! was eine andere matcht, plus mehr). Solche Paare sind STATISCH über
//! Pattern-Subsumption erkennbar — der ganze „Backtracking"-Nebel
//! reduziert sich darauf. Dieser Validator findet sie vor dem Lauf.
//!
//! ── Zwei Auflösungs-Klassen (empirisch belegt) ─────────────────────
//! - UNABHÄNGIG: die Subset-Regel braucht KEINEN Kontext, den die
//!   Superset-Regel erst erzeugt ⇒ Auflösung durch REINDEX
//!   („most specific first", Subset höher ranken), NAC-frei.
//!   Beleg: `case05_superset_order.rs`.
//! - KONTEXT-ABHÄNGIG: die Subset-Regel REFERENZIERT ein Erzeugnis der
//!   Superset-Regel (references-Corr auf einen Typ, den die Superset-
//!   Regel etabliert) ⇒ die Superset-Regel MUSS zuerst laufen (für den
//!   Kontext), also reicht Reindex NICHT; ein Ausschluss (NAC o. Ä.)
//!   ist nötig. Beleg: Case 17 (SubClass braucht die Wurzel-Table, die
//!   Class erst erzeugt — der NAC-freie Refactor scheitert nachweisbar).

use seesaw_tgg::rules::format::{CorrDecl, Role, RuleDecl};

// ═══════════════ Subsumption + Kontext-Analyse ═══════════════

/// Echtes Subset (Pattern-Subsumption): `sub` matcht höchstens dort,
/// wo `sup` matcht — gleicher Anker-Typ, jeder sup-Knotentyp ist in
/// sub vorhanden, und sub ist echt restriktiver (mehr Knoten oder mehr
/// value-Constraints).
fn is_proper_subset(sub: &RuleDecl, sup: &RuleDecl) -> bool {
    if sub.left.nodes.first().map(|n| &n.typ) != sup.left.nodes.first().map(|n| &n.typ) {
        return false;
    }
    let mut sub_types: Vec<&String> = sub.left.nodes.iter().map(|n| &n.typ).collect();
    for sn in &sup.left.nodes {
        if let Some(pos) = sub_types.iter().position(|t| **t == sn.typ) {
            sub_types.remove(pos);
        } else {
            return false;
        }
    }
    let sub_c = sub
        .left
        .nodes
        .iter()
        .filter(|n| n.predicate.is_some())
        .count();
    let sup_c = sup
        .left
        .nodes
        .iter()
        .filter(|n| n.predicate.is_some())
        .count();
    sub.left.nodes.len() > sup.left.nodes.len() || sub_c > sup_c
}

fn establishes(r: &RuleDecl) -> impl Iterator<Item = &CorrDecl> {
    r.corrs
        .iter()
        .filter(|c| matches!(c.role, Role::Establishes))
}
fn references(r: &RuleDecl) -> impl Iterator<Item = &CorrDecl> {
    r.corrs
        .iter()
        .filter(|c| matches!(c.role, Role::References))
}

/// Referenziert `sub` ein Erzeugnis von `sup`? Heuristik: `sub` hat
/// einen references-Corr, dessen Typ `sup` per establishes erzeugt.
/// Dann kann `sub` erst matchen, NACHDEM `sup` gelaufen ist.
fn sub_depends_on_sup_output(sub: &RuleDecl, sup: &RuleDecl) -> bool {
    let sup_established: Vec<&String> = establishes(sup).map(|c| &c.typ).collect();
    references(sub).any(|c| sup_established.contains(&&c.typ))
}

#[derive(Debug, PartialEq, Eq)]
enum Resolution {
    /// Ordnung ist bereits korrekt (Subset höher priorisiert).
    OkSpecificFirst,
    /// Unabhängiges Superset-Paar, Superset höher ⇒ Reindex empfohlen.
    NeedsReindex,
    /// Kontext-abhängig ⇒ Reindex reicht nicht, Ausschluss (NAC) nötig.
    NeedsExclusion,
}

#[derive(Debug)]
struct Finding {
    sup: String,
    sub: String,
    resolution: Resolution,
}

/// Prüft ein Ruleset auf Superset-Konflikte und klassifiziert jede
/// Auflösung. Höhere `rank` = höhere Priorität (feuert zuerst).
fn validate(rules: &[RuleDecl]) -> Vec<Finding> {
    let mut out = Vec::new();
    for a in rules {
        for b in rules {
            if a.name == b.name {
                continue;
            }
            // a ist Superset von b?
            if is_proper_subset(b, a) {
                let context_dep = sub_depends_on_sup_output(b, a);
                let sub_higher = b.rank > a.rank;
                let resolution = if context_dep {
                    Resolution::NeedsExclusion
                } else if sub_higher {
                    Resolution::OkSpecificFirst
                } else {
                    Resolution::NeedsReindex
                };
                out.push(Finding {
                    sup: a.name.clone(),
                    sub: b.name.clone(),
                    resolution,
                });
            }
        }
    }
    out
}

// ═══════════════ Test-Rulesets (die beiden echten Fälle) ═══════════════

fn general_rule(rank: u64) -> RuleDecl {
    serde_json::from_value(serde_json::json!({
            "name": "Node_2_Generic", "rank": rank,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Node"}
                ]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Target"}
                ]
            },
            "corrs": [
                {"type": "TargetCorr", "left": "l0", "right": "r0", "role": "establishes"}
            ]
    }))
    .expect("Regel parst")
}
fn special_rule(rank: u64) -> RuleDecl {
    serde_json::from_value(serde_json::json!({
            "name": "Node_2_Special", "rank": rank,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Node"},
                    {"name": "l1", "type": "special", "predicate": {"kind": "equals", "value": "true"}}
                ],
                "links": [["l0", "l1"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Target"}
                ]
            },
            "corrs": [
                {"type": "TargetCorr", "left": "l0", "right": "r0", "role": "establishes"}
            ]
    }))
    .expect("Regel parst")
}

/// Case-17-artiges Paar: Class (Superset) etabliert ClazzCorr;
/// SubClass (Subset) REFERENZIERT ClazzCorr (die Wurzel-Table) —
/// kontext-abhängig.
fn class_rule(rank: u64) -> RuleDecl {
    serde_json::from_value(serde_json::json!({
            "name": "Class_2_Table", "rank": rank,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Clazz"},
                    {"name": "l1", "type": "className"}
                ],
                "links": [["l0", "l1"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Table"}
                ]
            },
            "corrs": [
                {"type": "ClazzCorr", "left": "l0", "right": "r0", "role": "establishes"}
            ]
    }))
    .expect("Regel parst")
}
fn subclass_rule(rank: u64) -> RuleDecl {
    serde_json::from_value(serde_json::json!({
            "name": "SubClass_2_Table", "rank": rank,
            "left": {
                "anchor": "l0",
                "nodes": [
                    {"name": "l0", "type": "Clazz"},
                    {"name": "l1", "type": "Clazz"},
                    {"name": "l2", "type": "className"}
                ],
                "links": [["l0", "l1"], ["l1", "l2"]]
            },
            "right": {
                "anchor": "r0",
                "nodes": [
                    {"name": "r0", "type": "Table"}
                ]
            },
            "corrs": [
                {"type": "ClazzCorr", "left": "l0", "right": "r0", "role": "references"},
                {"type": "ClazzCorr", "left": "l1", "right": "r0", "role": "establishes"}
            ]
    }))
    .expect("Regel parst")
}

// ═══════════════ Tests ═══════════════

#[test]
fn independent_superset_wrong_order_flags_reindex() {
    // general (Superset) höher als special (Subset) ⇒ Überschattung,
    // Reindex empfohlen (kein NAC nötig).
    let rules = vec![general_rule(100), special_rule(90)];
    let f = validate(&rules);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].sup, "Node_2_Generic");
    assert_eq!(f[0].sub, "Node_2_Special");
    assert_eq!(f[0].resolution, Resolution::NeedsReindex);
}

#[test]
fn independent_superset_specific_first_is_ok() {
    // special höher ⇒ korrekt, kein Eingriff nötig.
    let rules = vec![general_rule(90), special_rule(100)];
    let f = validate(&rules);
    assert_eq!(f[0].resolution, Resolution::OkSpecificFirst);
}

#[test]
fn context_dependent_superset_needs_exclusion() {
    // Class/SubClass: SubClass referenziert Class' Erzeugnis (ClazzCorr)
    // ⇒ Reindex reicht NICHT, Ausschluss (NAC) nötig — EGAL welche
    // Rang-Ordnung. Das erklärt, warum der NAC-freie Case-17-Refactor
    // nachweislich scheitert.
    for (rc, rs) in [(900, 850), (850, 950)] {
        let rules = vec![class_rule(rc), subclass_rule(rs)];
        let f = validate(&rules);
        let conflict = f
            .iter()
            .find(|x| x.sub == "SubClass_2_Table")
            .expect("Superset erkannt");
        assert_eq!(
            conflict.resolution,
            Resolution::NeedsExclusion,
            "kontext-abhängig ⇒ NAC nötig (rank {rc}/{rs})"
        );
    }
}

#[test]
fn disjoint_rules_no_finding() {
    let other: RuleDecl = serde_json::from_value(serde_json::json!({
        "name": "Other", "rank": 50,
        "left": {
            "anchor": "l0",
            "nodes": [
                {"name": "l0", "type": "Other"}
            ]
        },
        "right": {
            "anchor": "r0",
            "nodes": [
                {"name": "r0", "type": "X"}
            ]
        },
        "corrs": [
            {"type": "XCorr", "left": "l0", "right": "r0", "role": "establishes"}
        ]
    }))
    .unwrap();
    let rules = vec![general_rule(100), other];
    assert!(
        validate(&rules).is_empty(),
        "disjunkte Regeln: kein Superset-Konflikt"
    );
}
