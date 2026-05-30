//! Rule-spec layer: deserializes RuleSets from JSON as produced by
//! `net.sandrakessler.seesaw.core.rules.RuleSetJsonExporter` on the
//! Java side. This module contains only the pure data structure;
//! compiling a spec into a `BasicRule` closure is the job of a
//! follow-up module (P7b).

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
