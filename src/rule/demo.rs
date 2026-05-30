//! Demo-rule access: the paper's running example as a
//! `CompiledRuleSpec` and `Box<dyn Rule>`.
//!
//! The JSON fixture under `tests/fixtures/demo-ruleset.json` is
//! embedded via `include_str!`, making the demo RuleSet text the
//! **single source of truth** shared between the Java exporter, the
//! Rust cross-module tests, and the in-binary demo-rule registration.
//!
//! Replaces the old hard-coded `build_demo_rule` factory from
//! `seesaw-jni`.

use crate::engine::Rule;

use super::compile::{compile, CompiledRuleSpec};
use super::instantiate::instantiate;
use super::spec::{parse_ruleset, RuleSetSpec};

/// Demo RuleSet JSON — embedded at compile time from the shared
/// fixture.
pub const DEMO_RULESET_JSON: &str = include_str!("../../tests/fixtures/demo-ruleset.json");

/// Parses the embedded demo RuleSet fixture. Panics if the fixture
/// itself is broken — that would be a build-time bug.
pub fn demo_ruleset_spec() -> RuleSetSpec {
    parse_ruleset(DEMO_RULESET_JSON).expect("embedded demo fixture ist valide")
}

/// Returns the compiled spec of a demo rule by name.
pub fn demo_rule_compiled(name: &str) -> Option<CompiledRuleSpec> {
    let rs = demo_ruleset_spec();
    let r = rs.rules.into_iter().find(|r| r.name == name)?;
    compile(&r).ok()
}

/// Returns a ready-to-use rule instance by name — drop-in replacement
/// for the old `build_demo_rule` factory.
pub fn demo_rule_instantiated(name: &str) -> Option<Box<dyn Rule>> {
    demo_rule_compiled(name).map(|c| instantiate(&c))
}

/// Names of all four paper demo rules in declaration order.
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
