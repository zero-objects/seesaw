//! Demo-Rules-Zugriff: die Paper-Running-Example als
//! `CompiledRuleSpec` und `Box<dyn Rule>`.
//!
//! Die JSON-Fixture unter `tests/fixtures/demo-ruleset.json` wird als
//! `include_str!` eingebettet — damit ist der Demo-RuleSet-Text die
//! **eine Quelle der Wahrheit**, gemeinsam zwischen Java-Exporter,
//! Rust-Cross-Module-Tests und der in-binary Demo-Rule-Registrierung.
//!
//! Ersetzt die alte hart-kodierte `build_demo_rule`-Fabrik aus
//! `seesaw-jni`.

use crate::engine::Rule;

use super::compile::{compile, CompiledRuleSpec};
use super::instantiate::instantiate;
use super::spec::{parse_ruleset, RuleSetSpec};

/// Demo-RuleSet-JSON — eingebettet zum Compile-Zeit aus der shared
/// Fixture.
pub const DEMO_RULESET_JSON: &str = include_str!("../../tests/fixtures/demo-ruleset.json");

/// Parst die eingebettete Demo-RuleSet-Fixture. Panikt, wenn die
/// Fixture selbst kaputt ist — das wäre ein Build-Time-Bug.
pub fn demo_ruleset_spec() -> RuleSetSpec {
    parse_ruleset(DEMO_RULESET_JSON).expect("embedded demo fixture ist valide")
}

/// Liefert die kompilierte Spec einer Demo-Rule per Name.
pub fn demo_rule_compiled(name: &str) -> Option<CompiledRuleSpec> {
    let rs = demo_ruleset_spec();
    let r = rs.rules.into_iter().find(|r| r.name == name)?;
    compile(&r).ok()
}

/// Liefert eine einsatzfertige Rule-Instanz per Name — Drop-in-Replace
/// für die alte `build_demo_rule`-Fabrik.
pub fn demo_rule_instantiated(name: &str) -> Option<Box<dyn Rule>> {
    demo_rule_compiled(name).map(|c| instantiate(&c))
}

/// Namen aller vier Paper-Demo-Rules in Deklarations-Reihenfolge.
pub const DEMO_RULE_NAMES: &[&str] = &["R_Class", "R_Attr", "R_Getter", "R_Setter"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_fixture_parst() {
        let rs = demo_ruleset_spec();
        assert_eq!(rs.rules.len(), 4);
    }

    #[test]
    fn alle_demo_rule_namen_liefern_eine_instanz() {
        for name in DEMO_RULE_NAMES {
            let r = demo_rule_instantiated(name)
                .unwrap_or_else(|| panic!("Demo-Rule '{name}' muss instantiiert werden"));
            assert_eq!(r.id(), *name);
        }
    }

    #[test]
    fn unbekannte_rule_liefert_none() {
        assert!(demo_rule_instantiated("R_DoesNotExist").is_none());
    }

    #[test]
    fn compiled_und_instantiated_sind_konsistent() {
        for name in DEMO_RULE_NAMES {
            let c = demo_rule_compiled(name).unwrap();
            let r = demo_rule_instantiated(name).unwrap();
            assert_eq!(r.id(), c.name);
            assert_eq!(r.rank(), c.rank.max(0) as u64);
        }
    }
}
