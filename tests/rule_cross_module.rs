//! Cross-Modul-Kompatibilität: das Java-seitige
//! `RuleSetJsonExporter` produziert JSON, das der Rust-Spec-
//! Deserialisierer 1:1 frisst. Der Test lädt ein Java-erzeugtes
//! Fixture und validiert Struktur gegen den `DemoRules`-Kontrakt.
//!
//! Die Fixture-Datei wird initial vom Java-Test-Setup erzeugt. Wenn
//! sich der Java-Exporter-Output ändert (z.B. umbenannte Felder), muss
//! die Fixture neu exportiert werden — dann schlagen diese Tests an
//! und machen die Inkompatibilität sichtbar.

use seesaw_tgg::rule::spec::{parse_ruleset, AttrTransform};

const DEMO_FIXTURE: &str = include_str!("fixtures/demo-ruleset.json");

#[test]
fn demo_ruleset_fixture_deserialisiert() {
    let rs = parse_ruleset(DEMO_FIXTURE).expect("fixture parst");
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
        assert_eq!(rs.rules[i].name, *name, "Rule {i} Name");
        assert_eq!(rs.rules[i].rank, *rank, "Rule {i} Rank");
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
        "R_Class muss den Model-Knoten als shared anchor auf beiden Seiten haben"
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

    // Die 3. Corr (CorrGetter) hat `getter_name`-Binding auf name
    let cg = &r_getter.correspondence_links[2];
    assert_eq!(cg.kind.as_deref(), Some("CorrGetter"));
    let name_binding = cg
        .attribute_bindings
        .iter()
        .find(|b| b.l_attr_name == "name")
        .expect("name-binding existiert");
    let t = AttrTransform::parse(name_binding.transformation.as_deref()).unwrap();
    assert_eq!(t, AttrTransform::GetterName);
    assert_eq!(t.apply("name"), "getName");
    assert_eq!(t.apply_inverse("getName"), "name");
}

#[test]
fn alle_patterns_sind_adjazenz_vollstaendig() {
    // Spiegelbild zum Java-Test `allDemoRulesHaveAdjacencyClosedPatterns`:
    // jeder Knoten ab 2+ Knoten pro Pattern muss durch mindestens eine
    // Kante verbunden sein.
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
                    "{}.{}: Knoten '{}' ({}) ist ohne Kante — Adjazenz-Lücke",
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
    // Summary-Invariante: Gesamt-Anzahl correspondence_links im
    // Demo-RuleSet — explizit verdrahtet, damit eine stille Zählungs-
    // Änderung auf Java-Seite hier sichtbar wird.
    let rs = parse_ruleset(DEMO_FIXTURE).unwrap();
    let total: usize = rs.rules.iter().map(|r| r.correspondence_links.len()).sum();
    // R_Class=1, R_Attr=2, R_Getter=3, R_Setter=3
    assert_eq!(total, 1 + 2 + 3 + 3);
}
