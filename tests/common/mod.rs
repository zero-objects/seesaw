//! Ladeweg der Fallstudien.
//!
//! Duenne Huelle um `seesaw_tgg::rules::load_file`: die Fallstudien
//! schreiben ihre Regeln als `serde_json::Value` im selben Format, das
//! auch aus einer Datei kaeme, und gehen durch dieselben drei Schritte.

#![allow(dead_code)]

use seesaw_tgg::graph::Graph;
use seesaw_tgg::plan::DirectedRule;
use seesaw_tgg::rules::format::RuleFile;

fn file(name: &str, rules: Vec<serde_json::Value>) -> RuleFile {
    serde_json::from_value(serde_json::json!({
        "format": 3,
        "name": name,
        "rules": rules,
    }))
    .unwrap_or_else(|e| panic!("Regelsatz {name} parst nicht: {e}"))
}

/// Alle Regeln, beide Richtungen: vorwaerts und rueckwaerts je Regel
/// abwechselnd, in Deklarationsreihenfolge.
pub fn load(name: &str, rules: Vec<serde_json::Value>, g: &mut Graph) -> Vec<DirectedRule> {
    seesaw_tgg::rules::load_file(&file(name, rules), g)
        .unwrap_or_else(|e| panic!("Regelsatz {name}: {e}"))
}

/// Nur die Vorwaertsrichtung. Fuer Fallstudien, deren Regeln
/// rueckwaerts nicht gefahren werden.
pub fn load_forward(name: &str, rules: Vec<serde_json::Value>, g: &mut Graph) -> Vec<DirectedRule> {
    load(name, rules, g).into_iter().step_by(2).collect()
}
