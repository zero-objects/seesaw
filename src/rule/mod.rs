//! Rule-Spec-Layer: deserialisiert RuleSets aus JSON, wie sie
//! `net.sandrakessler.seesaw.core.rules.RuleSetJsonExporter` auf der
//! Java-Seite produziert. In diesem Modul liegt nur die reine
//! Datenstruktur; die Kompilierung Spec → `BasicRule`-Closure ist
//! Aufgabe eines Folge-Moduls (P7b).

pub mod compile;
pub mod demo;
pub mod instantiate;
pub mod spec;

pub use compile::{
    compile, AttrPropagation, CompileError, CompiledRuleSpec, CreationPlan, EdgeSide,
    MatchConstraint, MatchEdge, MatchNode, MatchPlan, NodeOrigin,
};
pub use demo::{demo_rule_compiled, demo_rule_instantiated, DEMO_RULESET_JSON, DEMO_RULE_NAMES};
pub use instantiate::instantiate;
pub use spec::{
    AttrBindingSpec, AttrConstraintSpec, AttrMatcherSpec, CorrespondenceLinkSpec, EdgePatternSpec,
    NegativeApplicationCondition, NodePatternSpec, PatternSpec, RuleSetSpec, RuleSpec,
};
